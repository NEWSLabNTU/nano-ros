#!/usr/bin/env python3
"""No build glue may select a backend through an API crate's feature.

Issue 1461, and the class issues 1100 / 1156 / 1354 are all instances of —
re-scoped by RFC-0071 D9 (phase-506 W4d, issue 1297).

WHY THIS WAS A RULE

Every road that builds a mixed C/C++ image runs cargo twice into one
`--target-dir`, and both `nros-c` units write the same build-script byproduct
(`nros-c-generated/rclc/detail/nros_config_generated.h`). Cargo keys a unit by
its feature SET, so two spellings of one selection split `nros-c` in two. Issue
1461 measured the quiet form: a back-compat ALIAS (`cffi-zenoh-cffi` for
`rmw-zenoh`) gave two headers differing in one line, the shared file's mtime
moved on every build, and three pubsub coordinates read STALE for 18 days.

Every alias it found was a BACKEND selection: `nros-c` named its backends
(`rmw-zenoh`, `rmw-xrce`, `rmw-cyclonedds`), aliased them three times
(`cffi-zenoh-cffi`, `cffi-xrce-c`, `cffi-dds-cffi`), and `nros-cpp` forwarded a
fourth spelling (`rmw-<name>-cffi`). The two rules this gate used to check —
"the descriptor's `c_cffi_feature` agrees with what `nros-cpp` forwards" and
"no glue names an alias" — were about keeping those spellings in step.

WHAT IS CHECKED NOW

W4d deleted every one of those features: an image bundles its backend by
DEPENDING on it, through the image crate `cmake/NanoRosImageCrate.cmake`
generates. So there is no spelling left to keep in step, and the rule becomes
the one thing that can still go wrong — glue that still ASKS for one. Cargo
refuses an unknown feature loudly, but only on a road that RUNS, and several of
these roads (PX4, s32ds, a cross-only lane) run nowhere a merge waits on.

A retired selection is, derived from the descriptors (never listed here except
for the three historic aliases, which cannot grow):

  * `<cargo_feature>-cffi`              (`nros-cpp`'s spelling)
  * `nros-c/<cargo_feature>`, `nros-cpp/<cargo_feature>`  (a forward)
  * `cffi-zenoh-cffi`, `cffi-xrce-c`, `cffi-dds-cffi`   (the aliases)
  * an API manifest declaring `<cargo_feature>` or `<cargo_feature>-cffi`

A bare `rmw-<name>` in glue is refused only on a line that names an API crate
as the PACKAGE (`-p nros-c`, `--package nros-cpp`): elsewhere it is also every
BOARD crate's selection (RFC-0031 C5b), the live spelling on the Rust road.

Run: check-nros-c-feature-agreement.py [--self-test]
"""

import glob
import os
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent / "lib"))
import comments  # noqa: E402  phase-472 W3 — the one comment stripper
from check_just_sources import just_sources  # noqa: E402  phase-472 W2

try:
    import tomllib  # 3.11+
except ModuleNotFoundError:  # 3.10 backport, same spelling as the sibling gates
    import tomli as tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

API_MANIFESTS = ("packages/api/nros-c/Cargo.toml", "packages/api/nros-cpp/Cargo.toml")

# The three back-compat aliases `nros-c` carried until W4d. A historic list,
# not a vocabulary: nothing can add a fourth, because the features are gone.
RETIRED_ALIASES = ("cffi-zenoh-cffi", "cffi-xrce-c", "cffi-dds-cffi")

# This gate names every retired spelling in its docstring and self-test.
SCAN_EXEMPT = {"scripts/check-nros-c-feature-agreement.py"}

# Roots plus a suffix filter, NOT pathspec globs: `zephyr/**/*.txt` matches
# NOTHING for `zephyr/CMakeLists.txt`, and a glob that silently matches nothing
# narrows a gate toward OK (issue 0196's shape).
GLUE_ROOTS = ["cmake", "zephyr", "integrations", "packages", "just", "scripts", "examples"]
GLUE_SUFFIXES = (
    ".cmake",
    ".just",
    ".sh",
    ".py",
    ".toml",
    "CMakeLists.txt",
    "Makefile",
    "Make.defs",
)

