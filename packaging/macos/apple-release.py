#!/usr/bin/env python3
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Copyright 2026 Fei Notes contributors. See LICENSES.md.
"""Select exact macOS builds and advance Fei Notes' App Store Connect release."""
import argparse
import base64
import datetime
import json
import os
from pathlib import Path
import plistlib
import re
import subprocess
import sys
import time
from urllib.error import HTTPError, URLError
from urllib.parse import urlencode, urlsplit
from urllib.request import Request, urlopen

APP = "6832909618"
BUNDLE = "app.feinotes.mac"
API = "https://api.appstoreconnect.apple.com"
AUDIENCES = {"testflight": "INTERNAL_ONLY", "public-testflight": "APP_STORE_ELIGIBLE", "store": "APP_STORE_ELIGIBLE"}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def encoded(data):
    return base64.urlsafe_b64encode(data).rstrip(b"=")


def signed_token():
    now = int(time.time())
    claims = {"iss": os.environ["ASC_ISSUER_ID"], "aud": "appstoreconnect-v1", "iat": now, "exp": now + 600}
    header = {"alg": "ES256", "kid": os.environ["ASC_KEY_ID"], "typ": "JWT"}
    message = b".".join(encoded(json.dumps(item).encode()) for item in (header, claims))
    der = subprocess.run(
        ["/usr/bin/openssl", "dgst", "-sha256", "-sign", os.environ["ASC_KEY_PATH"]],
        input=message, capture_output=True, check=True,
    ).stdout
    require(len(der) >= 8 and der[0] == 0x30 and der[1] == len(der) - 2, "Invalid ES256 signature")
    offset = 2
    values = []
    for _ in range(2):
        length = der[offset + 1]
        require(der[offset] == 0x02 and 1 <= length <= 33, "Invalid ES256 integer")
        values.append(int.from_bytes(der[offset + 2:offset + 2 + length], "big").to_bytes(32, "big"))
        offset += length + 2
    require(offset == len(der), "Unexpected ES256 signature data")
    return b".".join((message, encoded(b"".join(values)))).decode()


def request(method, path, data=None, query=None):
    url = API + path if path.startswith("/") else path
    parsed = urlsplit(url)
    require(parsed.scheme == "https" and parsed.netloc == urlsplit(API).netloc, "Unexpected Apple API origin")
    if query:
        url += "?" + urlencode(query)
    body = None if data is None else json.dumps({"data": data}).encode()
    headers = {"Authorization": f"Bearer {signed_token()}", "Content-Type": "application/json"}
    attempts = 3 if method == "GET" else 1
    for attempt in range(attempts):
        last_attempt = attempt == attempts - 1
        try:
            with urlopen(Request(url, data=body, headers=headers, method=method), timeout=35) as response:
                return json.load(response) if response.status != 204 else {}
        except HTTPError as error:
            if last_attempt or error.code not in {408, 429, 500, 502, 503, 504}:
                try:
                    details = json.loads(error.read()).get("errors", [])
                except ValueError:
                    details = []
                summary = "; ".join(f"{item.get('code')}: {item.get('detail')}" for item in details)
                raise ValueError(f"{method} {parsed.path}: {error.code} {summary}") from None
            error.close()
        except (URLError, TimeoutError):
            if last_attempt:
                raise
        time.sleep(2 ** attempt)


def rows(path, query=None):
    while path:
        page = request("GET", path, query=query)
        yield from page["data"]
        path = page.get("links", {}).get("next")
        query = None


def reference(kind, identifier):
    return {"type": kind, "id": identifier}


def link(kind, identifier):
    return {"data": reference(kind, identifier)}


def create(kind, relationships, attributes=None):
    data = {"type": kind, "relationships": relationships}
    if attributes is not None:
        data["attributes"] = attributes
    return request("POST", f"/v1/{kind}", data)["data"]


def patch(kind, identifier, attributes=None, relationships=None):
    data = reference(kind, identifier)
    if attributes is not None:
        data["attributes"] = attributes
    if relationships is not None:
        data["relationships"] = relationships
    request("PATCH", f"/v1/{kind}/{identifier}", data)


def only(items, message):
    matches = list(items)
    require(len(matches) == 1, message)
    return matches[0]


