# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Copyright 2026 Fei Notes contributors. See LICENSES.md.
"""Offline checks for the release boundaries. No test calls Apple."""
import importlib.util
import json
import os
from pathlib import Path
from tempfile import TemporaryDirectory
from unittest import TestCase, main, mock
from urllib.error import URLError

module = Path(__file__).with_name("apple-release.py")
spec = importlib.util.spec_from_file_location("apple_release", module)
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


def record(kind, identifier, **attributes):
    return {"id": identifier, "type": kind, "attributes": attributes}


class AppleReleaseTest(TestCase):
    def test_ci_masks_cover_profile_and_certificate_owner(self):
        fingerprint = "A" * 40
        installer = '3rd Party Mac Developer Installer: Example Owner (EXAMPLE123)'
        profile = {"UUID": "test-profile-uuid", "Name": "Test Distribution Profile", "TeamName": "Test Team",
                   "TeamIdentifier": ["EXAMPLE123"], "ApplicationIdentifierPrefix": ["PREFIX1234"]}
        masks = release.ci_signing_masks(profile, f'  1) {fingerprint} "{installer}"\n  1 valid identities found')
        self.assertEqual(set(masks), {fingerprint, installer, "Example Owner", "EXAMPLE123", "PREFIX1234",
                                      "test-profile-uuid", "Test Distribution Profile", "Test Team"})
        self.assertLess(masks.index(installer), masks.index("Example Owner"))

    def setUp(self):
        switch = mock.patch.dict(os.environ, FEI_EXPIRE_SUPERSEDED_BETAS='0')
        switch.start()
        self.addCleanup(switch.stop)

    def beta_status(self, builds, states, enabled=True):
        group = record('betaGroups', 'public-group', publicLink='https://testflight.apple.com/join/example',
                       publicLinkEnabled=enabled)
        def get(method, path):
            identifier = path.split('/')[3]
            attributes = {'externalBuildState': states[identifier]} if path.endswith('buildBetaDetail') else {
                'version': {'new': '1.0.1', 'old': '1.0.0'}[identifier], 'platform': 'MAC_OS'}
            return {'data': {'attributes': attributes}}
        with TemporaryDirectory() as directory, mock.patch.dict(os.environ, FEI_PUBLIC_BETA_GROUP_ID='public-group'), \
             mock.patch.object(release, 'beta_group', return_value=group), \
             mock.patch.object(release, 'rows', return_value=iter(builds)), \
             mock.patch.object(release, 'request', side_effect=get) as request:
            output = Path(directory) / 'status.json'
            release.public_beta_status(output)
            return json.loads(output.read_text()), request

    def beta_builds(self):
        return [record('builds', identifier, version=number, uploadedDate=date, expired=False,
                       processingState='VALID', buildAudienceType='APP_STORE_ELIGIBLE')
                for identifier, number, date in [('old', '1006', '2026-11-11'), ('new', '1007', '2026-11-16')]]

    def test_review_preserves_the_available_public_build(self):
        status, _ = self.beta_status(self.beta_builds(), {'new': 'IN_BETA_REVIEW', 'old': 'IN_BETA_TESTING'})
        self.assertEqual((status['state'], status['version'], status['build']), ('available', '1.0.0', '1006'))
        self.assertEqual(status['pending'], {'state': 'review', 'version': '1.0.1', 'build': '1007'})

    def test_approval_selects_the_new_build_and_clears_pending(self):
        status, request = self.beta_status(self.beta_builds(), {'new': 'IN_BETA_TESTING', 'old': 'IN_BETA_REVIEW'})
        self.assertEqual((status['state'], status['version'], status['build']), ('available', '1.0.1', '1007'))
        self.assertNotIn('pending', status)
        self.assertFalse(any('/old/buildBetaDetail' in call.args[1] for call in request.call_args_list))

    def test_rejection_does_not_hide_an_older_available_build(self):
        status, _ = self.beta_status(self.beta_builds(), {'new': 'BETA_REJECTED', 'old': 'READY_FOR_BETA_TESTING'})
        self.assertEqual((status['state'], status['build']), ('available', '1006'))
        self.assertNotIn('pending', status)

    def test_review_alone_does_not_claim_a_downloadable_build(self):
        status, _ = self.beta_status(self.beta_builds()[1:], {'new': 'WAITING_FOR_BETA_REVIEW'})
        self.assertEqual((status['state'], status['version']), ('review', '1.0.1'))

    def test_expired_and_internal_only_builds_are_not_download_candidates(self):
        builds = self.beta_builds()
        builds[0]['attributes']['expired'] = True
        builds[1]['attributes']['buildAudienceType'] = 'INTERNAL_ONLY'
        status, request = self.beta_status(builds, {})
        self.assertEqual(status['state'], 'unavailable')
        request.assert_not_called()

    def test_disabled_public_invitation_is_not_available(self):
        status, request = self.beta_status(self.beta_builds(), {}, enabled=False)
        self.assertEqual(status['state'], 'unavailable')
        request.assert_not_called()

    def test_network_retry_is_limited_to_reads(self):
        response = mock.MagicMock()
        response.__enter__.return_value.status = 204
        with mock.patch.object(release, "signed_token", return_value="test"), \
             mock.patch.object(release.time, "sleep"), \
             mock.patch.object(release, "urlopen", side_effect=[URLError("timeout"), response]) as open_url:
            self.assertEqual(release.request("GET", "/v1/builds"), {})
            self.assertEqual(open_url.call_count, 2)
        with mock.patch.object(release, "signed_token", return_value="test"), \
             mock.patch.object(release, "urlopen", side_effect=URLError("timeout")) as open_url:
            with self.assertRaises(URLError):
                release.request("PATCH", "/v1/reviewSubmissions/draft", {"submitted": True})
            open_url.assert_called_once()

    def test_build_selection_uses_exact_version_build_and_mac(self):
        build = record("builds", "correct", version="3", expired=False, processingState="VALID",
                       buildAudienceType="INTERNAL_ONLY")
        upload = record("buildUploads", "upload", state={"state": "COMPLETE"})
        upload["relationships"] = {"build": release.link("builds", "correct")}
        with mock.patch.object(release, "request", return_value={"data": [upload], "included": [build]}) as call:
            self.assertEqual(release.accepted_build("testflight", "1.0.0", "3", 0), build)
            self.assertEqual(call.call_args.kwargs["query"], {
                "filter[platform]": "MAC_OS", "filter[cfBundleShortVersionString]": "1.0.0",
                "filter[cfBundleVersion]": "3", "include": "build",
            })
            with self.assertRaisesRegex(ValueError, "audience"):
                release.accepted_build("store", "1.0.0", "3", 0)

    def test_ios_upload_is_scoped_and_mac_default_is_unchanged(self):
        with mock.patch.object(release, 'request', return_value={'data': []}) as call:
            self.assertEqual(release.exact_upload('1.0.7', '1015', 'IOS'), (None, None))
            self.assertEqual(call.call_args.kwargs['query']['filter[platform]'], 'IOS')
            release.exact_upload('1.0.7', '1015')
            self.assertEqual(call.call_args.kwargs['query']['filter[platform]'], 'MAC_OS')
            with self.assertRaises(ValueError):
                release.exact_upload('1.0.7', '1015', 'TV_OS')

    def test_new_ios_approval_does_not_replace_mac_availability(self):
        builds = self.beta_builds()
        group = record('betaGroups', 'public-group', publicLink='https://testflight.apple.com/join/example', publicLinkEnabled=True)
        def get(method, path):
            identifier = path.split('/')[3]
            attributes = {'externalBuildState': 'IN_BETA_TESTING'} if path.endswith('buildBetaDetail') else {
                'version': '2.0.0' if identifier == 'new' else '1.0.0',
                'platform': 'IOS' if identifier == 'new' else 'MAC_OS'}
            return {'data': {'attributes': attributes}}
        with TemporaryDirectory() as directory, mock.patch.dict(os.environ, FEI_PUBLIC_BETA_GROUP_ID='public-group'), \
             mock.patch.object(release, 'beta_group', return_value=group), \
             mock.patch.object(release, 'rows', return_value=iter(builds)), mock.patch.object(release, 'request', side_effect=get):
            output = Path(directory) / 'status.json'
            release.public_beta_status(output)
            status = json.loads(output.read_text())
            self.assertEqual((status['version'], status['build']), ('1.0.0', '1006'))
            self.assertEqual(status['platforms']['IOS']['version'], '2.0.0')
            self.assertEqual(status['platforms']['MAC_OS']['build'], '1006')

    def test_release_specific_notes_are_preserved_for_both_locales(self):
        build = record('builds', 'mobile', usesNonExemptEncryption=False)
        with TemporaryDirectory() as directory, mock.patch.dict(os.environ, FEI_BETA_NOTES_DIR=directory, FEI_SUPPORT_URL=''), \
             mock.patch.object(release, 'rows', return_value=[]), mock.patch.object(release, 'create') as create:
            for locale in ('zh-Hans', 'en-US'):
                (Path(directory) / f'{locale}.txt').write_text(f'{locale} exact mobile notes')
            release.beta_notes(build)
            self.assertEqual([call.args[2]['whatsNew'] for call in create.call_args_list],
                             ['en-US exact mobile notes', 'zh-Hans exact mobile notes'])

    def test_ios_does_not_finish_until_upload_and_processing_are_both_complete(self):
        build = record('builds', 'ios', version='1015', expired=False, processingState='VALID', buildAudienceType='APP_STORE_ELIGIBLE')
        upload = record('buildUploads', 'upload', state={'state': 'PROCESSING'})
        with mock.patch.object(release, 'exact_upload', return_value=(upload, build)):
            with self.assertRaisesRegex(ValueError, 'not finished processing'):
                release.accepted_build('public-testflight', '1.0.7', '1015', 0, 'IOS')

    def test_ios_store_operation_stops_before_apple_mutations(self):
        import sys
        with mock.patch.object(sys, 'argv', ['apple-release.py', 'finish', '1.0.7', '1015', 'store', '--platform', 'IOS']), \
             mock.patch.object(release, 'request') as request:
            with self.assertRaisesRegex(ValueError, 'only the externally eligible beta'):
                release.main()
            request.assert_not_called()

    def test_internal_build_cannot_go_to_an_external_group(self):
        group = record("betaGroups", "external", isInternalGroup=False)
        with mock.patch.object(release, "rows", return_value=[group]), mock.patch.object(release, "request") as call:
            with self.assertRaisesRegex(ValueError, "externally"):
                release.distribute(record("builds", "build"), "external")
            call.assert_not_called()

    def test_store_draft_accepts_apple_omitting_a_zero_component(self):
        draft = record("appStoreVersions", "draft", versionString="1.0")
        other = record("appStoreVersions", "other", versionString="1.0.1")
        with mock.patch.object(release, "rows", return_value=[draft, other]):
            self.assertEqual(release.store_version("1.0.0"), draft)

    def test_public_beta_rejects_internal_only_upload(self):
        build = record("builds", "internal", buildAudienceType="INTERNAL_ONLY")
        with mock.patch.object(release, "beta_group") as group, mock.patch.object(release, "create") as create:
            with self.assertRaisesRegex(ValueError, "externally eligible"):
                release.distribute_public(build, "public-group")
            group.assert_not_called()
            create.assert_not_called()

    def test_public_beta_submits_the_exact_build(self):
        build = record("builds", "public-build", buildAudienceType="APP_STORE_ELIGIBLE")
        group = record("betaGroups", "public-group", name="Public Beta", isInternalGroup=False,
                       publicLink="https://testflight.apple.com/join/example")
        detail = record("buildBetaDetails", "detail", externalBuildState="READY_FOR_BETA_SUBMISSION")
        pending = record("buildBetaDetails", "detail", externalBuildState="WAITING_FOR_BETA_REVIEW")
        with mock.patch.object(release, "beta_group", return_value=group), \
             mock.patch.object(release, "beta_notes"), mock.patch.object(release, "patch"), \
             mock.patch.object(release, "request", side_effect=[{"data": detail}, {"data": pending}]), \
             mock.patch.object(release, "add_beta_build") as add, mock.patch.object(release, "create") as create:
            release.distribute_public(build, "public-group")
            create.assert_called_once_with("betaAppReviewSubmissions", {"build": release.link("builds", "public-build")})
            add.assert_called_once_with("public-build", "public-group")

    def test_public_beta_pending_review_does_not_resubmit(self):
        build = record("builds", "public-build", buildAudienceType="APP_STORE_ELIGIBLE")
        group = record("betaGroups", "public-group", name="Public Beta", isInternalGroup=False)
        detail = record("buildBetaDetails", "detail", externalBuildState="IN_BETA_REVIEW")
        with mock.patch.object(release, "beta_group", return_value=group), \
             mock.patch.object(release, "beta_notes"), mock.patch.object(release, "patch"), \
             mock.patch.object(release, "request", return_value={"data": detail}), \
             mock.patch.object(release, "add_beta_build"), mock.patch.object(release, "create") as create:
            release.distribute_public(build, "public-group")
            create.assert_not_called()

    def test_first_subscription_requires_web_review_without_mutating_drafts(self):
        with mock.patch.object(release, "rows", return_value=iter(())) as rows, \
             mock.patch.object(release, "prepare_store") as prepare, \
             mock.patch.object(release, "subscription_items") as subscriptions:
            with self.assertRaisesRegex(ValueError, "first subscription.*appstoreconnect.apple.com"):
                release.submit("1.0.0", record("builds", "build"))
            rows.assert_called_once_with(f"/v1/apps/{release.APP}/subscriptionGroups",
                                         {"filter[subscriptions.state]": "APPROVED"})
            prepare.assert_not_called()
            subscriptions.assert_not_called()

    def test_review_submission_refuses_an_unrelated_draft_item(self):
        draft = record("reviewSubmissions", "draft")
        other = {"relationships": {"appStoreVersion": release.link("appStoreVersions", "different")}}
        approved_group = record("subscriptionGroups", "approved")
        with mock.patch.object(release, "prepare_store", return_value="store-version"), \
             mock.patch.object(release, "subscription_items", return_value={}), \
             mock.patch.object(release, "rows", side_effect=[iter([approved_group]), [draft], [other]]), \
             mock.patch.object(release, "create") as create, mock.patch.object(release, "patch") as patch:
            with self.assertRaisesRegex(ValueError, "unrelated"):
                release.submit("1.0.0", record("builds", "build"))
            create.assert_not_called()
            patch.assert_not_called()

    def test_publish_rejects_versions_not_approved_for_manual_release(self):
        version = record("appStoreVersions", "draft", releaseType="MANUAL", appStoreState="READY_FOR_REVIEW")
        with mock.patch.object(release, "store_version", return_value=version), mock.patch.object(release, "create") as create:
            with self.assertRaisesRegex(ValueError, "approved"):
                release.publish("1.0.0")
            create.assert_not_called()


