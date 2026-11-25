"""What an import brings in is in front as soon as it finishes: a Markdown
file opens, an imported folder (Markdown, an Obsidian vault) shows its page,
a 思源笔记 workspace its 导入报告.

Build with --release --features devtools, then run this file. Each import
goes through a folder's "+" menu (思源笔记 and Obsidian through its
从其他应用导入… panel) and a `pick` instead of the system panel, into a
temporary library; `showing` checks what is in front. `found_vault` imports a
vault the panel found in a stand-in home folder (an Obsidian iCloud vault), so
the user's own is never looked at. No system clipboard or user library is
accessed. Pass case names to run only those.
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
# A folder to import into: 主页's "+" only opens folders, a folder's "+" brings things in.
(source / "Inbox").mkdir(parents=True)
(source / "Inbox" / "Start.md").write_text("# Start\n\nstart\n")
(source / ".quiet").mkdir()
(source / ".quiet" / "preferences.json").write_text(json.dumps({
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

# A stand-in home: what the panel finds on "this computer".
home = work / "home"
found = home / "Library/Mobile Documents/iCloud~md~obsidian/Documents/Thesis"
(found / ".obsidian").mkdir(parents=True)
(found / "Intro.md").write_text("# Intro\n\nFound.\n")

# Inbox's "+" menu (English): what to click, with `pick` before the row that opens the file panel.
# An Obsidian vault asks first, open it where it is or import a copy: these take the copy.
APPS, COPY = "Import from Another App…", "Import a copy"
CASES = {
    "markdown_file": (["pick", "Import Markdown Files…"], files / "Single.md", "Single"),
    "markdown_folder": (["pick", "Import Folder…"], files / "notes", "notes"),
    "obsidian_vault": (["pick", "Import Folder…", COPY], files / "Vault", "Vault"),
    "obsidian_app": ([APPS, "Obsidian", "pick", COPY], files / "Vault", "Vault"),
    "siyuan": ([APPS, "pick", "SiYuan"], files / "siyuan", "SiYuan Import Report"),
    "found_vault": ([APPS, "Obsidian · Thesis", COPY], None, "Thesis"),
}

failed = []
for name in sys.argv[1:] or CASES:
    rows, path, title = CASES[name]
    script = work / f"{name}.txt"
    # The panel looks for other apps' notes as it opens: a pause after each row.
    steps = [f"pick {path}" if r == "pick" else f"row {r}\nwait 1500" for r in rows]
    script.write_text("move 5 5\nwait 800\nrow Inbox\nwait 800\nshowing Inbox\nanchor create-folder\nwait 400\n" + "\n".join(steps) + f"\nwait 2500\nshot {name}\nshowing {title}\n")
    out = work / name
    r = subprocess.run([sys.executable, str(here / "run.py"), str(script), str(source), str(out)],
                       env=dict(os.environ, FDL_NOBLINK="1", HOME=str(home)), timeout=120)
    log = (out / "log").read_text(errors="replace") if (out / "log").exists() else ""
    said = [l for l in log.splitlines() if "showing" in l]
    if r.returncode == 0: print(f"{name}: passed", flush=True)
    else:
        failed.append(name)
        print(f"{name}: FAILED {said[-1] if said else ''}", flush=True)
print(f"import_open: {'passed' if not failed else 'FAILED ' + ', '.join(failed)}; artifacts: {work}")
sys.exit(1 if failed else 0)
