#!/usr/bin/env python3
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Copyright 2026 Fei Notes contributors. See LICENSES.md.
"""Exercise status publishing with a remote that rejects all product updates."""
import importlib.util
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("website_status", Path(__file__).with_name("website-status.py"))
status = importlib.util.module_from_spec(spec)
spec.loader.exec_module(status)


class StatusPublication(unittest.TestCase):
    def test_protected_product_and_repeatable_site_overlay(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            remote, repo = root / "remote.git", root / "checkout"
            subprocess.run(["git", "init", "--bare", str(remote)], check=True, capture_output=True)
            subprocess.run(["git", "clone", str(remote), str(repo)], check=True, capture_output=True)
            for key, value in [("user.name", "Status fixture"), ("user.email", "fixture@example.invalid")]:
                status.git(repo, "config", key, value)
            (repo / "application.txt").write_text("Application source stays fixed.\n")
            status.git(repo, "add", ".")
            status.git(repo, "commit", "-m", "Fixture application")
            status.git(repo, "push", "origin", "HEAD:refs/heads/product/quiet-sync")
            product = status.git(repo, "rev-parse", "HEAD")
            hook = remote / "hooks/pre-receive"
            hook.write_text('#!/bin/sh\nwhile read old new ref; do\n  if [ "$ref" = refs/heads/product/quiet-sync ]; then exit 1; fi\ndone\n')
            hook.chmod(0o755)
            platform = {"state": "available", "url": "https://testflight.apple.com/join/NdcXugMy", "version": "1.0.10", "build": "1022"}
            value = dict(platform, platforms={"MAC_OS": dict(platform), "IOS": dict(platform)})
            snapshot = root / "snapshot.json"
            snapshot.write_text(json.dumps(value) + "\n")
            site = repo / "website"
            site.mkdir()
            (site / "index.html").write_text("Keep the website unchanged.")
            (site / status.FILE).write_text(snapshot.read_text())
            self.assertEqual(status.stage(repo, site)["source"], "repository snapshot")
            first = status.publish(repo, snapshot)
            self.assertTrue(first["changed"])
            self.assertEqual(status.git(repo, "ls-tree", "--name-only", first["commit"]), status.FILE)
            self.assertFalse(status.publish(repo, snapshot)["changed"])
            value["platforms"]["IOS"]["pending"] = {"state": "review", "version": "1.1.0", "build": "1032"}
            snapshot.write_text(json.dumps(value) + "\n")
            second = status.publish(repo, snapshot)
            self.assertTrue(second["changed"])
            self.assertEqual(status.git(repo, "rev-parse", second["commit"] + "^"), first["commit"])
            self.assertEqual(status.stage(repo, site)["commit"], second["commit"])
            self.assertEqual(json.loads((site / status.FILE).read_text()), value)
            self.assertEqual((site / "index.html").read_text(), "Keep the website unchanged.")
            self.assertEqual(status.git(repo, "rev-parse", "HEAD"), product)
            self.assertTrue(status.git(repo, "ls-remote", "origin", "refs/heads/product/quiet-sync").startswith(product))

    def test_invalid_invitation_is_rejected_before_publication(self):
        platform = {"state": "available", "url": "https://example.invalid/join/invalid"}
        with self.assertRaises(ValueError):
            status.validate(json.dumps(dict(platform, platforms={"MAC_OS": platform, "IOS": platform})))

    def test_missing_invitation_can_only_disable_downloads(self):
        platform = {"state": "unavailable", "url": None}
        value = dict(platform, platforms={"MAC_OS": dict(platform), "IOS": dict(platform)})
        self.assertEqual(status.validate(json.dumps(value)), value)
        value["platforms"]["IOS"]["state"] = "available"
        with self.assertRaises(ValueError):
            status.validate(json.dumps(value))


if __name__ == "__main__":
    unittest.main()
