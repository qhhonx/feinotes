"""Shared helpers for the harness scripts.

The fixtures are written in the old layout (`docs/<id>.xml` + `index.json`);
the app moves them to FORMAT 4 when it opens the library, where a document is
a file named after its title in a real folder. `doc_file` finds it there.
"""
from pathlib import Path


def docs(lib, ext="xml"):
    """Documents of library `lib` (FORMAT 4), skipping `.quiet`, `.assets` and other hidden entries."""
    lib = Path(lib)
    return sorted(p for p in lib.rglob(f"*.{ext}") if not any(part.startswith(".") for part in p.relative_to(lib).parts))


def doc_file(lib):
    """The one document in `lib`."""
    found = docs(lib)
    assert len(found) == 1, f"expected one document in {lib}, found {found}"
    return found[0]


def as_migrated(tree):
    """`tree` (a fixture in the old layout) with the asset paths the move to FORMAT 4 gives it (`assets/` → `.assets/`)."""
    for el in tree.iter():
        for key in ("src", "href", "path"):
            if el.get(key, "").startswith("assets/"): el.set(key, "." + el.get(key))
    return tree
