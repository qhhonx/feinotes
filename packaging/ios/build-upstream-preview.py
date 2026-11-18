#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
# Copyright 2026 Fei Notes contributors. See LICENSES.md.
"""Package a development-signed universal iPhone/iPad upstream preview."""
import argparse
from datetime import datetime
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import subprocess
import tempfile

REPO = Path(__file__).resolve().parents[2]
IOS = REPO / 'packaging/ios'
BUNDLE = 'local.feinotes.upstream-preview.ios'


def run(*args, **kwargs):
    return subprocess.check_output(args, cwd=REPO, **kwargs)


def verify(app, revision):
    info = plistlib.loads((app/'Info.plist').read_bytes())
    assert info['CFBundleIdentifier'] == BUNDLE and info['FeiSourceRevision'] == revision
    assert info['UIDeviceFamily'] == [1, 2] and info['FeiUpstreamPreview'] is True
    assert not any(k in info for k in ['FeiTestFlightBeta','CFBundleURLTypes','CKSharingSupported'])
    run('codesign','--verify','--deep','--strict',str(app))
    entitlements = plistlib.loads(run('codesign','-d','--entitlements',':-',str(app),stderr=subprocess.DEVNULL))
    assert entitlements['application-identifier'].endswith('.'+BUNDLE)
    assert entitlements.get('get-task-allow') is True
    assert not any(k.startswith(('com.apple.developer.icloud','com.apple.developer.ubiquity','com.apple.security.application-groups')) for k in entitlements)
    assert all(group == entitlements['application-identifier'] for group in entitlements.get('keychain-access-groups',[]))
    profile = plistlib.loads(run('security','cms','-D','-i',str(app/'embedded.mobileprovision')))
    assert profile['Entitlements'].get('get-task-allow') is True and profile.get('ProvisionedDevices')
    return {'bundle_id':BUNDLE,'source_revision':revision,'device_family':info['UIDeviceFamily'],
            'version':info['CFBundleShortVersionString'],'build':info['CFBundleVersion'],
            'signed_entitlements':entitlements,'profile_expires':str(profile['ExpirationDate'])}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--profile',type=Path,required=True,help='Existing wildcard Apple Development profile')
    parser.add_argument('--identity',required=True,help='SHA1 of a local Apple Development identity')
    args = parser.parse_args()
    assert not run('git','status','--porcelain',text=True).strip(), 'Commit the candidate first'
    revision = run('git','rev-parse','HEAD',text=True).strip()
    assert run('git','branch','--show-current',text=True).strip().startswith('integrate/')
    assert re.fullmatch(r'[A-Fa-f0-9]{40}',args.identity)
    profile = plistlib.loads(run('security','cms','-D','-i',str(args.profile)))
    e = profile['Entitlements']; team = profile['TeamIdentifier'][0]
    assert e.get('get-task-allow') is True and e['application-identifier'] == team+'.*'
    assert not e.get('com.apple.developer.icloud-container-identifiers')
    assert profile['ExpirationDate'] > datetime.utcnow()
    assert args.identity.lower() in [hashlib.sha1(c).hexdigest() for c in profile['DeveloperCertificates']]
    out = IOS/'.private-builds'/('upstream-'+revision[:12])
    out.mkdir(parents=True,exist_ok=False)
    env = {k:v for k,v in os.environ.items() if not k.startswith(('FEI_','QUIET_','FDL_','DYLD_'))}
    env['PATH'] = str(Path.home()/'.cargo/bin')+':'+env.get('PATH','/usr/bin:/bin')
    subprocess.run(['xcodegen','generate','--spec',str(IOS/'project.yml'),'--project',str(IOS)],cwd=REPO,env=env,check=True)
    command = ['xcodebuild','-project',str(IOS/'FeiNotesEditorProbe.xcodeproj'),'-scheme','FeiNotesEditorProbe',
               '-configuration','UpstreamPreview','-sdk','iphoneos','-destination','generic/platform=iOS',
               '-derivedDataPath',str(out/'derived'),'CODE_SIGN_STYLE=Manual','DEVELOPMENT_TEAM='+team,
               'CODE_SIGN_IDENTITY='+args.identity,'PROVISIONING_PROFILE='+profile['UUID'],
               'PROVISIONING_PROFILE_SPECIFIER=','FEI_SOURCE_REVISION='+revision,'build']
    with (out/'xcodebuild.log').open('w') as log:
        subprocess.run(command,cwd=REPO,env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
    source = out/'derived/Build/Products/UpstreamPreview-iphoneos/FeiNotes.app'
    result = verify(source,revision)
    payload = out/'package/Payload'
    payload.mkdir(parents=True)
    shutil.copytree(source,payload/'FeiNotes.app')
    for device in ['iPhone','iPad']:
        ipa = out/f'FeiNotes-Upstream-{device}.ipa'
        subprocess.run(['/usr/bin/zip','-qr','-X',str(ipa),'Payload'],cwd=payload.parent,check=True)
        with tempfile.TemporaryDirectory(prefix='fei-preview-ipa-',dir='/private/tmp') as extracted:
            subprocess.run(['/usr/bin/unzip','-q',str(ipa),'-d',extracted],check=True)
            verify(Path(extracted)/'Payload/FeiNotes.app',revision)
    assert run('git','rev-parse','HEAD',text=True).strip() == revision
    assert not run('git','status','--porcelain',text=True).strip(), 'Candidate changed while building'
    result.update(app=str(source),scope='Isolated local SQLite preview; no live CloudKit acceptance',
                  packages={p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in out.glob('*.ipa')})
    (out/'preview-verification.json').write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,indent=2))


if __name__ == '__main__': main()
