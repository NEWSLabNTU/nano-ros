"""Tracked-file discovery through the git index instead of a filesystem walk.

Issue 0721. The gates that scan this repo look for files git already tracks, but
several found them with `Path.rglob` / `os.walk` rooted at `packages/` or
`examples/`. Those are the two trees that hold build output: measured 2026-08-20,
`examples/` was 828 GB and `rglob("Cargo.toml")` over it had not finished after
300 s, against 0.002 s for the same 347 paths from the index.

The trap that made it survive review is that naming the unwanted directories
looks like avoiding them:

    for m in root.rglob("Cargo.toml"):
        if any(p in {"target", "build", "generated"} for p in m.parts):
            continue

That filter runs on what the walk has ALREADY YIELDED. The descent into every
`target/` and `build-*/` tree happens first, to produce the paths it discards.
Pruning cannot be done after the fact — the same lesson
`scripts/check-no-tracked-file-find.sh` records for `find -prune` and
`check-image-panic-policy.py` for `glob("**")`.

One helper rather than a conversion per gate, because this repo's recurring
failure is a second spelling of a fix rather than a shared one.

A walk is still correct for files git CANNOT see — build output, staging copies,
a tree being deleted, an untracked submodule. Mark those `# walk-ok: <reason>`;
the gate accepts it and the reason is the point.
"""

import subprocess
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]


def tracked(*roots, suffix=None, name=None, repo=None):
    """Tracked files under `roots`, as absolute Paths.

    `suffix` (".rs") and `name` ("Cargo.toml") filter the result; both are
    applied in Python rather than as a pathspec so a caller's existing filter
    logic ports over unchanged.

    Roots outside the repo fall back to a walk — they have no index entry, so
    there is nothing to look up. That path is for temp trees a `--self-test`
    builds, which are small by construction; an in-repo root never reaches it,
    so the cost above cannot come back through here.
    """
    base = Path(repo) if repo else REPO
    rels, found = [], []
    for root in roots:
        root = Path(root)
        if not root.is_absolute():
            root = base / root
        try:
            rels.append(str(root.relative_to(base)))
        except ValueError:
            if root.is_dir():
                # walk-ok: an out-of-repo root has no index entry to consult;
                # this serves --self-test temp trees, which are tiny.
                found.extend(q for q in root.rglob("*") if q.is_file())
    if rels:
        out = subprocess.run(
            ["git", "-C", str(base), "ls-files", "-z", "--", *rels],
            capture_output=True, text=True, check=True,
        ).stdout.split("\0")
        found.extend(base / r for r in out if r)
    if suffix is not None:
        found = [p for p in found if p.suffix == suffix]
    if name is not None:
        found = [p for p in found if p.name == name]
    return sorted(found)
