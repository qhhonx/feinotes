#!/bin/bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Copyright 2026 Fei Notes contributors. See LICENSES.md.
# Build a Fei Notes app and distribution archive. The destination must be a new directory.
set -euo pipefail
umask 022

if [[ $# != 4 ]]; then
    cat >&2 <<'USAGE'
Usage: build-cloud-app.sh development|developer-id|app-store|testflight|public-testflight PROFILE CERT_SHA1 OUTPUT_DIR

app-store and testflight use a Mac App Store provisioning profile, an
application signing certificate fingerprint, and these variables:
  FEI_CLOUD_CONTAINER=iCloud.app.feinotes.mac
  FEI_INSTALLER_CERT_SHA1   Mac Installer Distribution certificate fingerprint
  FEI_BUILD_NUMBER         unused App Store Connect build number
  FEI_APP_VERSION          optional branded version, for example 1.0.0
  FEI_EXPORT_EXEMPT        set to 1 only for a verified export-exempt release
app-store requires FEI_STOREKIT_ANNUAL_PRODUCT and FEI_STOREKIT_MONTHLY_PRODUCT.
testflight omits product IDs and produces FeiNotes-Internal-TestFlight.pkg.
Use public-testflight for external beta review and public invitations.
Optional FEI_SIGNING_KEYCHAIN must contain both signing identities.
FEI_MAC_ARCHS defaults to "arm64 x86_64" (universal); either slice can be selected.
Set FEI_SUPPORT_URL, FEI_PRIVACY_URL and FEI_TERMS_URL to the published
HTTPS pages before public release. Internal builds may omit them.
The App Store package is built and verified locally. Upload is a separate step.
USAGE
    exit 2
fi
channel=$1
mode=$channel
unset FEI_TESTFLIGHT_BETA
if [[ "$channel" == testflight || "$channel" == public-testflight ]]; then
    mode=app-store
    export FEI_TESTFLIGHT_BETA=1
fi
profile=$(cd "$(dirname "$2")" && pwd)/$(basename "$2")
identity=$3
output=$(cd "$(dirname "$4")" && pwd)/$(basename "$4")
repo=$(cd "$(dirname "$0")/../.." && pwd)
case "$mode" in
    development)
        export FEI_CLOUD_ENVIRONMENT=Development
        export FEI_CLOUD_CONTAINER=${FEI_CLOUD_CONTAINER:-iCloud.app.quietmd.cloudlab}
        ;;
    developer-id|app-store)
        export FEI_CLOUD_ENVIRONMENT=Production
        export FEI_CLOUD_CONTAINER=${FEI_CLOUD_CONTAINER:?Set the approved production CloudKit container}
        ;;
    *) echo "Unknown signing mode: $mode" >&2; exit 2 ;;
esac
[[ "$identity" =~ ^[[:xdigit:]]{40}$ && -f "$profile" && ! -e "$output" && ! -L "$output" ]]
installer_identity=${FEI_INSTALLER_CERT_SHA1:-}
if [[ "$mode" == app-store ]]; then
    [[ "$installer_identity" =~ ^[[:xdigit:]]{40}$ ]] || {
        echo 'Set FEI_INSTALLER_CERT_SHA1 to the Mac Installer Distribution certificate fingerprint.' >&2
        exit 2
    }
elif [[ -n "$installer_identity" ]]; then
    echo 'FEI_INSTALLER_CERT_SHA1 requires app-store signing.' >&2
    exit 2
fi
notary_profile=${FEI_NOTARY_PROFILE:-}
notary_key=${FEI_NOTARY_KEY_PATH:-}
notary_key_id=${FEI_NOTARY_KEY_ID:-}
notary_issuer=${FEI_NOTARY_ISSUER_ID:-}
notary_auth=()
if [[ "$mode" == developer-id ]]; then
    if [[ -n "$notary_profile" && -z "$notary_key$notary_key_id$notary_issuer" ]]; then
        notary_auth=(--keychain-profile "$notary_profile")
    elif [[ -z "$notary_profile" && -r "$notary_key" && -n "$notary_key_id" && -n "$notary_issuer" ]]; then
        notary_auth=(--key "$notary_key" --key-id "$notary_key_id" --issuer "$notary_issuer")
    else
        echo 'Set FEI_NOTARY_PROFILE, or set FEI_NOTARY_KEY_PATH, FEI_NOTARY_KEY_ID and FEI_NOTARY_ISSUER_ID.' >&2
        exit 2
    fi
