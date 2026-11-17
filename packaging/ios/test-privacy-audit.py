# SPDX-License-Identifier: MIT
# Copyright 2026 Fei Notes contributors. See LICENSES.md.
"""Offline checks that the package gate cannot hide SQLite disk API usage."""
import importlib.util
from pathlib import Path
from unittest import TestCase, main

spec = importlib.util.spec_from_file_location('privacy_audit', Path(__file__).with_name('privacy-audit.py'))
privacy = importlib.util.module_from_spec(spec)
spec.loader.exec_module(privacy)


class PrivacyAuditTest(TestCase):
    def manifest(self):
        return {'NSPrivacyTracking': False, 'NSPrivacyAccessedAPITypes': [
            {'NSPrivacyAccessedAPIType': key, 'NSPrivacyAccessedAPITypeReasons': sorted(value)}
            for key, value in privacy.REASONS.items()]}

    def test_accepts_reviewed_reasons_and_system_sqlite_imports(self):
        privacy.check_manifest(self.manifest())
        privacy.check_symbols(' U _sqlite3_open_v2\n U _sqlite3_exec\n U _fstat\n U _mach_absolute_time\n')

    def test_disk_api_and_darwin_suffixes_are_not_ignored(self):
        for name in ('statfs', 'fstatfs', 'statvfs', 'fstatvfs', 'statfs64', 'fstatfs$INODE64'):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, 'Unreviewed disk-space'):
                privacy.check_symbols(' U _sqlite3_open_v2\n U _' + name)

    def test_new_clientdata_and_other_unreviewed_sqlite_imports_fail(self):
        for name in ('sqlite3_set_clientdata', 'sqlite3_get_clientdata', 'sqlite3_setlk_timeout'):
            with self.subTest(name=name), self.assertRaisesRegex(ValueError, 'exceed the reviewed'):
                privacy.check_symbols(' U _sqlite3_open_v2\n U _' + name)

    def test_actual_macho_minimum_is_checked(self):
        good = 'cmd LC_BUILD_VERSION\ncmdsize 32\nplatform IOS\nminos 17.0\nsdk 27.0'
        self.assertEqual(privacy.check_minimum_os(good), '17.0')
        for output in ('', good.replace('17.0', '18.0'), good + '\n' + good):
            with self.subTest(output=output), self.assertRaises(ValueError):
                privacy.check_minimum_os(output)

    def test_an_empty_symbol_table_does_not_prove_no_api_usage(self):
        with self.assertRaisesRegex(ValueError, 'No dynamic SQLite'):
            privacy.check_symbols('')

    def test_invented_disk_reason_does_not_mask_the_dependency_problem(self):
        manifest = self.manifest()
        manifest['NSPrivacyAccessedAPITypes'].append({'NSPrivacyAccessedAPIType': 'NSPrivacyAccessedAPICategoryDiskSpace', 'NSPrivacyAccessedAPITypeReasons': ['E174.1']})
        with self.assertRaisesRegex(ValueError, 'no reviewed disk-capacity use case'):
            privacy.check_manifest(manifest)

    def test_tracking_and_missing_reasons_fail(self):
        for mutation in ('tracking', 'missing'):
            manifest = self.manifest()
            if mutation == 'tracking': manifest['NSPrivacyTracking'] = True
            else: manifest['NSPrivacyAccessedAPITypes'][0]['NSPrivacyAccessedAPITypeReasons'] = []
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                privacy.check_manifest(manifest)


if __name__ == '__main__':
    main()
