# SPDX-License-Identifier: MIT
# Copyright 2026 Fei Notes contributors. See LICENSES.md.
"""Offline regression checks for rejecting development and mismatched iOS packages."""
import datetime
import hashlib
import importlib.util
from pathlib import Path
from unittest import TestCase, main
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('ios_release', Path(__file__).with_name('release.py'))
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


def profile():
    return {'UUID': 'expected', 'TeamIdentifier': [release.TEAM], 'Platform': ['iOS'],
            'ExpirationDate': datetime.datetime(2099, 1, 1), 'DeveloperCertificates': [b'distribution certificate'],
            'Entitlements': {'application-identifier': release.TEAM + '.' + release.BUNDLE,
                'get-task-allow': False, 'beta-reports-active': True,
                'com.apple.developer.icloud-container-environment': 'Production',
                'com.apple.developer.icloud-container-identifiers': [release.CONTAINER],
                'com.apple.developer.icloud-services': ['CloudKit'],
                'com.apple.developer.associated-domains': [release.DOMAIN]}}


class IOSReleaseTest(TestCase):
    def test_actual_signer_accepts_matching_leaf_and_rejects_another_certificate(self):
        certificate = b'distribution certificate'
        def extract(command, **kwargs):
            # Match codesign's optional-argument parsing, which rejects a separate prefix.
            option = next(item for item in command if item.startswith('--extract-certificates='))
            Path(option.split('=', 1)[1] + '0').write_bytes(certificate)
        with patch.object(release.subprocess, 'run', side_effect=extract):
            release.validate_signer(Path('FeiNotes.app'), hashlib.sha1(certificate).hexdigest())
            with self.assertRaises(ValueError): release.validate_signer(Path('FeiNotes.app'), '0' * 40)

    def test_signature_without_leaf_certificate_is_rejected(self):
        with patch.object(release.subprocess, 'run'):
            with self.assertRaisesRegex(ValueError, 'no leaf signing certificate'):
                release.validate_signer(Path('FeiNotes.app'), '0' * 40)

    def check_profile(self, value):
        return release.validate_profile(value, hashlib.sha1(b'distribution certificate').hexdigest(), 'expected')

    def test_distribution_profile_and_wildcard_domains(self):
        self.check_profile(profile())
        value = profile(); value['Entitlements']['com.apple.developer.associated-domains'] = '*'
        self.check_profile(value)

    def test_apple_distribution_allowlists_do_not_select_the_app_environment(self):
        value = profile()
        value['Entitlements']['com.apple.developer.icloud-container-environment'] = ['Production', 'Development']
        value['Entitlements']['com.apple.developer.icloud-services'] = '*'
        value['Entitlements']['com.apple.developer.associated-domains'] = ['*']
        self.check_profile(value)
        value['Entitlements']['com.apple.developer.icloud-container-environment'] = ['Development']
        with self.assertRaises(ValueError): self.check_profile(value)

    def test_app_selects_production_cloudkit_even_with_broad_profile_permissions(self):
        ent = profile()['Entitlements'].copy()
        release.validate_signed_entitlements(ent)
        ent['com.apple.developer.icloud-container-environment'] = 'Development'
        with self.assertRaises(ValueError): release.validate_signed_entitlements(ent)
        ent['com.apple.developer.icloud-container-environment'] = 'Production'
        ent['com.apple.developer.icloud-services'] = '*'
        with self.assertRaises(ValueError): release.validate_signed_entitlements(ent)

    def test_rejects_development_enterprise_mac_and_expired_profiles(self):
        invalid = [('ProvisionedDevices', ['device']), ('ProvisionsAllDevices', True),
                   ('Platform', ['OSX']), ('ExpirationDate', datetime.datetime(2000, 1, 1)),
                   ('TeamIdentifier', ['wrong']), ('UUID', 'wrong'), ('DeveloperCertificates', [b'other'])]
        for key, value in invalid:
            with self.subTest(key=key):
                data = profile(); data[key] = value
                with self.assertRaises(ValueError): self.check_profile(data)

    def test_rejects_development_and_wrong_cloud_entitlements(self):
        invalid = [('get-task-allow', True), ('beta-reports-active', False), ('application-identifier', 'probe'),
                   ('com.apple.developer.icloud-container-environment', 'Development'),
                   ('com.apple.developer.icloud-container-identifiers', ['iCloud.probe']),
                   ('com.apple.developer.icloud-services', []), ('com.apple.developer.associated-domains', [])]
        for key, value in invalid:
            with self.subTest(key=key):
                data = profile(); data['Entitlements'][key] = value
                with self.assertRaises(ValueError): self.check_profile(data)

    def info(self):
        return {'CFBundleIdentifier': release.BUNDLE, 'CFBundleShortVersionString': '1.0.7', 'CFBundleVersion': '1015',
                'CFBundleSupportedPlatforms': ['iPhoneOS'], 'UIDeviceFamily': [1, 2], 'MinimumOSVersion': '17.0',
                'FeiTestFlightBeta': True, 'ITSAppUsesNonExemptEncryption': False}

    def test_accepts_device_beta_package(self):
        release.validate_info(self.info(), '1.0.7', '1015')

    def test_rejects_development_simulator_wrong_version_or_subscription_packages(self):
        invalid = [('CFBundleIdentifier', 'app.feinotes.ipad.editorprobe'), ('CFBundleVersion', '48'),
                   ('CFBundleSupportedPlatforms', ['iPhoneSimulator']), ('FeiTestFlightBeta', False),
                   ('FeiTestFlightBeta', 'YES'), ('FeiStoreKitAnnualProduct', 'annual'),
                   ('ITSAppUsesNonExemptEncryption', True), ('UIDeviceFamily', [2])]
        for key, value in invalid:
            with self.subTest(key=key, value=value):
                info = self.info(); info[key] = value
                with self.assertRaises(ValueError): release.validate_info(info, '1.0.7', '1015')

    def settings(self):
        return [{'target': 'FeiNotesEditorProbe', 'buildSettings': {'CONFIGURATION': 'TestFlight',
                 'PRODUCT_BUNDLE_IDENTIFIER': release.BUNDLE, 'RUST_PROFILE': 'release',
                 'FEI_TESTFLIGHT_BETA': '1', 'FEI_MOBILE_QA': '0',
                 'SWIFT_ACTIVE_COMPILATION_CONDITIONS': 'FEI_CLOUD_PRODUCT FEI_TESTFLIGHT_BETA'}}]

    def test_accepts_consistent_release_namespace(self):
        release.validate_settings(self.settings())

    def test_rejects_qa_debug_or_missing_rust_gate(self):
        for key, value in [('CONFIGURATION', 'Product'), ('RUST_PROFILE', 'debug'), ('FEI_TESTFLIGHT_BETA', '0'),
                           ('FEI_MOBILE_QA', '1'), ('SWIFT_ACTIVE_COMPILATION_CONDITIONS', 'FEI_CLOUD_QA')]:
            with self.subTest(key=key):
                settings = self.settings(); settings[0]['buildSettings'][key] = value
                with self.assertRaises(ValueError): release.validate_settings(settings)

    def test_export_never_renumbers_or_restricts_to_internal_testers(self):
        options = release.export_options(profile(), 'fingerprint', 'upload')
        self.assertFalse(options['manageAppVersionAndBuildNumber'])
        self.assertFalse(options['testFlightInternalTestingOnly'])
        self.assertEqual(options['provisioningProfiles'], {release.BUNDLE: 'expected'})
        self.assertEqual(options['method'], 'app-store-connect')


if __name__ == '__main__':
    main()