elif [[ -n "$notary_profile$notary_key$notary_key_id$notary_issuer" ]]; then
    echo 'Notarization credentials require developer-id signing.' >&2
    exit 2
fi

work=$(mktemp -d "$(dirname "$output")/.fei-cloud-package.XXXXXX")
trap 'rm -rf "$work"' EXIT
app="$work/package/Fei Notes.app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Frameworks" "$app/Contents/Resources"
security cms -D -i "$profile" -o "$work/profile.plist"
signing_keychain=${FEI_SIGNING_KEYCHAIN:-}
if [[ -n "$signing_keychain" ]]; then
    security find-identity -v -p codesigning "$signing_keychain" > "$work/identities"
else
    security find-identity -v -p codesigning > "$work/identities"
fi
if [[ "$mode" == app-store ]]; then
    if [[ -n "$signing_keychain" ]]; then
        security find-identity -v "$signing_keychain" > "$work/installer-identities"
    else
        security find-identity -v > "$work/installer-identities"
    fi
fi
cd "$repo"
version=${FEI_APP_VERSION:-$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml)}
python3 - "$work" "$app" "$mode" "$identity" "$version" "$installer_identity" <<'PY'
import datetime
import hashlib
import os
from pathlib import Path
import plistlib
import re
import sys
from urllib.parse import urlsplit

work, app = map(Path, sys.argv[1:3])
mode, fingerprint, version, installer_fingerprint = sys.argv[3:]
environment = os.environ["FEI_CLOUD_ENVIRONMENT"]
container = os.environ["FEI_CLOUD_CONTAINER"]
profile = plistlib.loads((work / "profile.plist").read_bytes())
allowed = profile["Entitlements"]
def require(condition, detail):
    if not condition:
        raise SystemExit(detail)

require("OSX" in profile.get("Platform", []), "Use a macOS provisioning profile.")
require(profile["ExpirationDate"] > datetime.datetime.now(datetime.timezone.utc).replace(tzinfo=None),
        "The provisioning profile has expired.")
require(fingerprint.lower() in {hashlib.sha1(cert).hexdigest() for cert in profile["DeveloperCertificates"]},
        "This signing certificate is not in the provisioning profile.")
match = re.search(r'^\s*\d+\) ' + re.escape(fingerprint) + r' "([^"]+)"',
                  (work / "identities").read_text(), re.IGNORECASE | re.MULTILINE)
prefixes = {"development": ("Apple Development", "Mac Developer"),
            "developer-id": ("Developer ID Application",),
            "app-store": ("Apple Distribution", "3rd Party Mac Developer Application")}
require(match and any(match[1] == prefix or match[1].startswith(prefix + ":") for prefix in prefixes[mode]),
        "The keychain needs a valid identity for the selected signing mode.")
if mode == "development":
    require(profile.get("ProvisionedDevices"), "Use a device-limited development profile.")
elif mode == "developer-id":
    require(profile.get("ProvisionsAllDevices") is True, "Use a Developer ID distribution profile.")
else:
    require(not profile.get("ProvisionedDevices") and not profile.get("ProvisionsAllDevices"),
            "Use a Mac App Store distribution profile.")

application_id = allowed["com.apple.application-identifier"]
prefix, _, bundle_id = application_id.partition(".")
team = allowed["com.apple.developer.team-identifier"]
require(prefix in profile["ApplicationIdentifierPrefix"] and team in profile["TeamIdentifier"],
        "The profile's App ID prefix or team is inconsistent.")
require(re.fullmatch(r"[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)+", bundle_id), "Use an explicit App ID.")
require(mode == "development" or (bundle_id != "app.quietmd.cloudlab" and container != "iCloud.app.quietmd.cloudlab"),
        "Production requires a separate release App ID and CloudKit container.")