def exact_upload(version, build):
    query = {"filter[platform]": "MAC_OS", "filter[cfBundleShortVersionString]": version,
             "filter[cfBundleVersion]": build, "include": "build"}
    response = request("GET", f"/v1/apps/{APP}/buildUploads", query=query)
    if not response["data"]:
        return None, None
    upload = only(response["data"], "More than one matching Apple upload")
    connected = upload.get("relationships", {}).get("build", {}).get("data")
    builds = (item for item in response.get("included", []) if item["type"] == "builds")
    build_record = only((item for item in builds if connected and item["id"] == connected["id"]),
                        "Apple did not return the referenced build") if connected else None
    return upload, build_record


def accepted_build(channel, version, number, wait):
    until = time.monotonic() + wait
    while True:
        upload, build = exact_upload(version, number)
        state = upload["attributes"]["state"] if upload else {"state": "NOT_FOUND"}
        if state["state"] == "FAILED":
            raise ValueError(f"Apple rejected {version} ({number}): {state.get('errors', [])}")
        if build:
            attrs = build["attributes"]
            require(attrs["buildAudienceType"] == AUDIENCES[channel], "Build audience does not match the release channel")
            require(attrs["version"] == number and not attrs["expired"], "Unexpected or expired build")
            if attrs["processingState"] == "VALID":
                print(f'{version} ({number}) {attrs["buildAudienceType"]} {build["id"]}', flush=True)
                return build
            require(attrs["processingState"] not in {"FAILED", "INVALID"}, "Apple rejected the build")
        if time.monotonic() >= until:
            raise ValueError(f"Apple has not finished processing {version} ({number}): {state['state']}")
        print(f"Waiting for Apple to process {version} ({number}): {state['state']}", flush=True)
        time.sleep(20)


def archive(channel, app, folder, profile_path, app_signer, installer_signer, destination):
    info = plistlib.loads((app / "Contents/Info.plist").read_bytes())
    require(info["CFBundleIdentifier"] == BUNDLE, "Incorrect app bundle")
    internal = channel == "testflight"
    beta = channel in {"testflight", "public-testflight"}
    require(info.get("FeiTestFlightBeta", False) is beta, "App package does not match channel")
    products = [info.get(f"FeiStoreKit{plan}Product") for plan in ("Annual", "Monthly")]
    require(products == [None, None] if beta else all(products) and products[0] != products[1],
            "Unexpected subscription configuration")
    profile = plistlib.loads(subprocess.check_output(["security", "cms", "-D", "-i", str(profile_path)]))
    team = only(profile["TeamIdentifier"], "Expected one signing team")
    signature = subprocess.run(["codesign", "-dv", "--verbose=4", str(app)], capture_output=True, text=True, check=True)
    identity = re.search(r"^Authority=(.*)$", signature.stderr, re.MULTILINE)[1]
    path = folder / "Fei Notes.xcarchive"
    apps = path / "Products/Applications"
    apps.mkdir(parents=True)
    subprocess.run(["ditto", str(app), str(apps / app.name)], check=True)
    properties = {key: info[key] for key in ("CFBundleIdentifier", "CFBundleShortVersionString", "CFBundleVersion")}
    properties.update(ApplicationPath=f"Applications/{app.name}", SigningIdentity=identity, Team=team)
    properties["Architectures"] = subprocess.check_output(["lipo", "-archs", str(app / "Contents/MacOS/quiet_md")], text=True).split()
    archive_info = {"ArchiveVersion": 2, "CreationDate": datetime.datetime.now(datetime.timezone.utc).replace(tzinfo=None),
                    "Name": "Fei Notes", "SchemeName": "Fei Notes", "ApplicationProperties": properties}
    (path / "Info.plist").write_bytes(plistlib.dumps(archive_info))
    options = {"method": "app-store-connect", "destination": destination, "teamID": team,
               "signingStyle": "manual", "signingCertificate": app_signer, "installerSigningCertificate": installer_signer,
               "provisioningProfiles": {BUNDLE: profile["UUID"]}, "manageAppVersionAndBuildNumber": False,
               "stripSwiftSymbols": False, "uploadSymbols": False, "testFlightInternalTestingOnly": internal}
    (folder / "ExportOptions.plist").write_bytes(plistlib.dumps(options))


