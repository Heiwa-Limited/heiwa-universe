#!/usr/bin/env ruby
# frozen_string_literal: true

require "yaml"

path = ARGV.fetch(0, ".github/workflows/ci.yml")
workflow = YAML.safe_load(File.read(path), aliases: true)
jobs = workflow.fetch("jobs")
unbounded = jobs.each_with_object([]) do |(job_id, job), missing|
  missing << job_id unless job.is_a?(Hash) && job["timeout-minutes"] == 1
end

unless unbounded.empty?
  warn "every CI job must have a one-minute deadline; missing: #{unbounded.join(', ')}"
  exit 1
end

puts "Every CI job has a one-minute deadline."
