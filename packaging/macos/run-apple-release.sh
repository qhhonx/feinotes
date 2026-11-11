#!/bin/bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Copyright 2026 Fei Notes contributors. See LICENSES.md.
# This entry point runs on a protected release runner. It never reads credentials from Git.
set -euo pipefail
[[ $# == 4 ]] || { echo "Usage: $0 testflight|public-testflight|store|submit|publish VERSION BUILD CONFIRMATION" >&2; exit 2; }
operation=$1
version=$2
build=$3
confirmation=$4
[[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]] || { echo 'Expected a three-component version.' >&2; exit 2; }
[[ "$operation" == publish || "$build" =~ ^[1-9][0-9]{0,3}$ ]] || { echo 'Expected a positive integer build number.' >&2; exit 2; }

repo=$(cd "$(dirname "$0")/../.." && pwd)
cd "$repo"
release=(python3 packaging/macos/apple-release.py)
work=$(mktemp -d "${RUNNER_TEMP:-${TMPDIR:-/tmp}}/fei-apple-release.XXXXXX")
chmod 700 "$work"
keychain=
profile_added=
previous_keychains=()
changed_keychains=
cleanup() {
    if [[ -n "$changed_keychains" ]]; then security list-keychains -d user -s "${previous_keychains[@]}"; fi
    if [[ -n "$keychain" ]]; then security delete-keychain "$keychain" >/dev/null 2>&1 || true; fi
    if [[ -n "$profile_added" ]]; then rm -f "$profile_added"; fi
    rm -rf "$work"
}
trap cleanup EXIT
umask 077
printf '%s' "${ASC_PRIVATE_KEY_B64:?Missing the release API key}" | base64 -D > "$work/asc-key.p8"
export ASC_KEY_PATH="$work/asc-key.p8"
unset ASC_PRIVATE_KEY_B64
: "${ASC_KEY_ID:?}" "${ASC_ISSUER_ID:?}"
case "$operation" in
    submit) "${release[@]}" submit "$version" "$build" --confirm "$confirmation"; exit ;;
    publish) "${release[@]}" publish "$version" --confirm "$confirmation"; exit ;;
    testflight|public-testflight|store) ;;
    *) echo "Unsupported release operation." >&2; exit 2 ;;
esac

if [[ "$operation" == testflight || "$operation" == public-testflight ]]; then
    : "${FEI_BETA_GROUP_ID:?Set the intended internal beta group}"
    package_mode=$operation
    if [[ "$operation" == public-testflight ]]; then : "${FEI_PUBLIC_BETA_GROUP_ID:?Set the external beta group}"; fi
    unset FEI_STOREKIT_ANNUAL_PRODUCT FEI_STOREKIT_MONTHLY_PRODUCT
else
    package_mode=app-store
    : "${FEI_STOREKIT_ANNUAL_PRODUCT:?}" "${FEI_STOREKIT_MONTHLY_PRODUCT:?}"
    : "${FEI_SUPPORT_URL:?}" "${FEI_PRIVACY_URL:?}" "${FEI_TERMS_URL:?}"
fi

