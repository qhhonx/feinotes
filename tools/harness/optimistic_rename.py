"""重命名 and 移动到 show at once and are written in the background (src/tree_writes.rs).

Build with --release --features devtools, then run this file. Every case
renames in the sidebar with the write held back (`FDL_TREE_WRITE_DELAY`), acts
while it is on its way, and then checks the rows and the files on disk:

  normal        the row has the new name before the folder does; links follow; ⌘Z puts it back
  open_inside   a page inside clicked right after Enter opens once the folder has moved
  rename_twice  renamed again before the first rename landed: the last name wins
  edit_during   the open page inside is typed into before and after Enter: saved in the moved folder
  field_open    another row's rename field is open when the first rename lands: it keeps its text
  title_typed   a title typed in the editor is saved at once; the file takes its name (and the
                links to it follow) off the window's thread
  fails         the folder cannot be renamed (its parent is read-only): the old name is back,
                the row is in its field with what was typed, a toast says why
  move_dialog   移动到 puts a folder into another: there at once, a page in it opens once it has
                landed, links into it follow, ⌘Z brings it back
  move_fails    移动到 a folder that cannot be written to: the row is back where it was
  top_taken     a top-level folder renamed to a taken name: back in its field with what was
                typed; typed on to a free name, it is renamed

Everything happens in a temporary library; no system clipboard or user
library is accessed. Pass case names to run only those. Screenshots of each
case are left in the artifacts folder.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

here = Path(__file__).resolve().parent
work = Path(tempfile.mkdtemp(prefix="quiet-optimistic-rename-"))
PREFS = json.dumps({"appearance": "light", "english": True, "language": "en", "seen_version": "9.9.9", "auto_update": False, "quick_hotkey": False})
DELAY = "1500"


def library(root):
    """A folder of pages that link to each other and to a picture, a page outside linking in, and a folder that cannot be renamed."""
    (root / ".quiet").mkdir(parents=True)
    (root / ".quiet" / "preferences.json").write_text(PREFS)
    (root / "Project" / "img").mkdir(parents=True)
    (root / "Project" / "img" / "p.png").write_bytes(bytes.fromhex("89504e470d0a1a0a0000000d4948445200000001000000010802000000907753de0000000c4944415408d763f8cfc000000301010018dd8db00000000049454e44ae426082"))
    (root / "Project" / "Alpha.xml").write_text('<title>Alpha</title>\n<p>first</p>\n<p>See <cite type="doc" href="Beta.xml" file-type="docx">Beta</cite></p>\n')
    (root / "Project" / "Beta.xml").write_text('<title>Beta</title>\n<p>beta</p>\n')
    (root / "Project" / "Notes.md").write_text("![p](img/p.png)\n\nSee [Outside](../Outside.xml)\n")
    (root / "Outside.xml").write_text('<title>Outside</title>\n<p>To <cite type="doc" href="Project/Alpha.xml" file-type="docx">Alpha</cite> and <a href="Project/Beta.xml">the file</a></p>\n')
    (root / "Locked" / "Inner").mkdir(parents=True)
    (root / "Locked" / "Inner" / "Leaf.xml").write_text("<title>Leaf</title>\n<p>leaf</p>\n")
    return root


RENAME = "rrow {0}\nwait 300\nrow Rename\nwait 300\nkey cmd-a\ntype {1}\nkey enter\n"


def run(name, script, lib, env=None):
    out = work / name
    (work / f"{name}.txt").write_text("resize 1300 850\nwait 300\nmove 700 500\nwait 1200\n" + script)
    r = subprocess.run([sys.executable, str(here / "run.py"), str(work / f"{name}.txt"), str(lib), str(out)],
                       env=dict(os.environ, FDL_NOBLINK="1", FDL_TREE_WRITE_DELAY=DELAY, **(env or {})), timeout=180)
    log = (out / "log").read_text(errors="replace") if (out / "log").exists() else ""
    return r.returncode, out / "lib", log


def normal():
    # The row answers to the new name right after Enter, 1.5 s before the folder moves; ⌘Z renames it back.
    code, lib, log = run("normal", RENAME.format("Project", "Renamed") + "row Renamed\nshot ahead\nwait 3000\nshot landed\nrow Renamed\nwait 300\nkey cmd-z\nwait 3500\nrow Project\nshot undone\n", library(work / "src-normal"))
    assert code == 0, log[-600:]
    assert (lib / "Project" / "Alpha.xml").is_file() and not (lib / "Renamed").exists(), "⌘Z did not put the folder back"
    assert 'href="Project/Alpha.xml"' in (lib / "Outside.xml").read_text(), "links did not follow the undo"
    # … and without the undo, the folder and the links to it are as renamed.
    code, lib, log = run("normal-kept", RENAME.format("Project", "Renamed") + "wait 3000\nrow Renamed\n", library(work / "src-normal-kept"))
    assert code == 0, log[-600:]
    assert (lib / "Renamed" / "Alpha.xml").is_file() and not (lib / "Project").exists(), "the folder was not renamed on disk"
    outside = (lib / "Outside.xml").read_text()
    assert 'href="Renamed/Alpha.xml"' in outside and 'href="Renamed/Beta.xml"' in outside, f"links did not follow: {outside}"
    # What is inside keeps its depth: its links to its own pages and pictures read as they did.
    assert 'href="Beta.xml"' in (lib / "Renamed" / "Alpha.xml").read_text()
    assert (lib / "Renamed" / "Notes.md").read_text() == "![p](img/p.png)\n\nSee [Outside](../Outside.xml)\n"


def open_inside():
    code, lib, log = run("open_inside", "row Project\nwait 400\n" + RENAME.format("Project", "Renamed") + "row Alpha\nwait 3000\nshowing Alpha\nshot opened\nclick 700 400\ntype  typed\nwait 2500\n", library(work / "src-open"))
    assert code == 0, log[-600:]
    assert "Could not open" not in log
    assert "typed" in (lib / "Renamed" / "Alpha.xml").read_text(), "the page opened after the rename was not saved in the renamed folder"
    assert not (lib / "Project").exists(), "something was written to the old path"


def rename_twice():
    code, lib, log = run("rename_twice", RENAME.format("Project", "First") + RENAME.format("First", "Second") + "shot ahead\nwait 5000\nrow Second\nshot landed\n", library(work / "src-twice"))
    assert code == 0, log[-600:]
    assert (lib / "Second" / "Alpha.xml").is_file(), sorted(p.name for p in lib.iterdir())
    assert not (lib / "Project").exists() and not (lib / "First").exists(), sorted(p.name for p in lib.iterdir())
    assert 'href="Second/Alpha.xml"' in (lib / "Outside.xml").read_text()


def edit_during():
    code, lib, log = run("edit_during", "row Project\nwait 400\nrow Alpha\nwait 600\nclick 700 330\nkey cmd-down\ntype  one\n" + RENAME.format("Project", "Renamed") + "wait 100\nclick 700 330\nkey cmd-down\ntype  two\nwait 5000\nshot saved\n", library(work / "src-edit"))
    assert code == 0, log[-600:]
    text = (lib / "Renamed" / "Alpha.xml").read_text()
    assert "one" in text and "two" in text, f"edits around the rename were lost: {text}"
    assert not (lib / "Project").exists(), "the save went to the old path"
    assert "Could not save" not in log


def field_open():
    script = RENAME.format("Project", "Renamed") + "rrow Outside\nwait 300\nrow Rename\nwait 300\nkey cmd-a\ntype Kept\nwait 3000\nshot landed\ntype More\nkey enter\nwait 3500\nrow KeptMore\n"
    code, lib, log = run("field_open", script, library(work / "src-field"))
    assert code == 0, log[-600:]
    assert (lib / "Renamed" / "Alpha.xml").is_file() and (lib / "KeptMore.xml").is_file() and not (lib / "Outside.xml").exists(), sorted(p.name for p in lib.iterdir())
    assert "../KeptMore.xml" in (lib / "Renamed" / "Notes.md").read_text()


def title_typed():
    script = "row Project\nwait 400\nrow Beta\nwait 600\nclick 420 125\nkey cmd-right\ntype  Two\nwait 1200\nshot typed\nwait 4000\nrow Beta Two\n"
    code, lib, log = run("title_typed", script, library(work / "src-title"))
    assert code == 0, log[-600:]
    assert (lib / "Project" / "Beta Two.xml").is_file() and not (lib / "Project" / "Beta.xml").exists(), sorted(p.name for p in (lib / "Project").iterdir())
    assert 'href="Project/Beta Two.xml"' in (lib / "Outside.xml").read_text().replace("%20", " "), (lib / "Outside.xml").read_text()


def run_locked(name, script, src, locked):
    """`run`, with folder `locked` of the copied library read-only while the app runs."""
    out = work / name
    os.makedirs(out, exist_ok=True)
    lib = out / "lib"
    shutil.rmtree(lib, ignore_errors=True)
    shutil.copytree(src, lib)
    (out / "s.txt").write_text("resize 1300 850\nwait 300\nmove 700 500\nwait 1200\n" + script + "\nquit\n")
    binary = os.environ.get("FDL_BIN") or str(here.parent.parent / "target" / "release" / "quiet_md")
    env = {"HOME": os.path.expanduser("~"), "QUIET_MD_HOME": str(lib), "FDL_SCRIPT": str(out / "s.txt"), "FDL_SHOTS": str(out), "PATH": "/usr/bin:/bin", "FDL_NOBLINK": "1", "FDL_TREE_WRITE_DELAY": DELAY}
    os.chmod(lib / locked, 0o555)
    try:
        code = subprocess.run(binary, env=env, cwd=out, stdout=open(out / "log", "w"), stderr=subprocess.STDOUT, timeout=180).returncode
    finally:
        os.chmod(lib / locked, 0o755)
    return code, lib, (out / "log").read_text(errors="replace")


def fails():
    script = "row Locked\nwait 400\n" + RENAME.format("Inner", "Typed") + "row Typed\nshot ahead\nwait 3000\nshot back\nkey right\ntype X\nwait 200\nshot typed_on\nkey escape\nwait 300\nrow Inner\n"
    code, lib, log = run_locked("fails", script, library(work / "src-fails"), "Locked")
    assert code == 0, log[-600:]
    assert (lib / "Locked" / "Inner" / "Leaf.xml").is_file() and not (lib / "Locked" / "Typed").exists()


def moving(root):
    """A folder to move, one to move it into, and a page outside mentioning a page of the first."""
    (root / ".quiet").mkdir(parents=True)
    (root / ".quiet" / "preferences.json").write_text(PREFS)
    for d in ("Project", "Target"): (root / d).mkdir()
    (root / "Project" / "Alpha.xml").write_text('<title>Alpha</title>\n<p>a</p>\n<p>See <cite type="doc" href="../Outside.xml" file-type="docx">Outside</cite></p>\n')
    (root / "Target" / "Keep.xml").write_text("<title>Keep</title>\n<p>k</p>\n")
    (root / "Outside.xml").write_text('<title>Outside</title>\n<p>To <cite type="doc" href="Project/Alpha.xml" file-type="docx">Alpha</cite></p>\n')
    return root


# 移动到 on Project's row, then Target in the dialog's tree (Project itself is not offered) and OK:
# points in the 1300×850 window every run starts with (the default size follows the display).
MOVE = "rrow Project\nwait 300\nrow Move to\nwait 500\nclick 597 218\nwait 300\nclick 1017 710\n"


def move_dialog():
    # A page inside clicked right after OK opens once the folder is there; typed into, it is saved there.
    code, lib, log = run("move_dialog", MOVE + "shot ahead\nrow Alpha\nwait 3000\nshowing Alpha\nclick 700 400\nkey cmd-down\ntype  typed\nwait 2500\nshot landed\n", moving(work / "src-move"))
    assert code == 0, log[-600:]
    assert "Could not open" not in log
    alpha = (lib / "Target" / "Project" / "Alpha.xml").read_text()
    assert "typed" in alpha and 'href="../../Outside.xml"' in alpha, alpha
    assert not (lib / "Project").exists(), "something was left at, or written to, the old path"
    assert 'href="Target/Project/Alpha.xml"' in (lib / "Outside.xml").read_text()
    # ⌘Z puts the folder back at the top, links too.
    code, lib, log = run("move_undone", MOVE + "wait 3000\nkey cmd-z\nwait 3500\nshot undone\n", moving(work / "src-move-undo"))
    assert code == 0, log[-600:]
    assert (lib / "Project" / "Alpha.xml").is_file() and not (lib / "Target" / "Project").exists(), "⌘Z did not bring the folder back"
    assert 'href="Project/Alpha.xml"' in (lib / "Outside.xml").read_text()


def move_fails():
    code, lib, log = run_locked("move_fails", MOVE + "shot ahead\nwait 3000\nshot back\n", moving(work / "src-move-fails"), "Target")
    assert code == 0, log[-600:]
    assert (lib / "Project" / "Alpha.xml").is_file() and not (lib / "Target" / "Project").exists()
    assert 'href="Project/Alpha.xml"' in (lib / "Outside.xml").read_text()


def top_taken():
    src = work / "src-top"
    for name in ("One", "Two"):
        (src / name / ".quiet").mkdir(parents=True)
        (src / name / ".quiet" / "preferences.json").write_text(PREFS)
        (src / name / f"In {name}.xml").write_text(f"<title>In {name}</title>\n<p>x</p>\n")
    lib = work / "top_taken" / "lib"
    folders = work / "folders.json"
    folders.write_text(json.dumps({"v": 1, "folders": [{"key": "k-one", "path": str(lib / "One"), "added": 1}, {"key": "k-two", "path": str(lib / "Two"), "added": 2}]}))
    script = RENAME.format("One", "Two") + "wait 600\nshot back\nkey right\ntype 2\nkey enter\nwait 1500\nrow Two2\nshot renamed\n"
    code, lib, log = run("top_taken", script, src, {"QUIET_MD_FOLDERS": str(folders)})
    assert code == 0, log[-600:]
    assert (lib / "Two2" / "In One.xml").is_file() and (lib / "Two" / "In Two.xml").is_file() and not (lib / "One").exists(), sorted(p.name for p in lib.iterdir())


CASES = {"normal": normal, "open_inside": open_inside, "rename_twice": rename_twice, "edit_during": edit_during, "field_open": field_open, "title_typed": title_typed, "fails": fails, "move_dialog": move_dialog, "move_fails": move_fails, "top_taken": top_taken}
failed = []
for name in sys.argv[1:] or CASES:
    try:
        CASES[name]()
        print(f"{name}: passed", flush=True)
    except AssertionError as e:
        failed.append(name)
        print(f"{name}: FAILED {e}", flush=True)
print(f"optimistic_rename: {'passed' if not failed else 'FAILED ' + ', '.join(failed)}; artifacts: {work}")
sys.exit(1 if failed else 0)
