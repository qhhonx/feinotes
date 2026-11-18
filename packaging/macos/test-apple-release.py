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


if __name__ == "__main__":
    main()
