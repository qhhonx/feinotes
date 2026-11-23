#!/usr/bin/env python3
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Copyright 2026 Fei Notes contributors. See LICENSES.md.
"""Keep generated public beta data outside the protected application branch."""
import argparse
import json
import os
from pathlib import Path
import subprocess

BRANCH = "automation/testflight-status"
REF = f"refs/heads/{BRANCH}"
FILE = "testflight-status.json"


def git(repo, *args, data=None, env=None):
    return subprocess.run(["git", "-C", str(repo), *args], input=data, text=True,
                          capture_output=True, check=True, env=env).stdout.strip()


def validate(raw):
    value = json.loads(raw)
    if not isinstance(value, dict) or set(value.get("platforms", {})) != {"MAC_OS", "IOS"}:
        raise ValueError("Expected separate Mac and iOS availability")
    for status in [value, *value["platforms"].values()]:
        if status.get("state") not in {"available", "review", "unavailable"}:
            raise ValueError("Unexpected public beta state")
        url = status.get("url")
        if url != "https://testflight.apple.com/join/NdcXugMy" and (url is not None or status["state"] == "available"):
            raise ValueError("Unexpected public TestFlight invitation")
    return value


def current(repo):
    remote = git(repo, "ls-remote", "--heads", "origin", REF)
    if not remote:
        return None
    git(repo, "fetch", "--no-tags", "--depth=1", "origin", REF)
    return git(repo, "rev-parse", "FETCH_HEAD")


def publish(repo, path):
    raw = Path(path).read_text()
    validate(raw)
    parent = current(repo)
    if parent and git(repo, "show", f"{parent}:{FILE}") == raw.strip():
        return {"changed": False, "commit": parent}
    blob = git(repo, "hash-object", "-w", "--stdin", data=raw)
    tree = git(repo, "mktree", data=f"100644 blob {blob}\t{FILE}\n")
    env = dict(os.environ, GIT_AUTHOR_NAME="github-actions[bot]", GIT_COMMITTER_NAME="github-actions[bot]",
               GIT_AUTHOR_EMAIL="41898282+github-actions[bot]@users.noreply.github.com",
               GIT_COMMITTER_EMAIL="41898282+github-actions[bot]@users.noreply.github.com")
    parents = ["-p", parent] if parent else []
    commit = git(repo, "commit-tree", tree, *parents, data="Update public TestFlight availability\n", env=env)
    # A normal push rejects a concurrent writer; never force or update the product branch.
    git(repo, "push", "origin", f"{commit}:{REF}")
    return {"changed": True, "commit": commit}


def stage(repo, site):
    destination = Path(site) / FILE
    commit = current(repo)
    if commit:
        raw = git(repo, "show", f"{commit}:{FILE}") + "\n"
        validate(raw)
        destination.write_text(raw)
    else:
        # Bootstrap deployments retain the repository snapshot until the first status run.
        validate(destination.read_text())
    return {"commit": commit, "source": BRANCH if commit else "repository snapshot"}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["publish", "stage"])
    parser.add_argument("path", type=Path)
    args = parser.parse_args()
    result = publish(Path.cwd(), args.path) if args.command == "publish" else stage(Path.cwd(), args.path)
    if args.command == "publish" and os.environ.get("GITHUB_OUTPUT"):
        with open(os.environ["GITHUB_OUTPUT"], "a") as output:
            output.write(f"changed={str(result['changed']).lower()}\ncommit={result['commit']}\n")
    print(json.dumps(result))