if mode == "app-store":
    require((bundle_id, container) == ("app.feinotes.mac", "iCloud.app.feinotes.mac"),
            "Use the Fei Notes App Store App ID and production CloudKit container.")
    installer_identities = re.findall(r'^\s*\d+\) ([0-9A-F]{40}) "([^"]+)"',
                                      (work / "installer-identities").read_text(), re.IGNORECASE | re.MULTILINE)
    installer_names = {name for digest, name in installer_identities if digest.lower() == installer_fingerprint.lower()}
    require(len(installer_names) == 1, "The keychain needs the selected Mac Installer Distribution identity.")
    installer_name = next(iter(installer_names))
    require(installer_name.startswith("3rd Party Mac Developer Installer: "),
            "The package requires a Mac Installer Distribution certificate.")
    require(installer_name.endswith(f" ({team})") or installer_name == f"3rd Party Mac Developer Installer: {team}",
            "The Installer certificate team does not match the provisioning profile.")
    require({digest.lower() for digest, name in installer_identities if name == installer_name}
            == {installer_fingerprint.lower()}, "Use a signing keychain with only one identity for this Installer certificate name.")
    (work / "installer-name").write_text(installer_name)
container_key = "com.apple.developer.icloud-container-identifiers"
environment_key = "com.apple.developer.icloud-container-environment"
services_key = "com.apple.developer.icloud-services"
require(re.fullmatch(r"iCloud\.[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*", container)
        and container in allowed.get(container_key, []), "The profile does not authorize the requested CloudKit container.")
environments = allowed.get(environment_key, [])
if isinstance(environments, str):
    environments = [environments]
require(environment in environments, "The profile does not authorize the requested CloudKit environment.")
services = allowed.get(services_key, [])
require(services == "*" or "CloudKit" in services, "The profile does not authorize CloudKit.")
entitlements = {
    "com.apple.application-identifier": application_id,
    "com.apple.developer.team-identifier": team,
    container_key: [container],
    environment_key: environment,
    services_key: ["CloudKit"],
    "com.apple.security.app-sandbox": True,
    "com.apple.security.network.client": True,
    "com.apple.security.files.user-selected.read-write": True,
}
aps_key = "com.apple.developer.aps-environment"
if allowed.get(aps_key) == environment.lower():
    entitlements[aps_key] = environment.lower()
(work / "entitlements.plist").write_bytes(plistlib.dumps(entitlements))
(work / "bundle-id").write_text(bundle_id)
require(re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", version), "Use a three-component app version in Cargo.toml.")
build = os.environ.get("FEI_BUILD_NUMBER", "1" if mode != "app-store" else "")
require(re.fullmatch(r"[1-9][0-9]{0,3}(?:\.[0-9]{1,2}){0,2}", build), "Set a valid FEI_BUILD_NUMBER.")
info = {
    "CFBundleIdentifier": bundle_id, "CFBundleExecutable": "quiet_md", "CFBundlePackageType": "APPL",
    "CFBundleName": "Fei Notes", "CFBundleDisplayName": "Fei Notes",
    "CFBundleShortVersionString": version, "CFBundleVersion": build,
    "LSApplicationCategoryType": "public.app-category.productivity",
    "LSMinimumSystemVersion": "14.0", "NSHighResolutionCapable": True,
    "CFBundleDocumentTypes": [{
        "CFBundleTypeName": "Markdown", "CFBundleTypeRole": "Editor", "LSHandlerRank": "Alternate",
        "LSItemContentTypes": ["net.daringfireball.markdown"],
    }],
}
exempt = os.environ.get("FEI_EXPORT_EXEMPT", "")
if exempt:
    require(mode == "app-store" and exempt == "1", "Set FEI_EXPORT_EXEMPT=1 only for an eligible Apple upload.")
    info["ITSAppUsesNonExemptEncryption"] = False
annual = os.environ.get("FEI_STOREKIT_ANNUAL_PRODUCT", "")
monthly = os.environ.get("FEI_STOREKIT_MONTHLY_PRODUCT", "")
testflight = os.environ.get("FEI_TESTFLIGHT_BETA") == "1"
if testflight:
    require(not annual and not monthly, "TestFlight builds must omit StoreKit product IDs.")
    info["FeiTestFlightBeta"] = True
elif annual or monthly or mode == "app-store":
    require(mode in {"development", "app-store"} and annual != monthly
            and all(re.fullmatch(r"[A-Za-z0-9_.-]{1,100}", product) for product in (annual, monthly)),
            "App Store builds require two distinct StoreKit product IDs. Development builds may configure test products.")
    info.update(FeiStoreKitAnnualProduct=annual, FeiStoreKitMonthlyProduct=monthly)
for key, plist_key in (("FEI_SUPPORT_URL", "FeiSupportURL"), ("FEI_PRIVACY_URL", "FeiPrivacyURL"),
                       ("FEI_TERMS_URL", "FeiTermsURL")):
    url = os.environ.get(key, "")
    if not url:
        continue
    error = f"{key} must be an absolute HTTPS URL without credentials."
    require(not any(ch.isspace() or ord(ch) < 32 or ord(ch) == 127 or ch in '<>"\\^`{|}' for ch in url)
            and not re.search(r"%(?![0-9A-Fa-f]{2})", url), error)
    try:
        parsed = urlsplit(url)
        port = parsed.port
        require(parsed.scheme == "https" and parsed.hostname and parsed.username is None
                and parsed.password is None and port != 0, error)
        if parsed.netloc.startswith("["):
            require(re.fullmatch(r"\[[0-9A-Fa-f:.]+\](?::[0-9]+)?", parsed.netloc), error)
        else:
            hostname = parsed.hostname.encode("idna").decode("ascii").rstrip(".")
            require(all(re.fullmatch(r"[A-Za-z0-9](?:[A-Za-z0-9-]{0,61}[A-Za-z0-9])?", label)
                        for label in hostname.split(".")) and not parsed.netloc.endswith(":"), error)
    except (ValueError, UnicodeError):
        raise SystemExit(error) from None
    info[plist_key] = url
(app / "Contents/Info.plist").write_bytes(plistlib.dumps(info))
PY

export MACOSX_DEPLOYMENT_TARGET=14.0
export FEI_BUNDLE_ID
FEI_BUNDLE_ID=$(cat "$work/bundle-id")
read -r -a architectures <<< "${FEI_MAC_ARCHS:-arm64 x86_64}"
case "${architectures[*]}" in
    arm64|x86_64|"arm64 x86_64") ;;
    *) echo 'FEI_MAC_ARCHS must be arm64, x86_64, or "arm64 x86_64".' >&2; exit 2 ;;