def beta_notes(build):
    identifier = build["id"]
    require(build["attributes"].get("usesNonExemptEncryption") is not None,
            "Answer Apple's export-compliance question for this build before distribution")
    notes = {
        "en-US": "Write a local note, then turn on iCloud Sync. Edit the same sample note on a second Mac. You can also share and edit a sample note with a second Apple Account. Cloud access is included during TestFlight testing without a subscription.",
        "zh-Hans": "先写一篇本机笔记，再开启 iCloud 同步，在第二台 Mac 编辑同一篇样本笔记。也可以邀请另一个 Apple 账户一起编辑。TestFlight 测试期间可直接使用云功能，无需订阅。",
    }
    support_url = os.environ.get("FEI_SUPPORT_URL")
    if support_url:
        notes["en-US"] += f"\n\nFeedback: {support_url}"
        notes["zh-Hans"] += f"\n\n反馈入口：{support_url}"
    current = {row["attributes"]["locale"]: row for row in rows(f"/v1/builds/{identifier}/betaBuildLocalizations")}
    for locale, text in notes.items():
        if locale not in current:
            create("betaBuildLocalizations", {"build": link("builds", identifier)}, {"locale": locale, "whatsNew": text})
        elif current[locale]["attributes"].get("whatsNew") != text:
            patch("betaBuildLocalizations", current[locale]["id"], {"whatsNew": text})


def beta_group(group_id, internal):
    group = only((item for item in rows(f"/v1/apps/{APP}/betaGroups") if item["id"] == group_id),
                 "Configured beta group is not linked to Fei Notes")
    require(group["attributes"]["isInternalGroup"] is internal,
            "Refusing to distribute the internal build externally" if internal else "Expected an external TestFlight group")
    return group


def add_beta_build(identifier, group_id):
    existing = list(rows(f"/v1/betaGroups/{group_id}/builds"))
    if identifier not in {item["id"] for item in existing}:
        request("POST", f"/v1/builds/{identifier}/relationships/betaGroups", [reference("betaGroups", group_id)])
    require(any(item["id"] == identifier for item in rows(f"/v1/betaGroups/{group_id}/builds")),
            "Could not verify the exact build in the intended TestFlight group")


def distribute(build, group_id):
    group = beta_group(group_id, internal=True)
    beta_notes(build)
    identifier = build["id"]
    add_beta_build(identifier, group_id)
    state = request("GET", f"/v1/builds/{identifier}/buildBetaDetail")["data"]["attributes"]["internalBuildState"]
    print(f"Internal group: {group['attributes']['name']}, internal state: {state}")
    require(state in {"IN_BETA_TESTING", "READY_FOR_BETA_TESTING"}, f"TestFlight is not ready: {state}")


def distribute_public(build, group_id):
    require(build["attributes"]["buildAudienceType"] == "APP_STORE_ELIGIBLE",
            "External TestFlight requires an externally eligible upload")
    group = beta_group(group_id, internal=False)
    beta_notes(build)
    identifier = build["id"]
    detail = request("GET", f"/v1/builds/{identifier}/buildBetaDetail")["data"]
    patch("buildBetaDetails", detail["id"], {"autoNotifyEnabled": True})
    state = detail["attributes"]["externalBuildState"]
    if state == "READY_FOR_BETA_SUBMISSION":
        # Apple may finish a prior request after a network interruption.
        # Re-read the build state on the next run instead of retrying POST.
        create("betaAppReviewSubmissions", {"build": link("builds", identifier)})
    else:
        require(state in {"WAITING_FOR_BETA_REVIEW", "IN_BETA_REVIEW", "BETA_APPROVED",
                          "READY_FOR_BETA_TESTING", "IN_BETA_TESTING"},
                f"External TestFlight needs attention: {state}")
    add_beta_build(identifier, group_id)
    state = request("GET", f"/v1/builds/{identifier}/buildBetaDetail")["data"]["attributes"]["externalBuildState"]
    print(f"External group: {group['attributes']['name']}, review state: {state}")
    print(f"Public invitation: {group['attributes'].get('publicLink') or 'not enabled'}")


