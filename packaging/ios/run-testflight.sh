#!/bin/bash
# SPDX-License-Identifier: MIT
# Copyright 2026 Fei Notes contributors. See LICENSES.md.
set -euo pipefail
[[ $# == 4 ]] || { echo 'Usage: run-testflight.sh SOURCE VERSION BUILD NOTES_DIR' >&2; exit 2; }
source_sha=$1
version=$2
build=$3
notes=$4
[[ "$source_sha" =~ ^[0-9a-f]{40}$ && "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ && "$build" =~ ^[1-9][0-9]{0,3}$ ]]
(( build % 2 == 0 )) || { echo 'iOS TestFlight requires an even build number.' >&2; exit 2; }
repo=$(cd "$(dirname "$0")/../.." && pwd)
cd "$repo"
test "$(git rev-parse HEAD)" = "$source_sha"
# Notes must belong to the selected immutable source, never arbitrary runner files.
export FEI_BETA_NOTES_DIR
FEI_BETA_NOTES_DIR=$(python3 - "$repo" "$notes" <<'PY'
from pathlib import Path
import subprocess
import sys
root = Path(sys.argv[1]).resolve()
notes = (root / sys.argv[2]).resolve()
assert notes.is_relative_to(root), 'Notes must be inside the selected source'
for locale in ('zh-Hans', 'en-US'):
    path = notes / (locale + '.txt')
    assert path.is_file() and not path.is_symlink()
    relative = str(path.relative_to(root))
    original = subprocess.check_output(['git', 'show', 'HEAD:' + relative])
    assert path.read_bytes() == original and 0 < len(original.decode().strip()) <= 3500
print(notes)
PY
)
: "${FEI_PUBLIC_BETA_GROUP_ID:?}" "${FEI_BETA_GROUP_ID:?}"
: "${ASC_KEY_ID:?}" "${ASC_ISSUER_ID:?}" "${FEI_IOS_EXPORT_EXEMPT:?}"
[[ "$FEI_IOS_EXPORT_EXEMPT" == 1 ]] || { echo 'Confirm iOS export compliance before upload.'; exit 1; }
work=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/fei-ios-testflight.XXXXXX")
chmod 700 "$work"
keychain=
profile_added=
changed_keychains=
previous_keychains=()
cleanup() {
    if [[ -n "$changed_keychains" ]]; then security list-keychains -d user -s "${previous_keychains[@]}"; fi
    if [[ -n "$keychain" ]]; then security delete-keychain "$keychain" >/dev/null 2>&1 || true; fi
    if [[ -n "$profile_added" ]]; then rm -f "$profile_added"; fi
    rm -rf "$work"
}
trap cleanup EXIT
umask 077
printf '%s' "${ASC_PRIVATE_KEY_B64:?}" | base64 -D > "$work/api-key.p8"
unset ASC_PRIVATE_KEY_B64
export ASC_KEY_PATH="$work/api-key.p8"
release=(python3 -B packaging/macos/apple-release.py)
existing=$("${release[@]}" check-upload "$version" "$build" public-testflight --platform IOS)
if [[ "$existing" == new ]]; then
    : "${FEI_IOS_APP_CERT_SHA1:?}" "${FEI_IOS_PROFILE_UUID:?}" "${FEI_IOS_CERT_PASSWORD:?}"
    printf '%s' "${FEI_IOS_APP_CERT_B64:?}" | base64 -D > "$work/certificate.p12"
    printf '%s' "${FEI_IOS_STORE_PROFILE_B64:?}" | base64 -D > "$work/store.mobileprovision"
    unset FEI_IOS_APP_CERT_B64 FEI_IOS_STORE_PROFILE_B64
    python3 -B packaging/ios/release.py profile "$work/store.mobileprovision" "$FEI_IOS_APP_CERT_SHA1" "$FEI_IOS_PROFILE_UUID"
    while IFS= read -r path; do previous_keychains+=("$path"); done < <(
        security list-keychains -d user | sed -E 's/^[[:space:]]*"//; s/"$//')
    keychain="$work/ios-signing.keychain-db"
    password=$(/usr/bin/openssl rand -hex 24)
    security create-keychain -p "$password" "$keychain"
    security set-keychain-settings -lut 21600 "$keychain"
    security unlock-keychain -p "$password" "$keychain"
    security import "$work/certificate.p12" -f pkcs12 -k "$keychain" -P "$FEI_IOS_CERT_PASSWORD" -T /usr/bin/codesign >/dev/null
    rm "$work/certificate.p12"
    unset FEI_IOS_CERT_PASSWORD
    "${release[@]}" mask-ci-signing "$work/store.mobileprovision" "$keychain"
    signing_team=$(security cms -D -i "$work/store.mobileprovision" | plutil -extract TeamIdentifier.0 raw -)
    security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$password" "$keychain" >/dev/null
    security list-keychains -d user -s "$keychain" "${previous_keychains[@]}"
    changed_keychains=1
    profile_dir="$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles"
    mkdir -p "$profile_dir"
    profile_path="$profile_dir/$FEI_IOS_PROFILE_UUID.mobileprovision"
    [[ ! -e "$profile_path" ]] || { echo 'Refusing to overwrite an existing profile.'; exit 1; }
    cp "$work/store.mobileprovision" "$profile_path"
    profile_added="$profile_path"
    export FEI_TESTFLIGHT_BETA=1 FEI_IOS_PRODUCT_CLOUD=1 FEI_MOBILE_QA=0
    unset FEI_STOREKIT_ANNUAL_PRODUCT FEI_STOREKIT_MONTHLY_PRODUCT
    bash packaging/ios/prepare-gpui.sh
    python3 -B packaging/ios/prepare-notices.py --check
    (cd packaging/ios && xcodegen generate --spec project.yml)
    xcode_args=(-project packaging/ios/FeiNotesEditorProbe.xcodeproj -scheme FeiNotesTestFlight
        -configuration TestFlight -destination 'generic/platform=iOS'
        -derivedDataPath "$work/derived" MARKETING_VERSION="$version" CURRENT_PROJECT_VERSION="$build"
        DEVELOPMENT_TEAM="$signing_team" CODE_SIGN_STYLE=Manual CODE_SIGN_IDENTITY="$FEI_IOS_APP_CERT_SHA1"
        PROVISIONING_PROFILE="$FEI_IOS_PROFILE_UUID" "OTHER_CODE_SIGN_FLAGS=--keychain $keychain")
    xcodebuild "${xcode_args[@]}" -showBuildSettings -json > "$work/settings.json"
    python3 -B packaging/ios/release.py settings "$work/settings.json"
    umask 022
    xcodebuild -quiet "${xcode_args[@]}" -archivePath "$work/FeiNotes.xcarchive" archive
    app="$work/FeiNotes.xcarchive/Products/Applications/FeiNotes.app"
    python3 -B packaging/ios/release.py verify "$app" "$version" "$build" "$FEI_IOS_APP_CERT_SHA1" "$FEI_IOS_PROFILE_UUID"
    python3 -B packaging/ios/privacy-audit.py "$app"
    for destination in export upload; do
        python3 -B packaging/ios/release.py export-options "$work/store.mobileprovision" "$FEI_IOS_APP_CERT_SHA1" \
            "$FEI_IOS_PROFILE_UUID" "$destination" "$work/ExportOptions.plist"
        xcodebuild -quiet -exportArchive -archivePath "$work/FeiNotes.xcarchive" \
            -exportOptionsPlist "$work/ExportOptions.plist" -exportPath "$work/$destination" \
            -authenticationKeyPath "$ASC_KEY_PATH" -authenticationKeyID "$ASC_KEY_ID" \
            -authenticationKeyIssuerID "$ASC_ISSUER_ID"
        if [[ "$destination" == export ]]; then
            ipa=$(find "$work/export" -maxdepth 1 -name '*.ipa' -type f)
            [[ -n "$ipa" && "$ipa" != *$'\n'* ]]
            ditto -x -k "$ipa" "$work/exported"
            python3 -B packaging/ios/release.py verify "$work/exported/Payload/FeiNotes.app" \
                "$version" "$build" "$FEI_IOS_APP_CERT_SHA1" "$FEI_IOS_PROFILE_UUID"
            python3 -B packaging/ios/privacy-audit.py "$work/exported/Payload/FeiNotes.app"
        fi
    done
fi
# Exact IOS processing, dedicated notes and existing internal/public groups only.
"${release[@]}" finish "$version" "$build" public-testflight --platform IOS
