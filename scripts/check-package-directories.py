#!/usr/bin/env python3
"""The `packages/{...}` list in CLAUDE.md and ARCHITECTURE section 1 matches `ls packages/`.

Issue 1211. Both files stated the workspace layout, identically and wrongly, and
had done through two directory moves:

    packages/{core,zpico,xrce,dds,boards,drivers,interfaces,testing,
              verification,reference,codegen,cli}/

`zpico`, `xrce` and `dds` collapsed into `packages/rmw/{zenoh,xrce,cyclonedds}/`
and `packages/codegen` was retired into `packages/cli/` -- CLAUDE.md says so
itself thirty lines later, so the file contradicted itself on the same page.
Meanwhile `api` (the three user-facing language surfaces), `platform` (every
platform port), `rmw` and `tooling` appeared in neither, which is the half that
costs a reader something: section 1 describes the platform layer in prose one
line below a path list that points away from `packages/platform/`.

WHY A GATE, for a list of twelve words.

Because the workaround was already there. CLAUDE.md's own line read "Run
`ls packages/` for the current crate list" -- the list was KNOWN to be
untrustworthy and was patched with an instruction to ignore it rather than
corrected. A correction with nothing holding it lasts until the next directory
moves, and this one had already survived two. Three lines of comparison is the
cheapest ratchet available for a fact that is mechanically checkable.

It checks BOTH directions. A missing directory hides a whole layer from the
reader; an extra one sends them looking for something that is not there, which
is how `packages/codegen` outlived its own retirement note.

Run: python3 scripts/check-package-directories.py [--self-test]
"""

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]

# Each site states the set once, as a brace list. The regex spans newlines
# because both files wrap it -- ARCHITECTURE breaks inside the braces.
SITES = [
    "CLAUDE.md",
    "docs/design/ARCHITECTURE.md",
]

BRACE_LIST = re.compile(r"`packages/\{([^}]*)\}/`", re.S)


def actual_dirs(repo):
    return sorted(p.name for p in (Path(repo) / "packages").iterdir() if p.is_dir())


def offenders(repo=None):
    base = Path(repo) if repo else REPO
    want = actual_dirs(base)
    out = []
    for rel in SITES:
        path = base / rel
        if not path.is_file():
            out.append((rel, "file not found"))
            continue
        text = path.read_text(errors="replace")
        matches = BRACE_LIST.findall(text)
        if not matches:
            out.append((rel, "no `packages/{...}/` list found -- did the sentence move?"))
            continue
        if len(matches) > 1:
            out.append((rel, f"{len(matches)} `packages/{{...}}/` lists; there must be exactly one"))
            continue
        # Whitespace and line breaks are formatting, not content.
        got = sorted(x.strip() for x in re.sub(r"\s+", "", matches[0]).split(",") if x.strip())
        if got == want:
            continue
        missing = [d for d in want if d not in got]
        extra = [d for d in got if d not in want]
        why = []
        if missing:
            why.append(f"omits {', '.join(missing)}")
        if extra:
            why.append(f"names {', '.join(extra)} (no such directory)")
        out.append((rel, "; ".join(why)))
    return out


def self_test():
    """A tree whose docs are right, and one of each way they can be wrong."""
    import tempfile

    cases = [
        ("{alpha,beta}", 0, None, "an exact match passes"),
        ("{alpha}", 1, "omits beta", "a missing directory is caught"),
        ("{alpha,beta,gone}", 1, "no such directory", "a dead directory is caught"),
        ("{beta,alpha}", 0, None, "order is not content"),
        ("{alpha,\n  beta}", 0, None, "a wrapped list is one list"),
        ("nothing here", 1, "no `packages/", "a moved sentence is caught, not ignored"),
    ]
    failures = 0
    with tempfile.TemporaryDirectory() as td:
        root = Path(td)
        for name in ("alpha", "beta"):
            (root / "packages" / name).mkdir(parents=True)
        (root / "docs" / "design").mkdir(parents=True)
        for i, (body, want, want_why, name) in enumerate(cases):
            text = f"Workspace: `packages/{body}/`, examples/.\n" if body.startswith("{") else body
            for rel in SITES:
                (root / rel).write_text(text)
            found = offenders(repo=root)
            # Both sites carry the same text, so every case is 0 or 2 offenders.
            if len(found) != want * len(SITES):
                print(
                    f"  self-test FAIL: {name} -- got {len(found)}, want {want * len(SITES)}",
                    file=sys.stderr,
                )
                failures += 1
            elif want_why is not None and want_why not in found[0][1]:
                print(
                    f"  self-test FAIL: {name} -- reason {found[0][1]!r}"
                    f" does not mention {want_why!r}",
                    file=sys.stderr,
                )
                failures += 1
            _ = i
    if failures:
        print(f"check-package-directories self-test: FAILED ({failures})", file=sys.stderr)
        return 1
    print(f"check-package-directories self-test: OK ({len(cases)} cases)")
    return 0


def main(argv):
    if len(argv) == 2 and argv[1] == "--self-test":
        return self_test()
    if self_test():
        return 1
    bad = offenders()
    if not bad:
        print(
            f"check-package-directories: OK -- {len(SITES)} site(s) match"
            f" the {len(actual_dirs(REPO))} directories under packages/."
        )
        return 0
    print("check-package-directories: a documented workspace layout disagrees with disk:", file=sys.stderr)
    for rel, why in bad:
        print(f"  {rel}: {why}", file=sys.stderr)
    print("", file=sys.stderr)
    print(f"  On disk: packages/{{{','.join(actual_dirs(REPO))}}}/", file=sys.stderr)
    print("  A reader who cannot trust this line has no entry point into the tree;", file=sys.stderr)
    print("  RFC-0001's \"Directory map\" says what each directory holds.", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv))
