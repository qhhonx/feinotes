#!/usr/bin/env python3
"""Prepare and verify the in-app notices for the locked mobile dependency graph."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
OUTPUT = HERE / "Resources"
TARGET = "aarch64-apple-ios"


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check():
    manifest = json.loads((OUTPUT / "notices-manifest.json").read_text())
    for name, digest in manifest["inputs"].items():
        if sha(ROOT / name) != digest:
            raise SystemExit(f"Stale iOS notices input: {name}. Run packaging/ios/prepare-notices.py.")
    if sha(OUTPUT / "Open Source Licenses.txt") != manifest["output_sha256"]:
        raise SystemExit("The bundled iOS notices changed. Run packaging/ios/prepare-notices.py.")
    if sha(OUTPUT / "notices-inputs.xcfilelist") != manifest["file_list_sha256"]:
        raise SystemExit("The notices build inputs changed. Run packaging/ios/prepare-notices.py.")
    if sha(OUTPUT / "Open Source Licenses.json") != manifest["catalogue_sha256"]:
        raise SystemExit("The iOS notices catalogue changed. Run packaging/ios/prepare-notices.py.")
    print("Verified iOS notices for the locked dependency graph.")


def generate():
    spec = importlib.util.spec_from_file_location("fei_release_licenses", ROOT / "scripts/fei-release-licenses.py")
    collector = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(collector)
    supplemental = json.loads(collector.SUPPLEMENT.read_text())
    extra = json.loads((HERE / "dependency-notices.json").read_text())
    for key in ("components", "texts"):
        for name, value in extra[key].items():
            if name in supplemental[key] and supplemental[key][name] != value:
                raise SystemExit(f"Conflicting supplemental notice: {name}")
            supplemental[key][name] = value
    cargo_manifest = HERE / "Cargo.toml"
    packages = collector.selected_packages(TARGET, cargo_manifest, "fei_notes_ipad")
    simulator = collector.selected_packages("aarch64-apple-ios-sim", cargo_manifest, "fei_notes_ipad")
    inventory = lambda rows: sorted((p["name"], p["version"], p["license"]) for p in rows)
    if inventory(packages) != inventory(simulator):
        raise SystemExit("Device and simulator dependency graphs differ; prepare their combined notices explicitly.")
    rust_notices = collector.build_notice(TARGET, supplemental, cargo_manifest, "fei_notes_ipad")
    sources = [
        ("Quiet.md — MIT License", "LICENSE"),
        ("Fei Notes Source License Scope", "LICENSES.md"),
        ("Fei Notes — PolyForm Noncommercial 1.0.0", "LICENSE-FEINOTES.md"),
        ("Repository Notices", "THIRD_PARTY.md"),
        ("Lucide Interface Icons", "assets/icons/LICENSE-lucide"),
        ("Lucide Editor Icons", "assets/editor-icons/LICENSE-lucide"),
        ("KaTeX", "assets/fonts/katex/LICENSE"),
    ]
    sections = ["Fei Notes / 飞记\nOpen Source Licenses / 开源许可"]
    catalogue = {"documents": {}, "entries": []}
    def entry(title, subtitle, text):
        parts = []; start = 0
        while start < len(text):
            end = min(start + 6000, len(text))
            if end < len(text):
                line = text.rfind("\n", start + 1, end)
                if line > start: end = line + 1
            page = text[start:end]; key = hashlib.sha256(page.encode()).hexdigest()
            catalogue["documents"][key] = page; parts.append(key); start = end
        assert "".join(catalogue["documents"][key] for key in parts) == text
        catalogue["entries"].append({"title": title, "subtitle": subtitle, "parts": parts})
    for title, name in sources:
        text = (ROOT / name).read_text()
        sections.append(title + "\n\n" + text)
        entry(title, "Fei Notes", text)
    sections.append("Rust Dependencies in This Build\n\n" + rust_notices)
    for package in packages:
        title = package["name"] + " " + package["version"]
        subtitle = package["license"] or "See license text"
        body = title + "\nLicense: " + subtitle + "\nSource: " + collector.package_source(package) + "\n\n"
        for origin, text in collector.package_notices(package, supplemental):
            body += origin + "\n\n" + text + "\n\n"
        entry(title, subtitle, body)
    inputs = [name for _, name in sources] + [
        "scripts/fei-release-licenses.py", "packaging/macos/dependency-notices.json",
        "packaging/ios/prepare-notices.py", "packaging/ios/dependency-notices.json",
        "packaging/ios/Cargo.toml", "packaging/ios/Cargo.lock",
        "packaging/ios/.gpui-mobile/Cargo.toml", "packaging/ios/.gpui-mobile/LICENSE-APACHE",
        "packaging/ios/.gpui-mobile/LICENSE-AGPL", "packaging/ios/.gpui-mobile/LICENSE-GPL",
        "packaging/ios/.gpui-core/Cargo.toml", "packaging/ios/.gpui-core/LICENSE-APACHE",
    ]
    OUTPUT.mkdir(exist_ok=True)
    output = OUTPUT / "Open Source Licenses.txt"
    output.write_text("\n\n".join(sections) + "\n")
    catalogue_file = OUTPUT / "Open Source Licenses.json"
    catalogue_file.write_text(json.dumps(catalogue, ensure_ascii=False) + "\n")
    file_list = OUTPUT / "notices-inputs.xcfilelist"
    listed = inputs + ["packaging/ios/Resources/notices-manifest.json", "packaging/ios/Resources/Open Source Licenses.txt", "packaging/ios/Resources/Open Source Licenses.json"]
    file_list.write_text("".join("$(SRCROOT)/../../" + name + "\n" for name in listed))
    record = {"target": TARGET, "simulator_same_dependency_versions": True,
              "package": "fei_notes_ipad", "features": ["cloudkit-sync"],
              "third_party_packages": [{"name": n, "version": v, "license": l} for n, v, l in inventory(packages)],
              "inputs": {name: sha(ROOT / name) for name in inputs}, "output_sha256": sha(output), "file_list_sha256": sha(file_list), "catalogue_sha256": sha(catalogue_file)}
    (OUTPUT / "notices-manifest.json").write_text(json.dumps(record, indent=2) + "\n")
    check()


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    args = parser.parse_args()
    check() if args.check else generate()