existing=$("${release[@]}" check-upload "$version" "$build" "$operation")
if [[ "$existing" == new ]]; then
    : "${FEI_APP_CERT_SHA1:?}" "${FEI_INSTALLER_CERT_SHA1:?}" "${FEI_CERT_PASSWORD:?}"
    for name in FEI_APP_CERT_B64 FEI_INSTALLER_CERT_B64 FEI_STORE_PROFILE_B64; do
        printf '%s' "${!name:?Missing signing material}" | base64 -D > "$work/$name"
        unset "$name"
    done
    while IFS= read -r path; do previous_keychains+=("$path"); done < <(
        security list-keychains -d user | sed -E 's/^[[:space:]]*"//; s/"$//')
    keychain="$work/fei-signing.keychain-db"
    password=$(/usr/bin/openssl rand -hex 24)
    security create-keychain -p "$password" "$keychain"
    security set-keychain-settings -lut 21600 "$keychain"
    security show-keychain-info "$keychain"
    security unlock-keychain -p "$password" "$keychain"
    for name in FEI_APP_CERT_B64 FEI_INSTALLER_CERT_B64; do
        security import "$work/$name" -f pkcs12 -k "$keychain" -P "$FEI_CERT_PASSWORD" \
            -T /usr/bin/codesign -T /usr/bin/productbuild
        rm "$work/$name"
    done
    unset FEI_CERT_PASSWORD
    security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$password" "$keychain" >/dev/null
    security list-keychains -d user -s "$keychain" "${previous_keychains[@]}"
    changed_keychains=1

    # Fail before compilation if signing would require an interactive prompt.
    cp /usr/bin/true "$work/signing-probe"
    python3 - "$FEI_APP_CERT_SHA1" "$keychain" "$work/signing-probe" <<'PYSIGN'
import subprocess
import sys
subprocess.run(["/usr/bin/codesign", "--force", "--sign", sys.argv[1], "--keychain", sys.argv[2],
                "--timestamp=none", sys.argv[3]], timeout=60, check=True)
subprocess.run(["/usr/bin/codesign", "--verify", "--strict", sys.argv[3]], timeout=30, check=True)
print("Noninteractive signing check passed.", flush=True)
PYSIGN

    profile="$work/FEI_STORE_PROFILE_B64"
    uuid=$(security cms -D -i "$profile" | plutil -extract UUID raw -)
    [[ "$uuid" =~ ^[[:xdigit:]-]{36}$ ]]
    profile_dir="$HOME/Library/Developer/Xcode/UserData/Provisioning Profiles"
    mkdir -p "$profile_dir"
    if [[ ! -e "$profile_dir/$uuid.provisionprofile" ]]; then
        cp -n "$profile" "$profile_dir/$uuid.provisionprofile"
        profile_added="$profile_dir/$uuid.provisionprofile"
    fi
    export FEI_SIGNING_KEYCHAIN="$keychain" FEI_CLOUD_CONTAINER=iCloud.app.feinotes.mac
    export FEI_APP_VERSION="$version" FEI_BUILD_NUMBER="$build"
    bash packaging/macos/build-cloud-app.sh "$package_mode" "$profile" "$FEI_APP_CERT_SHA1" "$work/package"

    # Xcode re-signs the app during export. Keep the enclosing work directory private,
    # but ensure the resulting signature and installer payload are readable on customer Macs.
    umask 022
    app="$work/package/Fei Notes.app"
    for destination in export upload; do
        "${release[@]}" archive "$operation" "$app" "$work/$destination" "$profile" \
            "$FEI_APP_CERT_SHA1" "$FEI_INSTALLER_CERT_SHA1" "$destination"
        xcodebuild -exportArchive -archivePath "$work/$destination/Fei Notes.xcarchive" \
            -exportOptionsPlist "$work/$destination/ExportOptions.plist" -exportPath "$work/$destination/result" \
            -authenticationKeyPath "$ASC_KEY_PATH" -authenticationKeyID "$ASC_KEY_ID" \
            -authenticationKeyIssuerID "$ASC_ISSUER_ID"
        if [[ "$destination" == export ]]; then
            pkgutil --expand-full "$work/export/result/Fei Notes.pkg" "$work/expanded"
            payload="$work/expanded/app.feinotes.mac.pkg/Payload/Fei Notes.app"
            test -d "$payload"
            test -z "$(find "$payload" \( -type f ! -perm -004 -o -type d ! -perm -005 \) -print)" \
                || { echo 'Installer payload includes unreadable files or directories.' >&2; exit 1; }
            codesign --verify --deep --strict "$payload"
        fi
    done
fi
"${release[@]}" finish "$version" "$build" "$operation"
