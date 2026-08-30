#!/usr/bin/env python3
"""phase-400 W8 — a migrated knob has exactly ONE reader.

Retirement is a wave, not a side effect. A mechanism that still resolves is a
mechanism people still use, and a fallback left in place winning silently is how
issues 0135 and 0316 happened: two consumers disagreeing about one value with no
diagnostic. Both were "a struct's size differed between TUs"; neither failed
loudly.

So once a knob is migrated into the RFC-0049 ladder, the ladder must be the only
thing that resolves it. Concretely: a knob listed in KNOB_ENV_NAMES may be

  * READ once, by the resolver that owns it, and
  * mentioned freely in comments, docs and tests,

but must not be read a second time by a build script that would then disagree
with the resolver about the value.

The check is deliberately narrow. It looks for the env-reading IDIOMS this tree
uses -- `env_usize("X"`, `env::var("X")`, `env::var_os("X")` -- and not for the
bare string, because the whole point is that the NAME stays valid as a front-end
spelling. Finding the name in a comment is correct and expected.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]

# Knobs migrated into the ladder, with the crate that legitimately reads each.
# Adding a row here is the second half of migrating a knob; a knob that is in
# the ladder but not here is simply unchecked, which is why W6 and W8 move
# together.
MIGRATED: dict[str, str] = {
    "NROS_EXECUTOR_MAX_CBS": "packages/core/nros-node/build.rs",
    "NROS_EXECUTOR_MAX_SC": "packages/core/nros-node/build.rs",
    "NROS_EXECUTOR_MAX_NODES": "packages/core/nros-node/build.rs",
    "NROS_EXECUTOR_MAX_SHUTDOWN_CBS": "packages/core/nros-node/build.rs",
    "NROS_EXECUTOR_ACTION_CLIENTS": "packages/core/nros-node/build.rs",
    "NROS_EXECUTOR_ARENA_SIZE": "packages/core/nros-node/build.rs",
    "NROS_SUBSCRIPTION_BUFFER_SIZE": "packages/core/nros-node/build.rs",
    "NROS_PARAM_SERVICE_BUFFER_SIZE": "packages/core/nros-node/build.rs",
}

# The resolver itself names every knob in its front-end table; that is the map,
# not a second reader.
EXEMPT = {
    "packages/boards/nros-board-common/src/platform_config.rs",
}

READ_IDIOMS = [
    r'env_usize\(\s*"{k}"',
    r'env_bool\(\s*"{k}"',
    r'env::var\(\s*"{k}"',
    r'env::var_os\(\s*"{k}"',
    r'std::env::var\(\s*"{k}"',
    r'std::env::var_os\(\s*"{k}"',
]


def strip_comments(src: str) -> str:
    """Drop `//` and `/* */` comments.

    The docstring promises a knob may be "mentioned freely in comments", and the
    gate has to actually honour that: prose explaining WHY a read was removed
    naturally quotes the idiom verbatim, and matching it would make writing the
    explanation trip the check. Not a full Rust lexer — a `//` inside a string
    literal over-strips — but this only ever causes a MISSED reader, never a
    false one, and the failure mode of a config gate should be quiet rather than
    crying wolf.
    """
    src = re.sub(r"/\*.*?\*/", "", src, flags=re.S)
    return re.sub(r"//[^\n]*", "", src)


def main() -> int:
    # Single pass over the sources: read each file once and test every knob
    # against it. The naive shape (a pass per knob) re-reads several thousand
    # files eight times and takes minutes, which is a gate nobody will run.
    pats = {
        knob: [re.compile(i.format(k=re.escape(knob))) for i in READ_IDIOMS]
        for knob in MIGRATED
    }
    readers: dict[str, set[str]] = {k: set() for k in MIGRATED}

    # `git ls-files`, not a filesystem walk: an index lookup skips the vendored
    # trees and build outputs for free, and `check-no-tracked-file-find` forbids
    # the walk outright -- it measured 7m36s versus 0.8s for the same paths, and
    # notes that pruning does not help because find still stats every directory
    # it considers pruning. It caught this script's first draft.
    listing = subprocess.run(
        ["git", "ls-files", "-z", "--", "*.rs"],
        cwd=REPO,
        capture_output=True,
        check=True,
    )
    for rel in listing.stdout.decode("utf-8", "ignore").split("\0"):
        if not rel or rel in EXEMPT:
            continue
        path = REPO / rel
        try:
            text = path.read_text(encoding="utf-8", errors="ignore")
        except OSError:
            continue
        if "NROS_" not in text:
            continue
        text = strip_comments(text)
        for knob, ps in pats.items():
            if knob in text and any(p.search(text) for p in ps):
                readers[knob].add(rel)

    failures = []
    for knob, owner in sorted(MIGRATED.items()):
        extra = sorted(readers[knob] - {owner})
        if extra:
            failures.append(
                f"  {knob}: migrated, owner is {owner}, but also read by:\n"
                + "".join(f"      {r}\n" for r in extra)
            )

    if failures:
        print("check-knob-single-reader: a migrated knob has more than one reader\n")
        print("".join(failures))
        print(
            "A second reader is not a fallback, it is a disagreement waiting to\n"
            "happen: the two can resolve different values and nothing reports it\n"
            "(issues 0135, 0316). Delete the second reader, or -- if it is the\n"
            "legitimate owner -- update MIGRATED in this script."
        )
        return 1

    print(
        f"check-knob-single-reader: OK - {len(MIGRATED)} migrated knob(s), "
        "one reader each"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
