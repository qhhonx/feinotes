#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
# Copyright 2026 Fei Notes contributors. See LICENSES.md.
"""Validate an iOS distribution archive without changing Apple signing resources."""
import argparse
import datetime
import hashlib
import json
from pathlib import Path
import plistlib
import subprocess
import tempfile

BUNDLE = 'app.feinotes.mac'
TEAM = '95BC6VVXL8'
CONTAINER = 'iCloud.app.feinotes.mac'
DOMAIN = 'applinks:feinotes.app'


def require(value, message):
    if not value:
        raise ValueError(message)


def validate_profile(profile, signer, uuid, now=None):
    now = now or datetime.datetime.now(datetime.timezone.utc)
    expiry = profile['ExpirationDate'].replace(tzinfo=datetime.timezone.utc)
    require(expiry > now, 'iOS profile expired')
    require(profile['UUID'] == uuid, 'Unexpected iOS profile UUID')
    require(profile.get('TeamIdentifier') == [TEAM], 'Wrong iOS signing team')
    require('iOS' in profile.get('Platform', []), 'Expected an iOS provisioning profile')
    require(not profile.get('ProvisionedDevices') and not profile.get('ProvisionsAllDevices'),
            'Device-limited and enterprise profiles cannot ship to TestFlight')
    ent = profile['Entitlements']
    require(ent.get('application-identifier') == TEAM + '.' + BUNDLE, 'Wrong iOS application identifier')
    require(ent.get('get-task-allow') is False, 'Development signing is forbidden')
    require(ent.get('beta-reports-active') is True, 'Expected an App Store distribution profile')
    environments = ent.get('com.apple.developer.icloud-container-environment')
    require(environments == 'Production' or isinstance(environments, list) and 'Production' in environments, 'Profile does not authorize Production CloudKit')
    require(ent.get('com.apple.developer.icloud-container-identifiers') == [CONTAINER], 'Wrong iCloud container')
    services = ent.get('com.apple.developer.icloud-services', [])
    require(services in ('*', 'CloudKit') if isinstance(services, str) else 'CloudKit' in services or '*' in services, 'Missing CloudKit service')
    domains = ent.get('com.apple.developer.associated-domains', [])
    require(domains == '*' or DOMAIN in domains or isinstance(domains, list) and '*' in domains, 'iOS profile does not allow the invitation domain')
    fingerprints = {hashlib.sha1(cert).hexdigest().upper() for cert in profile['DeveloperCertificates']}
    require(signer.upper() in fingerprints, 'iOS certificate is not in this profile')
    return profile


def decode_profile(path):
    return plistlib.loads(subprocess.check_output(['security', 'cms', '-D', '-i', str(path)]))


def validate_info(info, version, build):
    require(info['CFBundleIdentifier'] == BUNDLE, 'Unexpected iOS bundle')
    require((info['CFBundleShortVersionString'], info['CFBundleVersion']) == (version, build), 'Incorrect iOS version/build')
    require(info.get('CFBundleSupportedPlatforms') == ['iPhoneOS'], 'Simulator packages cannot be uploaded')
    require(set(info.get('UIDeviceFamily', [])) == {1, 2}, 'Expected both iPhone and iPad')
    require(tuple(map(int, info.get('MinimumOSVersion', '0').split('.'))) >= (17, 0), 'Unexpected iOS deployment target')
    require(info.get('FeiTestFlightBeta') is True, 'Missing beta package marker')
    require(not any(info.get('FeiStoreKit' + plan + 'Product') for plan in ('Annual', 'Monthly')),
            'TestFlight must not require subscription products')
    require(info.get('ITSAppUsesNonExemptEncryption') is False, 'Export compliance must be explicitly configured')


def validate_signed_entitlements(ent):
    require(ent.get('get-task-allow') in (None, False), 'Development signed app')
    require(ent.get('application-identifier') == TEAM + '.' + BUNDLE, 'Signed app identity mismatch')
    require(ent.get('com.apple.developer.icloud-container-environment') == 'Production', 'Signed app uses Development CloudKit')
    require(ent.get('com.apple.developer.icloud-container-identifiers') == [CONTAINER], 'Signed container mismatch')
    require(ent.get('com.apple.developer.icloud-services') == ['CloudKit'], 'Signed app must select only CloudKit')
    require(DOMAIN in ent.get('com.apple.developer.associated-domains', []), 'Signed invitation domain missing')


