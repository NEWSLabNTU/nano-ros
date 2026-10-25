#!/usr/bin/env python3
"""Every book probe track still EXTRACTS — phase-517 W10.

`just probe <platform>` replays a chapter's `probe=NN` blocks in a container,
tens of minutes per chapter, so it runs weekly (`probe-platforms.yml`) and not
per push. What CAN be checked per push, for free, is that each track's script
still assembles: the chapter's tags survive an edit, every `probe-run` step
keeps the `probe-expect` output block it is asserted against (and every such
block keeps its step), a `variant=` still selects something, the setup
placeholder the harness substitutes still matches once, and the result parses
(`bash -n`). Each of those breaking is otherwise discovered a week later, after
an install, as a track that fails for a reason that is not about the chapter.

The track list is READ from run-bootstrap-probe.sh's `PROBE_PLATFORM` case
table, so a platform added there is checked here without editing this file.

Self-test (every call): a chapter whose run step lost its expected-output block
must fail to extract — the property this gate leans on.

Run:  python3 scripts/check-book-probe-tracks.py
"""

import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
HARNESS = REPO / "scripts/probe/run-bootstrap-probe.sh"
EXTRACTOR = REPO / "scripts/probe/extract-book-steps.py"
CASE_ROW = re.compile(r"^\s+([a-z0-9-]+)\)\s+PLATFORM_CHAPTER=")


def platforms() -> list[str]:
    rows = [m.group(1) for line in HARNESS.read_text().splitlines()
            if (m := CASE_ROW.match(line))]
    if "linux" not in rows or len(rows) < 2:
        sys.exit(f"check-book-probe-tracks: read {rows} from {HARNESS.name}'s "
                 "PROBE_PLATFORM table — the table changed shape; update CASE_ROW")
    return rows


def extract(platform: str, out: Path) -> subprocess.CompletedProcess:
    env = dict(os.environ, PROBE_TRACK="installed", PROBE_PLATFORM=platform,
               PROBE_EXTRACT_ONLY=str(out))
    return subprocess.run(["bash", str(HARNESS)], env=env, cwd=REPO,
                          capture_output=True, text=True)


def selftest() -> None:
    with tempfile.TemporaryDirectory() as d:
        ch = Path(d) / "chapter.md"
        ch.write_text("```bash probe=30 probe-run=10\nqemu-system-arm -kernel x\n```\n")
        r = subprocess.run([sys.executable, str(EXTRACTOR), "--out", str(Path(d) / "o.sh"),
                            str(ch)], capture_output=True, text=True)
        if r.returncode == 0 or "probe-expect" not in r.stderr:
            sys.exit("check-book-probe-tracks: SELFTEST FAILED — a run step with no "
                     f"expected-output block extracted (rc={r.returncode}): {r.stderr}")


def main() -> int:
    selftest()
    failed = []
    names = platforms()
    with tempfile.TemporaryDirectory() as d:
        for p in names:
            r = extract(p, Path(d) / f"{p}.sh")
            if r.returncode != 0:
                failed.append((p, (r.stderr or r.stdout).strip()))
    if failed:
        print(f"check-book-probe-tracks: {len(failed)} track(s) no longer extract:",
              file=sys.stderr)
        for p, why in failed:
            print(f"  just probe {p}:\n    " + why.replace("\n", "\n    "), file=sys.stderr)
        return 1
    print(f"check-book-probe-tracks: OK ({len(names)} installed-track chapter(s) extract)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
