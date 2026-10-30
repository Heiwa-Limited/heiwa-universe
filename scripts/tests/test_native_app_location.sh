#!/usr/bin/env bash
set -euo pipefail
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ruby - "$repo_root" <<'RUBY'
require 'tmpdir'
require 'fileutils'
require 'open3'
source = File.read(File.join(ARGV[0], 'apps/heiwa_app/clients/web/install'))
start = source.index('  # One per-user native app.') or abort 'native app install block missing'
finish = source.index("\nfi\n\ncockpit_link=", start) or abort 'native app install block end missing'
block = source[start...finish]
Dir.mktmpdir('heiwa-native-location') do |temporary|
  %w[fresh legacy alias].each do |scenario|
    home = File.join(temporary, scenario)
    app_dir = File.join(home, '.heiwa/app')
    unpacked = File.join(home, 'unpacked')
    canonical = File.join(home, 'Applications/Heiwa.app')
    legacy = File.join(app_dir, 'Heiwa.app')
    FileUtils.mkdir_p([app_dir, File.join(unpacked, 'Heiwa.app/Contents/MacOS')])
    File.write(File.join(unpacked, 'Heiwa.app/Contents/MacOS/Heiwa'), 'new native binary')
    unless scenario == 'fresh'
      FileUtils.mkdir_p(File.join(canonical, 'Contents/MacOS'))
      File.write(File.join(canonical, 'Contents/MacOS/Heiwa'), 'old native binary')
      if scenario == 'alias'
        File.symlink(canonical, legacy)
      else
        FileUtils.mkdir_p(File.join(legacy, 'Contents/MacOS'))
        File.write(File.join(legacy, 'Contents/MacOS/Heiwa'), 'old launcher')
      end
    end
    script = "set -eu\n" + block
    output, error, status = Open3.capture3({'HOME'=>home, 'app_dir'=>app_dir, 'tmp_dir'=>unpacked}, 'sh', stdin_data: script)
    abort "#{scenario} failed: #{output}#{error}" unless status.success?
    abort 'native app not installed in user Applications' unless File.read(File.join(canonical, 'Contents/MacOS/Heiwa')) == 'new native binary'
    abort 'runtime app must be an alias to the canonical app' unless File.symlink?(legacy) && File.realpath(legacy) == File.realpath(canonical)
    if scenario == 'legacy'
      backup = Dir.glob(File.join(app_dir, '.Heiwa-before-canonical.*')).first
      abort 'previous launcher was not recoverable' unless backup && File.read(File.join(backup, 'Contents/MacOS/Heiwa')) == 'old launcher'
    end
  end
end
puts 'Native installer preserves one user app, its runtime alias, and legacy recovery.'
RUBY