def pending_record(identifier, version='1.0.8', build='1016', platform='MAC_OS', state='WAITING_FOR_BETA_REVIEW', **changes):
    result = dict(id=identifier, version=version, build=build, platform=platform, state=state,
                  expired=False, processing='VALID', audience='APP_STORE_ELIGIBLE')
    result.update(changes)
    return result


class SupersessionTest(TestCase):
    def test_same_platform_pending_only_and_numeric_order(self):
        records = [pending_record('new'), pending_record('older', version='1.0.6', build='1013', state='IN_BETA_REVIEW'),
                   pending_record('approved', version='1.0.7', build='1014', state='IN_BETA_TESTING'),
                   pending_record('ios', build='1017', platform='IOS'), pending_record('same-version-old', build='99'),
                   pending_record('unsubmitted', build='1000', state='READY_FOR_BETA_SUBMISSION'),
                   pending_record('rejected', build='1001', state='BETA_REJECTED')]
        planned = release.pending_beta_supersession_plan(records)
        self.assertEqual({p['older']['id'] for p in planned}, {'older', 'same-version-old'})
        self.assertTrue(all(p['replacement']['id'] == 'new' for p in planned))

    def test_new_build_must_be_valid_unexpired_and_submitted(self):
        for changes in [dict(processing='PROCESSING'), dict(expired=True), dict(audience='INTERNAL_ONLY'),
                        dict(state='READY_FOR_BETA_SUBMISSION'), dict(state='BETA_REJECTED')]:
            with self.subTest(changes=changes):
                self.assertEqual(release.pending_beta_supersession_plan([pending_record('old', build='1015'), pending_record('new', **changes)]), [])

    def test_version_comparison_and_platform_isolation(self):
        planned = release.pending_beta_supersession_plan([pending_record('old', version='1.0.9'), pending_record('new', version='1.0.10'),
                               pending_record('ios', version='2.0.0', platform='IOS')])
        self.assertEqual([(p['older']['id'], p['replacement']['id']) for p in planned], [('old', 'new')])
        self.assertEqual(release.pending_beta_supersession_plan([pending_record('mac'), pending_record('ios', version='2.0.0', platform='IOS')]), [])

    def test_malformed_versions_and_equal_builds_are_not_candidates(self):
        self.assertEqual(release.pending_beta_supersession_plan([pending_record('old', version='unknown'), pending_record('new')]), [])
        self.assertEqual(release.pending_beta_supersession_plan([pending_record('same'), pending_record('also-same')]), [])

    def simulate(self, apply=True, fresh_state=None, fresh_new_state=None, remove_new=False, confirm=True, repeat=False):
        records = {p['id']: p for p in [pending_record('old', build='1015'), pending_record('new')]}
        detail_reads = {}; membership_reads = 0
        def builds(path):
            nonlocal membership_reads
            membership_reads += 1
            ids = ['old'] if remove_new and membership_reads > 1 else list(records)
            return iter(raw(identifier) for identifier in ids)
        def raw(identifier):
            p = records[identifier]
            return {'id': identifier, 'attributes': {'version': p['build'], 'expired': p['expired'],
                    'processingState': p['processing'], 'buildAudienceType': p['audience']}}
        def get(method, path):
            self.assertEqual(method, 'GET')
            identifier = path.split('/')[3]; p = records[identifier]
            if path.endswith('/preReleaseVersion'):
                return {'data': {'attributes': {'version': p['version'], 'platform': p['platform']}}}
            if path.endswith('/buildBetaDetail'):
                detail_reads[identifier] = detail_reads.get(identifier, 0) + 1
                state = fresh_state if identifier == 'old' and detail_reads[identifier] > 1 and fresh_state else p['state']
                if identifier == 'new' and detail_reads[identifier] > 1 and fresh_new_state: state = fresh_new_state
                return {'data': {'attributes': {'externalBuildState': state}}}
            return {'data': raw(identifier)}
        def expire(kind, identifier, values):
            self.assertEqual((kind, identifier, values), ('builds', 'old', {'expired': True}))
            if confirm: records[identifier].update(expired=True, state='EXPIRED')
        with mock.patch.dict(os.environ, FEI_EXPIRE_SUPERSEDED_BETAS='1'), \
             mock.patch.object(release, 'beta_group', return_value={'id': 'public'}), \
             mock.patch.object(release, 'rows', side_effect=builds), \
             mock.patch.object(release, 'request', side_effect=get), \
             mock.patch.object(release, 'patch', side_effect=expire) as patch:
            first = release.supersede_pending_betas('public', apply)
            second = release.supersede_pending_betas('public', apply) if repeat else None
            return first, second, patch.call_count

    def test_dry_run_never_mutates(self):
        result, _, count = self.simulate(apply=False)
        self.assertEqual(count, 0)
        self.assertEqual(len(result['planned']), 1)

    def test_apply_requires_explicit_enabled_policy(self):
        with mock.patch.dict(os.environ, {}, clear=True), \
             mock.patch.object(release, 'beta_group', return_value={'id': 'public'}), \
             mock.patch.object(release, 'rows', return_value=[]), \
             mock.patch.object(release, 'patch') as patch:
            with self.assertRaises(ValueError): release.supersede_pending_betas('public', True)
            patch.assert_not_called()

    def test_expiration_is_read_back_and_idempotent(self):
        first, second, count = self.simulate(repeat=True)
        self.assertEqual(count, 1)
        self.assertTrue(first['expired'][0]['after']['expired'])
        self.assertEqual(second['expired'], [])

    def test_newly_approved_old_build_is_preserved(self):
        result, _, count = self.simulate(fresh_state='IN_BETA_TESTING')
        self.assertEqual(count, 0)
        self.assertEqual(len(result['skipped']), 1)

    def test_rejected_replacement_prevents_expiration(self):
        result, _, count = self.simulate(fresh_new_state='BETA_REJECTED')
        self.assertEqual(count, 0)
        self.assertEqual(len(result['skipped']), 1)

    def test_changed_membership_prevents_expiration(self):
        result, _, count = self.simulate(remove_new=True)
        self.assertEqual(count, 0)
        self.assertEqual(len(result['skipped']), 1)

    def test_missing_apple_expiration_confirmation_is_an_error(self):
        with self.assertRaisesRegex(ValueError, 'did not confirm'):
            self.simulate(confirm=False)