# Roads that build an API crate. Their absence from the scanned set makes this
# gate quieter without making it look wrong, so it is a failure.
REACH_MUST_HOLD = [
    "justfile",  # phase-472 W2 — the root justfile has no suffix and no root dir
    "just/px4.just",
    "zephyr/CMakeLists.txt",
    "integrations/nuttx/Makefile",
    "integrations/px4/NanoRosPx4Module.cmake",
    "cmake/NanoRosFeatureSet.cmake",
]


def backend_features():
    """Each in-tree backend's `cargo_feature` (`rmw-<name>`), from its descriptor dir."""
    out = []
    for path in sorted(glob.glob(os.path.join(ROOT, "packages/rmw/*/*/nros-rmw.toml"))):
        name = os.path.relpath(path, ROOT).split(os.sep)[2]
        out.append(f"rmw-{name}")
    return sorted(set(out))


def retired_patterns(features):
    """`{spelling: regex}` for every retired API-crate backend selection."""
    spellings = list(RETIRED_ALIASES)
    for f in features:
        spellings += [f"{f}-cffi", f"nros-c/{f}", f"nros-cpp/{f}"]
    # Not inside another word: `nros-rmw-xrce-cffi` is a crate, not a feature.
    return {s: re.compile(rf"(?<![\w-]){re.escape(s)}(?![\w-])") for s in spellings}


API_PACKAGE_RE = re.compile(r"(?:-p|--package)[ =]+nros-c(?:pp)?(?![\w-])")


def check_manifests(features, read=None):
    """The API crates declare no backend feature."""
    read = read or (lambda p: open(os.path.join(ROOT, p), "rb").read())
    fails = []
    banned = set(features) | {f"{f}-cffi" for f in features} | set(RETIRED_ALIASES)
    for rel in API_MANIFESTS:
        declared = tomllib.loads(read(rel).decode()).get("features", {})
        for name in sorted(banned & set(declared)):
            fails.append(
                f"{rel}: declares the backend feature {name!r} — the API crates name no "
                f"backend (RFC-0071 D9); the image crate bundles it"
            )
    return fails


def strip_comments(text, rel):
    lang = comments.lang_for(rel)
    if lang is not None:
        return comments.strip_comments(text, lang)
    return "\n".join("" if line.lstrip().startswith("#") else line for line in text.splitlines())


def tracked_glue_files():
    try:
        listed = subprocess.run(
            ["git", "-C", ROOT, "ls-files", "-z", "--", *GLUE_ROOTS],
            capture_output=True,
            text=True,
            check=True,
        ).stdout
    except (OSError, subprocess.CalledProcessError):
        return None
    glue = [p for p in listed.split("\0") if p and p.endswith(GLUE_SUFFIXES)]
    just = [os.path.relpath(p, ROOT) for p in just_sources(ROOT)]
    return sorted(set(glue) | set(just))


