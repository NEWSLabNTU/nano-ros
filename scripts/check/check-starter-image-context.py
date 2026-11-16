#!/usr/bin/env python3
"""check-starter-image-context — the starter image's context can be written
from this tree (issue 2635).

The `starter` / `starter-zephyr` context is written from the book: the Zephyr
starter page's `probe=` steps become the stage's scripts
(`_nros_starter_zephyr_steps`, `scripts/ci/lib/image-context.sh`). A book edit
can therefore break the image, and nothing wrote the context before a
release: #3989 split step 34 into three blocks, the writer's one-block reader
refused it, and the 0.6.0-nros11 images failed AFTER the release published.

On every run, first the step reader's negative controls
(`extract-book-steps.py`'s self-test, which covers `order_body`'s refusals:
no block, a tabbed block, a run before the last block), then the real thing:
`scripts/ci/image-context.sh starter <tmp>` for the current tree, no engine
and no network, and each Zephyr step script present and non-empty, and the
Dockerfile naming both targets. ~1 s.
"""
import importlib.util
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
STEPS = ("zephyr-host.sh", "zephyr-provision.sh", "zephyr-ws.sh", "zephyr-board.sh",
         "install.sh", "Dockerfile")
TARGETS = ("starter", "starter-zephyr")


def reader_self_test():
    spec = importlib.util.spec_from_file_location(
        "extract_book_steps", ROOT / "scripts/probe/extract-book-steps.py")
    ebs = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(ebs)
    ebs.self_test()


def problems_in(ctx):
    out = []
    for f in STEPS:
        p = ctx / f
        if not p.is_file() or p.stat().st_size == 0:
            out.append(f"{f} is missing or empty in the written context")
    df = ctx / "Dockerfile"
    text = df.read_text() if df.is_file() else ""
    for t in TARGETS:
        if not re.search(rf"^FROM \S+ AS {re.escape(t)}$", text, re.M):
            out.append(f"the Dockerfile has no `FROM … AS {t}` stage")
    return out


def main():
    reader_self_test()
    # The context checks' own negative control: an empty context is refused
    # for every file and both targets.
    with tempfile.TemporaryDirectory() as d:
        if len(problems_in(Path(d))) != len(STEPS) + len(TARGETS):
            print("check-starter-image-context: self-test FAIL, an empty context was not refused",
                  file=sys.stderr)
            return 1
    with tempfile.TemporaryDirectory() as d:
        ctx = Path(d) / "ctx"
        p = subprocess.run(["bash", str(ROOT / "scripts/ci/image-context.sh"), "starter", str(ctx)],
                           cwd=ROOT, capture_output=True, text=True)
        if p.returncode != 0:
            sys.stderr.write(p.stdout + p.stderr)
            print("check-starter-image-context: the starter image's context cannot be written "
                  "from this tree (issue 2635)", file=sys.stderr)
            return 1
        probs = problems_in(ctx)
    for pr in probs:
        print(f"check-starter-image-context: {pr}", file=sys.stderr)
    if probs:
        return 1
    print("check-starter-image-context: OK (context written; Zephyr steps and both targets present)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