class SameTrainReviewTest(TestCase):
    def candidate(self, **changes):
        return pending_record('new', platform='IOS', build='1019', state='READY_FOR_BETA_SUBMISSION', **changes)

    def test_only_older_pending_same_platform_and_version(self):
        rows = [pending_record('old', platform='IOS', build='1017'),
                pending_record('approved', platform='IOS', build='1016', state='IN_BETA_TESTING'),
                pending_record('mac', build='1018'),
                pending_record('other-version', platform='IOS', version='1.0.7'),
                pending_record('expired', platform='IOS', build='1015', expired=True)]
        self.assertEqual([x['id'] for x in release.same_train_pending_plan(self.candidate(), rows)], ['old'])

    def test_replacement_must_be_ready_valid_external_and_unexpired(self):
        for changes in [dict(processing='PROCESSING'), dict(audience='INTERNAL_ONLY'), dict(expired=True)]:
            with self.subTest(changes=changes), self.assertRaises(ValueError):
                release.same_train_pending_plan(self.candidate(**changes), [])
        with self.assertRaises(ValueError):
            release.same_train_pending_plan(pending_record('new', state='WAITING_FOR_BETA_REVIEW'), [])

    def test_newer_submitted_build_blocks_a_stale_replacement(self):
        with self.assertRaisesRegex(ValueError, 'newer build'):
            release.same_train_pending_plan(self.candidate(), [pending_record('newer', platform='IOS', build='1020')])

    def simulate(self, fresh=None, confirm=True, enabled=True):
        candidate=self.candidate();old=pending_record('old', platform='IOS', build='1017')
        row=record('builds','old',expired=False,processingState='VALID',buildAudienceType='APP_STORE_ELIGIBLE')
        sequence=[candidate, old, fresh or candidate]
        if fresh is None:sequence.append(old)
        else:sequence.append(dict(old, state='IN_BETA_TESTING'))
        with mock.patch.dict(os.environ, FEI_EXPIRE_SUPERSEDED_BETAS='1' if enabled else '0'), \
             mock.patch.object(release,'rows',return_value=[row]), \
             mock.patch.object(release,'beta_build_snapshot',side_effect=sequence), \
             mock.patch.object(release,'request',return_value={'data':record('builds','old',expired=confirm)}) as request, \
             mock.patch.object(release,'patch') as patch:
            result=release.expire_same_train_pending(record('builds','new'),'public')
            return result,patch.call_args_list,request.call_count

    def test_expiration_is_verified_and_requires_explicit_switch(self):
        result,calls,_=self.simulate();self.assertEqual(result,['old']);self.assertEqual(calls,[mock.call('builds','old',{'expired':True})])
        with self.assertRaisesRegex(ValueError,'not enabled'):self.simulate(enabled=False)
        with self.assertRaisesRegex(ValueError,'did not confirm'):self.simulate(confirm=False)

    def test_approved_predecessor_race_is_preserved(self):
        result,calls,_=self.simulate(fresh=self.candidate());self.assertEqual(result,[]);self.assertEqual(calls,[])

    def test_concurrent_submission_does_not_expire_predecessor(self):
        result,calls,_=self.simulate(fresh=dict(self.candidate(),state='WAITING_FOR_BETA_REVIEW'))
        self.assertEqual(result,[]);self.assertEqual(calls,[])

    def test_initial_submission_race_skips_expiration(self):
        candidate=dict(self.candidate(),state='WAITING_FOR_BETA_REVIEW')
        with mock.patch.dict(os.environ,FEI_EXPIRE_SUPERSEDED_BETAS='1'), \
             mock.patch.object(release,'request',return_value={'data':record('builds','new')}), \
             mock.patch.object(release,'beta_build_snapshot',return_value=candidate), \
             mock.patch.object(release,'rows') as rows, mock.patch.object(release,'patch') as patch:
            self.assertEqual(release.expire_same_train_pending(record('builds','new'),'public'),[])
        rows.assert_not_called();patch.assert_not_called()

    def test_submission_reread_avoids_a_second_post(self):
        build=record('builds','new',buildAudienceType='APP_STORE_ELIGIBLE')
        group=record('betaGroups','public',name='Public Beta')
        ready=record('buildBetaDetails','detail',externalBuildState='READY_FOR_BETA_SUBMISSION')
        pending=record('buildBetaDetails','detail',externalBuildState='WAITING_FOR_BETA_REVIEW')
        with mock.patch.dict(os.environ,FEI_EXPIRE_SUPERSEDED_BETAS='1'), \
             mock.patch.object(release,'beta_group',return_value=group), \
             mock.patch.object(release,'beta_notes'), mock.patch.object(release,'patch'), \
             mock.patch.object(release,'request',side_effect=[{'data':ready},{'data':pending},{'data':pending}]), \
             mock.patch.object(release,'expire_same_train_pending') as expire, \
             mock.patch.object(release,'add_beta_build'), mock.patch.object(release,'create') as create:
            release.distribute_public(build,'public')
        expire.assert_called_once_with(build,'public');create.assert_not_called()

    def test_public_finish_distributes_internally_before_external_review(self):
        import sys
        for platform in ('IOS','MAC_OS'):
            calls=[]
            with mock.patch.dict(os.environ,FEI_BETA_GROUP_ID='internal',FEI_PUBLIC_BETA_GROUP_ID='public',FEI_BETA_NOTES_DIR='/tmp/notes'), \
                 mock.patch.object(sys,'argv',['apple-release.py','finish','1.0.8','1019','public-testflight','--platform',platform]), \
                 mock.patch.object(release,'accepted_build',return_value=record('builds','new')), \
                 mock.patch.object(release,'distribute',side_effect=lambda *a,**k:calls.append('internal')), \
                 mock.patch.object(release,'distribute_public',side_effect=lambda *a,**k:calls.append('external')):
                release.main()
            self.assertEqual(calls,['internal','external'])

    def test_both_workflows_pass_the_existing_expiration_switch(self):
        root=module.parents[2]
        for name in ('fei-apple-release.yml','fei-ios-testflight.yml'):
            self.assertIn('FEI_EXPIRE_SUPERSEDED_BETAS: ${{ vars.FEI_EXPIRE_SUPERSEDED_BETAS }}',(root/'.github/workflows'/name).read_text())


if __name__ == "__main__":
    main()