esac
slices=()
bridges=()
: > "$work/rust-notices.txt"
for architecture in "${architectures[@]}"; do
    case "$architecture" in
        arm64) rust_target=aarch64-apple-darwin ;;
        x86_64) rust_target=x86_64-apple-darwin ;;
    esac
    rustup target add "$rust_target"
    slice="$work/$architecture"
    mkdir "$slice"
    cargo build --release --locked --target "$rust_target" --no-default-features --features cloudkit-sync --message-format=json > "$slice/build.jsonl"
    python3 - "$slice" <<'PYBUILD'
import json
from pathlib import Path
import shutil
import sys

slice = Path(sys.argv[1])
rows = [json.loads(line) for line in (slice / "build.jsonl").read_text().splitlines()]
artifact = next(row for row in rows if row.get("reason") == "compiler-artifact"
                and row.get("executable") and row["target"]["name"] == "quiet_md")
if "cloudkit-sync" not in artifact["features"] or "devtools" in artifact["features"]:
    raise SystemExit("Expected the CloudKit release executable without developer tools.")
script = next(row for row in rows if row.get("reason") == "build-script-executed"
              and row["package_id"] == artifact["package_id"])
shutil.copy2(artifact["executable"], slice / "quiet_md")
shutil.copy2(Path(script["out_dir"]) / "libQuietCloudRestore.dylib", slice / "libQuietCloudRestore.dylib")
PYBUILD
    for file in "$slice/quiet_md" "$slice/libQuietCloudRestore.dylib"; do
        [[ "$(lipo -archs "$file")" == "$architecture" ]]
        minos=$(xcrun vtool -show-build "$file" | awk '$1 == "minos" { print $2 }')
        [[ "$minos" == 14.0 ]] || { echo "Unexpected minimum macOS version: $minos" >&2; exit 1; }
    done
    slices+=("$slice/quiet_md")
    bridges+=("$slice/libQuietCloudRestore.dylib")
    python3 scripts/fei-release-licenses.py "$rust_target" "$slice/notices"
    cat "$slice/notices/Open Source Notices.txt" >> "$work/rust-notices.txt"
    printf '\n' >> "$work/rust-notices.txt"