def public_beta_status(output):
    group = beta_group(os.environ["FEI_PUBLIC_BETA_GROUP_ID"], internal=False)
    builds = [item for item in rows(f"/v1/betaGroups/{group['id']}/builds")
              if not item["attributes"].get("expired") and item["attributes"].get("processingState") == "VALID"
              and item["attributes"].get("buildAudienceType") == "APP_STORE_ELIGIBLE"]
    status = {"state": "unavailable", "url": group["attributes"].get("publicLink")}
    if builds:
        build = max(builds, key=lambda item: item["attributes"]["uploadedDate"])
        detail = request("GET", f"/v1/builds/{build['id']}/buildBetaDetail")["data"]["attributes"]
        version = request("GET", f"/v1/builds/{build['id']}/preReleaseVersion")["data"]["attributes"]["version"]
        state = detail["externalBuildState"]
        available = state in {"READY_FOR_BETA_TESTING", "IN_BETA_TESTING"}
        reviewing = state in {"WAITING_FOR_BETA_REVIEW", "IN_BETA_REVIEW", "READY_FOR_BETA_SUBMISSION"}
        status.update(state="available" if available else "review" if reviewing else "unavailable",
                      version=version, build=build["attributes"]["version"])
    previous = json.loads(output.read_text()) if output.exists() else {}
    if {k: v for k, v in previous.items() if k != "updatedAt"} != status:
        status["updatedAt"] = datetime.datetime.now(datetime.timezone.utc).isoformat()
        output.write_text(json.dumps(status, indent=2) + "\n")
    print(json.dumps(status))


def store_version(version):
    def normalized(value):
        parts = [int(part) for part in value.split(".")]
        return tuple((parts + [0, 0])[:3])

    versions = rows(f"/v1/apps/{APP}/appStoreVersions", {"filter[platform]": "MAC_OS"})
    return only((item for item in versions if normalized(item["attributes"]["versionString"]) == normalized(version)),
                "Expected one existing macOS version draft")


def prepare_store(version, build):
    require(build["attributes"].get("usesNonExemptEncryption") is not None,
            "Answer Apple's export-compliance question for the store build")
    draft = store_version(version)
    require(draft["attributes"]["appStoreState"] in {"PREPARE_FOR_SUBMISSION", "READY_FOR_REVIEW"},
            "Store version is not an editable draft")
    patch("appStoreVersions", draft["id"], {"releaseType": "MANUAL"}, {"build": link("builds", build["id"])})
    linked = request("GET", f'/v1/appStoreVersions/{draft["id"]}/relationships/build')["data"]
    require(linked == reference("builds", build["id"]), "Store draft is linked to a different build")
    print(f"App Store draft {version} uses build {build['attributes']['version']} with manual release")
    return draft["id"]


def subscription_items():
    group_id = os.environ["FEI_SUBSCRIPTION_GROUP_ID"]
    group = only((item for item in rows(f"/v1/apps/{APP}/subscriptionGroups") if item["id"] == group_id),
                 "Configured subscription group is not linked to Fei Notes")
    products = {item["attributes"]["productId"]: item for item in rows(f"/v1/subscriptionGroups/{group['id']}/subscriptions")}
    targets = []
    for variable, period in [("FEI_STOREKIT_ANNUAL_PRODUCT", "ONE_YEAR"), ("FEI_STOREKIT_MONTHLY_PRODUCT", "ONE_MONTH")]:
        product = products.get(os.environ[variable])
        require(product and product["attributes"]["subscriptionPeriod"] == period, f"Wrong subscription for {variable}")
        targets.append(("subscription", "subscriptions", product["id"]))
    targets.insert(0, ("subscriptionGroup", "subscriptionGroups", group_id))
    items = {}
    for kind, resource, identifier in targets:
        versions = list(rows(f"/v1/{resource}/{identifier}/versions"))
        pending = [item for item in versions if item["attributes"]["state"] in {"PREPARE_FOR_SUBMISSION", "READY_FOR_REVIEW"}]
        if pending:
            item = only(pending, f"Multiple reviewable versions for {kind}")
            items[kind + "Version", item["id"]] = reference(item["type"], item["id"])
        else:
            require(any(item["attributes"]["state"] == "APPROVED" for item in versions),
                    f"No reviewable or approved version for {kind}")
    return items