def verify_app(app, version, build, signer, uuid):
    info = plistlib.loads((app / 'Info.plist').read_bytes())
    validate_info(info, version, build)
    privacy = plistlib.loads((app / 'PrivacyInfo.xcprivacy').read_bytes())
    require(privacy.get('NSPrivacyTracking') is False, 'Unexpected tracking declaration')
    reasons = {item['NSPrivacyAccessedAPIType']: set(item['NSPrivacyAccessedAPITypeReasons'])
               for item in privacy.get('NSPrivacyAccessedAPITypes', [])}
    for category, required in [('FileTimestamp', {'C617.1', '3B52.1'}),
                               ('SystemBootTime', {'35F9.1'}), ('UserDefaults', {'CA92.1'})]:
        require(required <= reasons.get('NSPrivacyAccessedAPICategory' + category, set()),
                'Missing iOS required API reasons: ' + category)
    profile = validate_profile(decode_profile(app / 'embedded.mobileprovision'), signer, uuid)
    subprocess.run(['codesign', '--verify', '--deep', '--strict', str(app)], check=True)
    result = subprocess.run(['codesign', '-d', '--entitlements', ':-', str(app)], capture_output=True, check=True)
    ent = plistlib.loads(result.stdout)
    validate_signed_entitlements(ent)
    executable = app / info['CFBundleExecutable']
    require(subprocess.check_output(['lipo', '-archs', str(executable)], text=True).split() == ['arm64'],
            'Expected a device arm64 executable')
    # Verify the actual leaf signing certificate, not only profile membership.
    with tempfile.TemporaryDirectory() as temporary:
        prefix = str(Path(temporary) / 'signer-')
        subprocess.run(['codesign', '-d', '--extract-certificates', prefix, str(app)], capture_output=True, check=True)
        leaf = Path(prefix + '0').read_bytes()
        require(hashlib.sha1(leaf).hexdigest().upper() == signer.upper(), 'Unexpected actual iOS signer')
    return {'platform': 'IOS', 'version': version, 'build': build, 'profileUUID': profile['UUID'],
            'signerSHA1': signer.upper(), 'bundle': BUNDLE, 'architectures': ['arm64']}


def export_options(profile, signer, destination):
    return {'method': 'app-store-connect', 'destination': destination, 'teamID': TEAM,
            'signingStyle': 'manual', 'signingCertificate': signer, 'provisioningProfiles': {BUNDLE: profile['UUID']},
            'manageAppVersionAndBuildNumber': False, 'testFlightInternalTestingOnly': False,
            'stripSwiftSymbols': True, 'uploadSymbols': True}


def validate_settings(settings):
    matches = [item['buildSettings'] for item in settings if item.get('target') == 'FeiNotesEditorProbe']
    require(len(matches) == 1, 'Expected one mobile product target')
    config = matches[0]
    require(config.get('CONFIGURATION') == 'TestFlight', 'Wrong archive configuration')
    require(config.get('PRODUCT_BUNDLE_IDENTIFIER') == BUNDLE, 'Probe ID cannot ship')
    require(config.get('RUST_PROFILE') == 'release', 'Expected an optimized Rust static library')
    flags = config.get('SWIFT_ACTIVE_COMPILATION_CONDITIONS', '').split()
    require({'FEI_CLOUD_PRODUCT', 'FEI_TESTFLIGHT_BETA'} <= set(flags) and 'FEI_CLOUD_QA' not in flags,
            'Wrong Swift beta namespace or QA configuration')
    require(config.get('FEI_MOBILE_QA', '0') == '0', 'QA fixtures cannot ship')
    require(config.get('FEI_TESTFLIGHT_BETA') == '1', 'Rust beta access flag is missing')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    settings = commands.add_parser('settings')
    settings.add_argument('json', type=Path)
    profile = commands.add_parser('profile')
    profile.add_argument('path', type=Path)
    profile.add_argument('signer')
    profile.add_argument('uuid')
    export = commands.add_parser('export-options')
    export.add_argument('profile', type=Path)
    export.add_argument('signer')
    export.add_argument('uuid')
    export.add_argument('destination', choices=('export', 'upload'))
    export.add_argument('output', type=Path)
    verify = commands.add_parser('verify')
    verify.add_argument('app', type=Path)
    verify.add_argument('version')
    verify.add_argument('build')
    verify.add_argument('signer')
    verify.add_argument('uuid')
    args = parser.parse_args()
    if args.command == 'settings':
        validate_settings(json.loads(args.json.read_text()))
    elif args.command == 'profile':
        validate_profile(decode_profile(args.path), args.signer, args.uuid)
    elif args.command == 'export-options':
        profile = validate_profile(decode_profile(args.profile), args.signer, args.uuid)
        args.output.write_bytes(plistlib.dumps(export_options(profile, args.signer, args.destination)))
    else:
        print(json.dumps(verify_app(args.app, args.version, args.build, args.signer, args.uuid)))


if __name__ == '__main__':
    main()
