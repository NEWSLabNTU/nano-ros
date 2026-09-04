#!/usr/bin/env python3
"""Every TRACKED source path must survive the ROS-box mirror.

`scripts/dev/ros2-box-sync.sh` excludes build-output directories by NAME
pattern, and the tree contains tracked SOURCE whose names match those patterns.
That collision has now eaten tracked files four times, each time discovered as a
build failure inside the box that named something else entirely:

    `build`        matched `scripts/build/`            -> lost cargo.sh
    `build-*`      matched `scripts/build/build-root.sh`
    (fix: anchor, then make every pattern directory-only)
    `build-*/`     matched `packages/cli/build-support/`
                   -> "couldn't read nros-cli-core/../build-support/
                       submodule_watch.rs", i.e. the box could not build `nros`

Anchoring cured one instance and the trailing `/` cured another; neither cured
the CLASS, because a tracked DIRECTORY whose name begins with `build-` still
matches a directory-only pattern.

This asks the question the fixes kept answering locally: does any tracked path
match an exclusion without a matching re-include? It is a source-only check —
no rsync, no box, no network.
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SYNC = ROOT / "scripts/dev/ros2-box-sync.sh"


def rules():
    """(kind, pattern) in file order — rsync takes the FIRST match."""
    out = []
    for line in SYNC.read_text().splitlines():
        m = re.match(r"\s*--(include|exclude)\s+'([^']+)'", line)
        if m:
            out.append((m.group(1), m.group(2)))
    return out


def matches(pattern, path):
    """Approximate rsync matching for the shapes this file uses.

    DIRECTORY-ONLY is the load-bearing part. A pattern ending in `/` matches
    directories and never files -- that is precisely the fix the sync script's
    header records for its second incident, and a checker that ignores it
    reports `build-all.mk` and 30 book pages as lost. They are not: `build-*/`
    cannot match a file. Getting this wrong makes the gate cry wolf about the
    very files the last fix rescued.
    """
    dir_only = pattern.rstrip("*").endswith("/")
    pat = pattern.rstrip("*").rstrip("/")
    anchored = pat.startswith("/")
    pat = pat.lstrip("/")
    rx = re.escape(pat).replace(r"\*\*", ".*").replace(r"\*", "[^/]*")
    # A directory-only rule matches a PATH only when the matched run is
    # followed by more path -- i.e. it is a parent directory of this file.
    tail = r"/" if dir_only else r"(/|$)"
    if anchored:
        return re.match(rx + tail, path) is not None
    return re.search(r"(^|/)" + rx + tail, path) is not None


def main() -> int:
    tracked = subprocess.run(
        ["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, check=True
    ).stdout.split()
    rs = rules()

    # `/tmp/` is the repo's scratch directory and is gitignored (.gitignore:50).
    # Ten files are tracked there anyway -- `collapse-*-case.sh` and two
    # `migrate-*.py`, historical one-off repro and migration scripts committed
    # before the convention settled. They are not build inputs, nothing in the
    # box reads them, and re-including a directory the sync script deliberately
    # drops would be the wrong fix.
    #
    # Listed by PREFIX rather than by name so a new scratch file does not fail
    # this gate -- and called out here rather than silently skipped, because
    # "tracked under a gitignored path" is itself worth someone's attention:
    # a `git add` of one of these needs `-f`, so each was deliberate at the time.
    ALLOWED_ABSENT = ("tmp/",)

    lost = []
    for path in tracked:
        for kind, pattern in rs:
            if matches(pattern, path):
                if kind == "exclude" and not path.startswith(ALLOWED_ABSENT):
                    lost.append((path, pattern))
                break  # first rule wins, include or exclude
    if lost:
        print(
            f"check-box-sync-covers-tracked-source: {len(lost)} TRACKED path(s) "
            f"would not reach the box mirror",
            file=sys.stderr,
        )
        for path, pattern in lost[:20]:
            print(f"  {path}\n      excluded by  --exclude '{pattern}'", file=sys.stderr)
        if len(lost) > 20:
            print(f"  ... and {len(lost) - 20} more", file=sys.stderr)
        print("", file=sys.stderr)
        print("  A build-output pattern has eaten tracked SOURCE — the class in", file=sys.stderr)
        print("  ros2-box-sync.sh's header, which has now happened four times.", file=sys.stderr)
        print("  Re-include the path AHEAD of the exclusion (rsync takes the", file=sys.stderr)
        print("  first matching rule), as `/packages/cli/build-support/***` is.", file=sys.stderr)
        return 1
    skipped = sum(1 for p in tracked if p.startswith(ALLOWED_ABSENT))
    print(
        f"check-box-sync-covers-tracked-source: OK ({len(tracked)} tracked path(s); "
        f"{skipped} scratch path(s) under tmp/ deliberately not mirrored)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