def submit(version, build):
    approved_groups = rows(f"/v1/apps/{APP}/subscriptionGroups", {"filter[subscriptions.state]": "APPROVED"})
    require(next(approved_groups, None) is not None,
            "Apple requires the first subscription and app version to be submitted together on appstoreconnect.apple.com")
    expected = subscription_items()
    version_id = prepare_store(version, build)
    expected["appStoreVersion", version_id] = reference("appStoreVersions", version_id)
    drafts = list(rows(f"/v1/apps/{APP}/reviewSubmissions",
                       {"filter[platform]": "MAC_OS", "filter[state]": "READY_FOR_REVIEW"}))
    if drafts:
        draft = only(drafts, "Multiple macOS review drafts need human selection")
    else:
        draft = create("reviewSubmissions", {"app": link("apps", APP)}, {"platform": "MAC_OS"})
    draft_id = draft["id"]

    def current_items():
        include = ",".join(sorted({key for key, _ in expected}))
        current = set()
        for item in rows(f"/v1/reviewSubmissions/{draft_id}/items", {"include": include}):
            relationships = item.get("relationships", {})
            matches = {(key, rel["data"]["id"]) for key, rel in relationships.items()
                       if isinstance(rel.get("data"), dict) and "id" in rel["data"]} & expected.keys()
            require(len(matches) == 1, "The existing draft has an unrelated or unidentifiable review item")
            current.update(matches)
        return current

    existing = current_items()
    for key, ref in expected.items():
        if key not in existing:
            create("reviewSubmissionItems", {"reviewSubmission": link("reviewSubmissions", draft_id),
                                             key[0]: {"data": ref}})
    require(current_items() == expected.keys(), "Review draft does not contain the expected app and subscription items")
    patch("reviewSubmissions", draft_id, {"submitted": True})
    state = request("GET", f"/v1/reviewSubmissions/{draft_id}")["data"]["attributes"]["state"]
    print(f"Apple review submission {draft_id}: {state}")
    require(state in {"WAITING_FOR_REVIEW", "IN_REVIEW", "COMPLETING", "COMPLETE"}, "Apple has not accepted the review request")


def publish(version):
    draft = store_version(version)
    attrs = draft["attributes"]
    require(attrs["releaseType"] == "MANUAL" and attrs["appStoreState"] == "PENDING_DEVELOPER_RELEASE",
            "Only an approved macOS version pending manual release can be published")
    create("appStoreVersionReleaseRequests", {"appStoreVersion": link("appStoreVersions", draft["id"])})
    print(f"Requested release of approved macOS {version}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    ops = parser.add_subparsers(dest="operation", required=True)
    pack = ops.add_parser("archive")
    pack.add_argument("channel", choices=AUDIENCES)
    for arg in ("app", "folder", "profile"):
        pack.add_argument(arg, type=Path)
    pack.add_argument("app_signer")
    pack.add_argument("installer_signer")
    pack.add_argument("destination", choices=("export", "upload"))
    status = ops.add_parser("beta-status")
    status.add_argument("output", type=Path)
    for name in ("check-upload", "finish", "submit", "publish"):
        cmd = ops.add_parser(name)
        cmd.add_argument("version")
        if name != "publish":
            cmd.add_argument("build")
        if name in {"check-upload", "finish"}:
            cmd.add_argument("channel", choices=AUDIENCES)
        if name in {"submit", "publish"}:
            cmd.add_argument("--confirm", required=True)
    args = parser.parse_args()
    if args.operation == "archive":
        archive(args.channel, args.app, args.folder, args.profile, args.app_signer, args.installer_signer, args.destination)
        return
    if args.operation == "beta-status":
        public_beta_status(args.output)
        return
    require(re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", args.version), "Expected a three-component version")
    if args.operation != "publish":
        require(re.fullmatch(r"[1-9][0-9]{0,3}", args.build), "Expected a positive integer build number")
    if args.operation == "check-upload":
        upload, build = exact_upload(args.version, args.build)
        if upload:
            state = upload["attributes"]["state"]["state"]
            require(state in {"COMPLETE", "PROCESSING"} or build, f"Upload already exists in state {state}. Check Apple before retrying.")
            if build:
                require(build["attributes"]["buildAudienceType"] == AUDIENCES[args.channel], "Existing build is for another channel")
        print("existing" if upload else "new")
    elif args.operation == "finish":
        build = accepted_build(args.channel, args.version, args.build, 1200)
        if args.channel == "testflight":
            distribute(build, os.environ["FEI_BETA_GROUP_ID"])
        elif args.channel == "public-testflight":
            distribute_public(build, os.environ["FEI_PUBLIC_BETA_GROUP_ID"])
        else:
            prepare_store(args.version, build)
    elif args.operation == "submit":
        require(args.confirm == f"submit:{args.version}:{args.build}", "The submission confirmation does not match this build")
        submit(args.version, accepted_build("store", args.version, args.build, 0))
    elif args.operation == "publish":
        require(args.confirm == f"publish:{args.version}", "The release confirmation does not match this version")
        publish(args.version)


if __name__ == "__main__":
    try:
        main()
    except (KeyError, ValueError, OSError, subprocess.SubprocessError) as error:
        sys.exit(str(error))
