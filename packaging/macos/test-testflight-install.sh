#!/bin/bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Copyright 2026 Fei Notes contributors. See LICENSES.md.
# Optionally pass a genuine installed TestFlight app for positive and tamper checks.
set -euo pipefail
cd "$(dirname "$0")/../.."
work=$(mktemp -d /private/tmp/fei-testflight-signature.XXXXXX)
trap 'rm -rf "$work"' EXIT
cat > "$work/main.swift" <<'SWIFT'
import Foundation
let actual = TestFlightInstall.verified(at: URL(fileURLWithPath: CommandLine.arguments[1]))
let expected = CommandLine.arguments[2] == "true"
guard actual == expected else { fatalError("Unexpected signature verification result: \(actual)") }
print("TESTFLIGHT_SIGNATURE_OK \(CommandLine.arguments[3])")
SWIFT
xcrun swiftc -swift-version 6 -DFEI_TESTFLIGHT_BETA -module-cache-path "$work/cache" \
    packaging/macos/TestFlightInstall.swift "$work/main.swift" -o "$work/verify"
"$work/verify" "$work/missing.app" false missing
mkdir -p "$work/Forged.app/Contents/MacOS"
cp "$work/verify" "$work/Forged.app/Contents/MacOS/verify"
cat > "$work/Forged.app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>CFBundleIdentifier</key><string>app.feinotes.mac</string><key>CFBundleExecutable</key><string>verify</string><key>FeiTestFlightBeta</key><true/></dict></plist>
PLIST
cat > "$work/entitlements.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict><key>beta-reports-active</key><true/><key>com.apple.developer.team-identifier</key><string>95BC6VVXL8</string></dict></plist>
PLIST
"$work/verify" "$work/Forged.app" false unsigned
codesign --force --sign - --entitlements "$work/entitlements.plist" "$work/Forged.app"
"$work/verify" "$work/Forged.app" false forged-beta-entitlements
if [[ $# == 1 ]]; then
    "$work/verify" "$1" true apple-testflight
    ditto "$1" "$work/Tampered.app"
    /usr/libexec/PlistBuddy -c 'Set FeiTestFlightBeta false' "$work/Tampered.app/Contents/Info.plist"
    "$work/verify" "$work/Tampered.app" false modified-plist
    cp "$1/Contents/Info.plist" "$work/Tampered.app/Contents/Info.plist"
    printf '\nmodified' >> "$work/Tampered.app/Contents/Resources/FeiNotes.icns"
    "$work/verify" "$work/Tampered.app" false modified-resource
    ditto "$1" "$work/Nested.app"
    printf '\nmodified' >> "$work/Nested.app/Contents/Frameworks/libQuietCloudRestore.dylib"
    "$work/verify" "$work/Nested.app" false modified-cloud-bridge
fi
