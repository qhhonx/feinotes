#!/usr/bin/env python3
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Copyright 2026 Fei Notes contributors. See LICENSES.md.
"""Collect license notices for the selected Fei Notes macOS Cargo build."""

import argparse
import json
from pathlib import Path
import re
import subprocess


REPO = Path(__file__).resolve().parents[1]
SUPPLEMENT = REPO / "packaging/macos/dependency-notices.json"
NOTICE_FILE = re.compile(r"^(license|licence|copying|copyright|notice|unlicense)(?:[_.-]|$)", re.I)
OWN_PACKAGES = {"fdl_core", "quiet_md"}


def cargo(*args):
    return subprocess.check_output(["cargo", *args], cwd=REPO, text=True)


def selected_packages(target):
    features = ("--locked", "--offline", "--no-default-features", "--features", "cloudkit-sync")
    metadata = json.loads(cargo("metadata", "--format-version=1", "--filter-platform", target, *features))
    tree = cargo("tree", "-p", "quiet_md", "--target", target, *features,
                 "--edges", "normal,no-proc-macro", "--prefix", "none", "--format", "{p}")
    active = {tuple(re.match(r"^(\S+) v(\S+)", line).groups()) for line in tree.splitlines()}
    packages = [package for package in metadata["packages"] if (package["name"], package["version"]) in active]
    if len(packages) != len(active):
        raise SystemExit("Could not identify every package selected for the release build.")
    return sorted(
        (p for p in packages if p["source"] or p["name"] not in OWN_PACKAGES),
        key=lambda p: (p["name"], p["version"]),
    )


def package_notices(package, supplemental):
    root = Path(package["manifest_path"]).parent
    files = {p for p in root.iterdir() if p.is_file() and NOTICE_FILE.match(p.name)}
    if package["license_file"]:
        files.add(root / package["license_file"])
    notices = [(p.name, p.read_text(encoding="utf-8-sig")) for p in sorted(files)]
    component = f"{package['name']}@{package['version']}"
    for notice in supplemental["components"].get(component, []):
        notices.append((notice["source"], supplemental["texts"][notice["text"]]))
    return notices


def package_source(package):
    registry = package["source"] == "registry+https://github.com/rust-lang/crates.io-index"
    if registry:
        return f"https://crates.io/api/v1/crates/{package['name']}/{package['version']}/download"
    if "MPL" in (package["license"] or ""):
        raise SystemExit(f"An exact MPL source archive is required for {package['name']}.")
    return package["repository"] or "Bundled source in the project checkout"


def build_notice(target, supplemental):
    packages = selected_packages(target)
    blocks = {}
    components = []
    mpl_sources = []
    missing = []
    for package in packages:
        name, version, license_ = (package[k] for k in ("name", "version", "license"))
        source = package_source(package)
        authors = [re.sub(r"\s*<[^>]*>", "", a) for a in package["authors"]]
        notices = package_notices(package, supplemental)
        if not notices:
            missing.append(f"{name} {version}")
            continue
        components.extend([f"{name} {version}  |  {license_ or 'See license text'}", f"Source: {source}"])
        if authors:
            components.append(f"Authors: {', '.join(authors)}")
        for origin, text in notices:
            block = blocks.setdefault(text.rstrip(), len(blocks) + 1)
            components.append(f"Notice {block:03d}: {origin}")
        components.append("")
        if "MPL" in (license_ or ""):
            mpl_sources.append(f"{name} {version}: {source}")
    if missing:
        raise SystemExit("Missing license notices for: " + ", ".join(missing))

    lines = [
        "Fei Notes — Rust dependency notices",
        f"{target} | cloudkit-sync | {len(packages)} third-party packages",
        "",
        "This inventory follows the normal Cargo dependency graph for this build",
        "and excludes procedural macros. License choices below are package declarations.",
        "The notices are reproduced once when their texts are identical.",
        "",
        "MPL 2.0 source code",
        "Download the corresponding Source Code Form for these dependencies at no charge:",
        *mpl_sources,
        "",
        "Component inventory",
        "",
        *components,
        "License texts",
        "",
    ]
    for text, number in blocks.items():
        lines.extend([f"--- Notice {number:03d} ---", text, ""])
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("target", choices=("aarch64-apple-darwin", "x86_64-apple-darwin"))
    parser.add_argument("output_directory", type=Path)
    args = parser.parse_args()
    supplemental = json.loads(SUPPLEMENT.read_text(encoding="utf-8"))
    notice = build_notice(args.target, supplemental)
    args.output_directory.mkdir(parents=True, exist_ok=True)
    (args.output_directory / "Open Source Notices.txt").write_text(notice, encoding="utf-8")


if __name__ == "__main__":
    main()
