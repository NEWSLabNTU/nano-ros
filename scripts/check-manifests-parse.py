#!/usr/bin/env python3
"""issue 0722 — every tracked `Cargo.toml` must parse, whatever workspace reaches it.

# What this protects

A manifest is not read by "the build". It is read by whichever workspace claims
it, and this tree has many roots: the repo root, `packages/cli`, and each nested
example workspace. A crate that no root claims is compiled only by the leaf that
names it, so a manifest defect in it is invisible to every command run from the
top — including `cargo metadata`, which is the check most gates lean on.

# The defect it was written for

`packages/boards/nros-board-esp32-qemu/Cargo.toml` ended 2026-08-20 with the key
`nros-log` declared TWICE in one `[dependencies]` table. Issue 0708's fix and
issue 0710's fix each added their own copy, and the dedup commit between them
(`6834eb7dc`) removed a THIRD, leaving two. Cargo does not merge or warn — it
refuses the file:

    error: duplicate key
      --> packages/boards/nros-board-esp32-qemu/Cargo.toml:80:1

That failure propagates outward: `cargo metadata` for
`examples/workspaces/rust` died on it four frames up, naming
`esp32_entry_nros_selection` rather than the board, and took `just format` with
it. Meanwhile `cargo metadata` from the repo root was GREEN, because the root
workspace does not include this board. So the tree read as healthy from the
place anyone would look.

# The rule

Every `Cargo.toml` tracked by git parses as TOML. Nothing about content — this
is the syntactic floor that a duplicate key, an unterminated string, or a
mis-nested table all fall through.

# Detection

The manifests are parsed directly rather than shelled out to cargo: 353 files
against one `cargo metadata` per file is the difference between a fast-line gate
and a coffee break, and a TOML parser rejects a duplicate key for the same
reason cargo does. Python 3.10 has no `tomllib`, so this uses the repo's
established `tomli` fallback.

Run: python3 scripts/check-manifests-parse.py
"""

import subprocess
import sys

try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11 — the repo's interpreter is 3.10
    import tomli as tomllib


def main() -> int:
    listed = subprocess.run(
        ["git", "ls-files", "-z", "*Cargo.toml"],
        capture_output=True,
        check=True,
    ).stdout
    paths = [p for p in listed.decode().split("\0") if p]

    bad: list[tuple[str, str]] = []
    for path in paths:
        try:
            with open(path, "rb") as fh:
                tomllib.load(fh)
        except FileNotFoundError:
            # tracked but not checked out (a sparse or partial worktree)
            continue
        except Exception as exc:
            bad.append((path, str(exc)))

    if bad:
        print("tracked Cargo.toml files that do not parse:", file=sys.stderr)
        for path, why in bad:
            print(f"  {path}: {why}", file=sys.stderr)
        print(
            "\ncargo REFUSES such a manifest rather than warning, and the failure\n"
            "surfaces in whichever workspace claims the crate — which may not be the\n"
            "repo root. See issue 0722.",
            file=sys.stderr,
        )
        return 1

    print(f"check-manifests-parse: OK ({len(paths)} manifest(s))")
    return 0


if __name__ == "__main__":
    sys.exit(main())
