# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Copyright 2026 Fei Notes contributors. See LICENSES.md.
"""Offline checks for the release boundaries. No test calls Apple."""
import importlib.util
from pathlib import Path
from unittest import TestCase, main, mock
from urllib.error import URLError

module = Path(__file__).with_name("apple-release.py")
spec = importlib.util.spec_from_file_location("apple_release", module)
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


def record(kind, identifier, **attributes):
    return {"id": identifier, "type": kind, "attributes": attributes}


class AppleReleaseTest(TestCase):
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
