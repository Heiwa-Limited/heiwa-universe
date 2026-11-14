#!/bin/sh
# Signs a built Heiwa.app with a stable local identity so macOS privacy grants
# (Full Disk Access, Automation, Calendars, keychain items) survive rebuilds.
# Ad hoc signatures pin those grants to one build's cdhash; a certificate pins
# them to the identifier plus this certificate. Not for distribution: there is
# no Developer ID and no notarization.
#
#   scripts/sign-macos-local.sh path/to/Heiwa.app ["Heiwa Local Signing"]
set -eu

app=${1:?usage: sign-macos-local.sh path/to/Heiwa.app [identity]}
identity=${2:-${HEIWA_LOCAL_SIGNING_IDENTITY:-Heiwa Local Signing}}
id=ltd.heiwa.desktop

if ! security find-identity -p codesigning | grep -q "\"$identity\""; then
  echo "No code-signing identity named \"$identity\" in the keychain." >&2
  exit 1
fi

sign() { codesign --force --timestamp=none --sign "$identity" "$@"; }

# Inside out: loose Mach-O files, then frameworks, helper apps, and the app.
find "$app/Contents/Resources" -type f -perm -u+x -o -type f -name '*.node' -o -type f -name '*.dylib' |
  while IFS= read -r file; do
    if file -b "$file" | grep -q 'Mach-O'; then
      sign --identifier "$(basename "$file" | sed 's/\.[^.]*$//')" "$file"
    fi
  done
for framework in "$app"/Contents/Frameworks/*.framework; do
  sign --deep "$framework"
done
for helper in "$app"/Contents/Frameworks/*.app; do
  suffix=$(basename "$helper" .app | sed -e 's/^Heiwa Helper//' -e 's/[ ()]//g')
  sign --deep --identifier "$id.helper${suffix:+.$suffix}" "$helper"
done
sign --identifier "$id" "$app"

codesign --verify --deep --strict "$app"
codesign -d -r- "$app" 2>&1 | grep designated
