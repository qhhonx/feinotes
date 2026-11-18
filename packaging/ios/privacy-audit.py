#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
# Copyright 2026 Fei Notes contributors. See LICENSES.md.
"""Check the current iOS release's system-SQLite and privacy-manifest contract.

This is a targeted package gate, not a substitute for reviewing actual API purposes.
Overlapping attribute-list APIs require source-level review of the requested fields.
"""
import argparse
import hashlib
import json
from pathlib import Path
import plistlib
import subprocess

DISK_CALLS = {'statfs', 'statfs64', 'fstatfs', 'fstatfs64', 'statvfs', 'fstatvfs'}
REASONS = {
    'NSPrivacyAccessedAPICategoryFileTimestamp': {'C617.1', '3B52.1'},
    'NSPrivacyAccessedAPICategorySystemBootTime': {'35F9.1'},
    'NSPrivacyAccessedAPICategoryUserDefaults': {'CA92.1'},
}


def require(value, message):
    if not value:
        raise ValueError(message)


def check_symbols(output):
    # Normalize Darwin's leading underscore and optional INODE64/other suffixes.
    names = {line.split()[-1].lstrip('_').split('$', 1)[0] for line in output.splitlines() if line.split()}
    unresolved = sorted(names & DISK_CALLS)
    require(not unresolved, 'Unreviewed disk-space API imports in this release: ' + ', '.join(unresolved))
    require(any(name.startswith('sqlite3_') for name in names), 'No dynamic SQLite imports found; inspect the actual dependency backend')
    baseline = json.loads(Path(__file__).with_name('sqlite-api-baseline.json').read_text())
    unexpected = sorted(name for name in names if name.startswith('sqlite3_') and name not in baseline['symbols'])
    require(not unexpected, 'SQLite imports exceed the reviewed minimum API baseline: ' + ', '.join(unexpected))
    return names


def check_manifest(manifest):
    require(manifest.get('NSPrivacyTracking') is False, 'Expected no tracking')
    require(not manifest.get('NSPrivacyTrackingDomains'), 'Unexpected tracking domains')
    entries = manifest.get('NSPrivacyAccessedAPITypes', [])
    categories = {item['NSPrivacyAccessedAPIType']: set(item.get('NSPrivacyAccessedAPITypeReasons', [])) for item in entries}
    require(len(categories) == len(entries), 'Duplicate privacy API category')
    for category, reasons in REASONS.items():
        require(reasons <= categories.get(category, set()), 'Missing reviewed API reasons for ' + category)
    require('NSPrivacyAccessedAPICategoryDiskSpace' not in categories,
            'This release has no reviewed disk-capacity use case; do not add a reason to cover SQLite lock/filesystem checks')
    return categories


def version(value):
    parts = tuple(int(part) for part in value.split('.'))
    return (parts + (0, 0))[:3]


def check_minimum_os(load_commands):
    command = None
    minimum = []
    for line in load_commands.splitlines():
        fields = line.split()
        if fields[:1] == ['cmd']:
            command = fields[1]
        elif len(fields) == 2 and ((command == 'LC_BUILD_VERSION' and fields[0] == 'minos') or
                                  (command == 'LC_VERSION_MIN_IPHONEOS' and fields[0] == 'version')):
            minimum.append(fields[1])
    require(len(minimum) == 1 and version(minimum[0]) == (17, 0, 0), 'Expected a single iOS17.0 minimum in the actual device executable')
    return minimum[0]


def audit(app):
    app = app.resolve()
    info = plistlib.loads((app / 'Info.plist').read_bytes())
    require(version(info.get('MinimumOSVersion', '0')) == (17, 0, 0), 'Bundle minimum OS must remain iOS17.0')
    executable = (app / info['CFBundleExecutable']).resolve()
    require(executable.is_relative_to(app), 'Executable escaped the app bundle')
    manifest_path = app / 'PrivacyInfo.xcprivacy'
    require(manifest_path.is_file() and not manifest_path.is_symlink(), 'Privacy manifest missing from the actual app')
    categories = check_manifest(plistlib.loads(manifest_path.read_bytes()))
    imports = subprocess.check_output(['nm', '-u', str(executable)], text=True)
    names = check_symbols(imports)
    minimum = check_minimum_os(subprocess.check_output(['otool', '-l', str(executable)], text=True))
    libraries = subprocess.check_output(['otool', '-L', str(executable)], text=True)
    require(any(path in libraries for path in ('/usr/lib/libsqlite3.dylib ', '/usr/lib/libsqlite3.0.dylib ')),
            'Expected the Apple system SQLite library')
    return {'policy': 'fei-ios-1.0.7-system-sqlite', 'version': info['CFBundleShortVersionString'],
            'build': info['CFBundleVersion'], 'manifestSHA256': hashlib.sha256(manifest_path.read_bytes()).hexdigest(),
            'executableSHA256': hashlib.sha256(executable.read_bytes()).hexdigest(),
            'reasons': {key: sorted(value) for key, value in categories.items()},
            'declaredTracking': False, 'systemSQLiteLinked': True, 'directDiskSpaceImports': [],
            'dynamicSQLiteImportCount': sum(name.startswith('sqlite3_') for name in names),
            'binaryMinimumOS': minimum, 'sqliteAPIBaseline': '3.34.1',
            'validationScope': 'Bundle metadata and linked imports; actual minimum-OS device acceptance is recorded separately'}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('app', type=Path)
    args = parser.parse_args()
    print(json.dumps(audit(args.app), indent=2))