done
binary="$app/Contents/MacOS/quiet_md"
bridge="$app/Contents/Frameworks/libQuietCloudRestore.dylib"
lipo -create "${slices[@]}" -output "$binary"
lipo -create "${bridges[@]}" -output "$bridge"
arch="${architectures[*]}"
for file in "$binary" "$bridge"; do
    for architecture in "${architectures[@]}"; do
        lipo "$file" -verify_arch "$architecture"
    done
    [[ $(lipo -archs "$file" | wc -w) -eq ${#architectures[@]} ]]
done
cp "$profile" "$app/Contents/embedded.provisionprofile"
/bin/sh packaging/macos/brand-app.sh "$app" "$(cat "$work/bundle-id")" "$work/rust-notices.txt"
# An installer may run as root, but App Store users must be able to read its payload.
chmod -R a+rX "$app"
sign=(--force --sign "$identity")
if [[ -n "$signing_keychain" ]]; then sign+=(--keychain "$signing_keychain"); fi
case "$mode" in
    developer-id) sign+=(--options runtime --timestamp) ;;
    *) sign+=(--timestamp=none) ;;
esac
codesign "${sign[@]}" "$bridge"
codesign "${sign[@]}" --entitlements "$work/entitlements.plist" "$app"
codesign --verify --deep --strict "$app"
if ! codesign -d --entitlements - --xml "$app" > "$work/signed-entitlements.plist" 2>"$work/codesign-display.log"; then
    codesign -d --entitlements :- "$app" > "$work/signed-entitlements.plist"
fi
python3 - "$work" <<'PY'
from pathlib import Path
import plistlib
import sys

work = Path(sys.argv[1])
expected, signed = [plistlib.loads((work / f"{name}.plist").read_bytes())
                    for name in ["entitlements", "signed-entitlements"]]
if expected != signed:
    raise SystemExit("The signed app's entitlements differ from the verified configuration.")
PY

if [[ "$mode" == app-store ]]; then
    # productbuild defaults to both architectures even for a single-architecture app.
    read -r -a architectures <<< "$arch"
    python3 - "$work/product.plist" "$MACOSX_DEPLOYMENT_TARGET" "${architectures[@]}" <<'PY'
import plistlib
import sys

with open(sys.argv[1], "wb") as out:
    plistlib.dump({"os": [sys.argv[2]], "arch": sys.argv[3:]}, out)
PY
    archive="$work/package/FeiNotes.pkg"
    if [[ "$channel" == testflight ]]; then archive="$work/package/FeiNotes-Internal-TestFlight.pkg"; fi
    if [[ "$channel" == public-testflight ]]; then archive="$work/package/FeiNotes-Public-TestFlight.pkg"; fi
    installer_name=$(cat "$work/installer-name")
    package_sign=(--sign "$installer_name")
    if [[ -n "$signing_keychain" ]]; then package_sign+=(--keychain "$signing_keychain"); fi
    productbuild --component "$app" /Applications --product "$work/product.plist" "${package_sign[@]}" "$archive"
    pkgutil --check-signature "$archive" > "$work/installer-signature.log"
    awk -v name="$installer_name" '$1 == "1." { sub(/^[[:space:]]*1\.[[:space:]]+/, ""); if ($0 == name) verified = 1 } END { exit !verified }' \
        "$work/installer-signature.log" || { echo 'The package has an unexpected installer signature.' >&2; exit 1; }
    pkgutil --expand-full "$archive" "$work/expanded"
    bundled_app="$work/expanded/$FEI_BUNDLE_ID.pkg/Payload/Fei Notes.app"
    codesign --verify --deep --strict "$bundled_app"
    diff -rq "$app" "$bundled_app"
else
    archive="$work/package/Fei Notes.zip"
    ditto -c -k --keepParent "$app" "$archive"
    if [[ "$mode" == developer-id ]]; then
        xcrun notarytool submit "$archive" "${notary_auth[@]}" --wait
        xcrun stapler staple "$app"
        xcrun stapler validate "$app"
        ditto -c -k --keepParent "$app" "$archive"
    fi
fi
mkdir "$output"
mv "$app" "$archive" "$output/"
printf 'Fei Notes %s (%s, %s)\n%s\n' "$channel" "$FEI_CLOUD_ENVIRONMENT" "$arch" "$output"
