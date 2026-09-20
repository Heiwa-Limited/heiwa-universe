#!/usr/bin/env bash
# Execute the real aggregate with every dependency result; a skipped check fails.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/../.."
ruby - <<'RUBY'
require 'yaml'
require 'open3'
workflow = YAML.safe_load(File.read('.github/workflows/ci.yml'), aliases: true)
jobs = workflow.fetch('jobs')
aggregate = jobs.fetch('rust-source-policy')
raise 'protected status context changed' unless aggregate.fetch('name') == 'Rust Source Policy'
raise 'aggregate must always run' unless aggregate.fetch('if') == 'always()'
step = aggregate.fetch('steps').find { |item| item.key?('run') }
bindings = step.fetch('env').transform_values do |value|
  # GitHub job ids allow digits and underscores, not just [a-z-]. The narrow
  # pattern made this guard raise on any legal job name containing a digit
  # (e.g. system1-engine, l0-acceptance) rather than checking it. Widening the
  # extractor does not weaken the assertion below — every need must still be
  # bound to a result, and only the binding's shape is being parsed here.
  match = value.match(/\A\$\{\{ needs\.([a-z0-9_-]+)\.result \}\}\z/)
  raise "unexpected result binding: #{value}" unless match
  match[1]
end
raise 'every dependency must be bound to a result' unless bindings.values.sort == aggregate.fetch('needs').sort
raise 'sidecar lane must be required' unless bindings.value?('python-tests')
sidecar_runs = jobs.fetch('python-tests').fetch('steps').map { |item| item['run'] }.compact.join("\n")
raise 'shared sidecar gate missing' unless sidecar_runs.include?('bash scripts/check_python_sidecar.sh')
environment = bindings.transform_values { 'success' }
stdout, stderr, status = Open3.capture3(environment, 'bash', '-c', step.fetch('run'))
raise "successful checks rejected: #{stdout}#{stderr}" unless status.success?
bindings.each_key do |key|
  %w[failure cancelled skipped].each do |result|
    _, _, status = Open3.capture3(environment.merge(key => result), 'bash', '-c', step.fetch('run'))
    raise "aggregate accepted #{key}=#{result}" if status.success?
  end
end
puts 'Required aggregate accepts success and rejects failed, cancelled, or skipped dependencies.'
RUBY
