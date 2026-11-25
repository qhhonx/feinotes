"""What an import brings in is in front as soon as it finishes: a Markdown
file opens, an imported folder (Markdown, an Obsidian vault) shows its page,
a 思源笔记 workspace its 导入报告.

Build with --release --features devtools, then run this file. Each import
goes through 我的文档's "+" menu and a `pick` instead of the system panel,
into a temporary library; `showing` checks what is in front. No system
clipboard or user library is accessed. Pass case names to run only those.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

here = Path(__file__).resolve().parent
repo = here.parent.parent
work = Path(tempfile.mkdtemp(prefix="quiet-import-open-"))
source = work / "source"
(source / "docs").mkdir(parents=True)
DOC = "aaaaaaaa-0000-0000-0000-000000000001"
(source / "docs" / f"{DOC}.xml").write_text("<title>Start</title>\n<p>start</p>\n")
(source / "index.json").write_text(json.dumps({DOC: {"created": 1, "modified": 1, "opened": 1}}))
(source / "preferences.json").write_text(json.dumps({
    "appearance": "light", "english": True, "language": "en", "seen_version": "9.9.9",
    "auto_update": False, "quick_hotkey": False,
}))

files = work / "files"
(files / "notes").mkdir(parents=True)
(files / "Single.md").write_text("# Single\n\nOne file.\n")
(files / "notes" / "a.md").write_text("# A\n\nIn a folder.\n")
(files / "notes" / "b.md").write_text("# B\n\nAlso.\n")
(files / "Vault" / ".obsidian").mkdir(parents=True)
(files / "Vault" / ".obsidian" / "app.json").write_text("{}")
(files / "Vault" / "Daily.md").write_text("# Daily\n\nSee [[Other]].\n")
(files / "Vault" / "Other.md").write_text("# Other\n\nlinked\n")
shutil.copytree(repo / "crates/core/tests/fixtures/siyuan", files / "siyuan")

# The "+" menu under 我的文档 (English, no 数据表): its 导入 rows.
ROW = {"md": 308, "folder": 340, "siyuan": 372}
CASES = {
    "markdown_file": (ROW["md"], files / "Single.md", "Single"),
    "markdown_folder": (ROW["folder"], files / "notes", "notes"),
    "obsidian_vault": (ROW["folder"], files / "Vault", "Vault"),
    "siyuan": (ROW["siyuan"], files / "siyuan", "SiYuan Import Report"),
}

failed = []
for name in sys.argv[1:] or CASES:
    y, path, title = CASES[name]
    script = work / f"{name}.txt"
    script.write_text(f"move 5 5\nwait 800\nshowing Start\nanchor create-sidebar\nwait 400\npick {path}\nclick 340 {y}\nwait 2500\nshot {name}\nshowing {title}\n")
    out = work / name
    r = subprocess.run([sys.executable, str(here / "run.py"), str(script), str(source), str(out)],
                       env=dict(os.environ, FDL_NOBLINK="1"), timeout=120)
    log = (out / "log").read_text(errors="replace") if (out / "log").exists() else ""
    said = [l for l in log.splitlines() if "showing" in l]
    if r.returncode == 0: print(f"{name}: passed", flush=True)
    else:
        failed.append(name)
        print(f"{name}: FAILED {said[-1] if said else ''}", flush=True)
print(f"import_open: {'passed' if not failed else 'FAILED ' + ', '.join(failed)}; artifacts: {work}")
sys.exit(1 if failed else 0)