def check_glue(features, files=None, read=None, reach_check=True):
    """No glue asks an API crate for a retired backend selection."""
    if not features:
        return ["no nros-rmw.toml found — refusing to pass on an empty set"]
    if files is None:
        files = tracked_glue_files()
        if files is None:
            return ["`git ls-files` failed — refusing to report OK over an unknown file set"]
    if not files:
        return ["no build-glue files matched — refusing to report OK over an empty set"]
    missing = [p for p in REACH_MUST_HOLD if p not in files] if reach_check else []
    if missing:
        return [
            "the scanned file set does not reach " + ", ".join(missing)
            + " — each is a road that builds an API crate, so a set without it is a "
            "gate that got quieter, not a tree that got cleaner"
        ]
    read = read or (lambda p: open(os.path.join(ROOT, p), encoding="utf-8").read())
    patterns = retired_patterns(features)
    bare = {f: re.compile(rf"(?<![\w/-]){re.escape(f)}(?![\w-])") for f in features}
    fails = []
    for rel in files:
        if rel in SCAN_EXEMPT:
            continue
        try:
            body = strip_comments(read(rel), rel)
        except (OSError, UnicodeDecodeError):
            continue
        for n, line in enumerate(body.splitlines(), 1):
            live = dict(patterns)
            if API_PACKAGE_RE.search(line):
                live.update(bare)
            for spelling, pat in live.items():
                if pat.search(line):
                    fails.append(
                        f"{rel}:{n}: names {spelling!r}, a backend selection the API "
                        f"crates no longer have (RFC-0071 D9, phase-506 W4d).\n"
                        f"    Build through the image crate (`cmake/NanoRosImageCrate.cmake`, "
                        f"or its `-P` script mode) instead.\n"
                        f"    {line.strip()}"
                    )
    return fails


def self_test():
    fails = 0

    def ok(msg):
        print(f"  ok   {msg}")

    def bad(msg, detail):
        nonlocal fails
        print(f"  FAIL {msg}: {detail}", file=sys.stderr)
        fails += 1

    feats = ["rmw-zenoh"]
    bodies = {
        "fake/glue.cmake": 'set(_f "rmw-cffi,rmw-zenoh-cffi,platform-posix")\n',
        "fake/fwd.toml": 'x = ["nros-c/rmw-zenoh"]\n',
        "fake/alias.sh": "cargo build --features cffi-zenoh-cffi\n",
        "fake/doc.cmake": "# never `rmw-zenoh-cffi` again\nset(_f rmw-zenoh)\n",
        "fake/crate.toml": 'nros-rmw-zenoh-cffi = { path = "x" }\nfeatures = ["rmw-zenoh"]\n',
        "fake/pkg.just": 'r:\n    cargo build -p nros-c --features "std,rmw-zenoh"\n',
        "fake/board.just": 'r:\n    cargo build -p nros-board-linux --features rmw-zenoh\n',
    }
    found = check_glue(feats, files=sorted(bodies), read=lambda p: bodies[p], reach_check=False)
    hit = sorted({f.split(":", 1)[0] for f in found})
    if hit == ["fake/alias.sh", "fake/fwd.toml", "fake/glue.cmake", "fake/pkg.just"]:
        ok("a -cffi spelling, a forward, an alias and a bare rmw-<name> on `-p nros-c` "
           "are caught; a comment, a crate name and a board's rmw-<name> are not")
    else:
        bad("glue scan", found)

    manifests = {
        API_MANIFESTS[0]: b'[features]\nrmw-cffi = []\nrmw-zenoh = []\n',
        API_MANIFESTS[1]: b'[features]\nrmw-cffi = []\n',
    }
    got = check_manifests(feats, read=lambda p: manifests[p])
    if len(got) == 1 and "'rmw-zenoh'" in got[0]:
        ok("an API manifest declaring a backend feature is refused")
    else:
        bad("manifest rule", got)

    if check_glue(feats, files=[], read=lambda p: "", reach_check=False) and check_glue(
        [], files=["x"], read=lambda p: "", reach_check=False
    ):
        ok("an empty file set and an empty backend set both REFUSE")
    else:
        bad("emptiness refusal", "reported OK over nothing")

    if fails:
        return 1
    print("check-nros-c-feature-agreement self-test OK")
    return 0


def main():
    if "--self-test" in sys.argv:
        return self_test()
    if self_test() != 0:
        sys.stderr.write("check-nros-c-feature-agreement: SELF-TEST FAILED\n")
        return 1
    feats = backend_features()
    fails = check_manifests(feats) + check_glue(feats)
    if fails:
        sys.stderr.write("check-nros-c-feature-agreement: FAILED\n\n")
        for f in fails:
            sys.stderr.write(f"  {f}\n\n")
        return 1
    print("check-nros-c-feature-agreement: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())
