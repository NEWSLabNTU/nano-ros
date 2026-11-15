#!/usr/bin/env python3
"""Every script that runs an offline `nros build` warms its graphs first.

`nros build --offline` resolves every cargo graph it drives from the local
cache. Since issue 2444 that includes the cargo zephyr-lang-rust runs inside a
west build. So the cache must already hold each graph the build resolves: the
image's generated cargo root, or a west Rust app's. ONE function does that for
every road, `nros_warm_leaf_cache` (`scripts/build/leaf-cache.sh`). A driver
that skips it builds on whatever an earlier warm happened to share, and fails
on the first crate only its graph needs.

That happened on live-peer 43675564346 (2026-11-15): `failed to download
annotate-snippets v0.11.5 … --offline was specified`. Two west-road drivers
warmed only the repo root's graph, and a third built a workspace's default
images with no image warm at all.

The rule: a tracked shell or `just` file that runs `nros build … --offline`
(other than `--dry-run`) calls `nros_warm_leaf_cache` somewhere in the same
file. This is a file-level check, deliberately: it catches a new driver that
never heard of the warm, the shape that recurred, without parsing control
flow.

Run: python3 scripts/check/check-offline-build-warmed.py [--self-test]
"""

import re
import subprocess
import sys

# `nros build ... --offline`, in each spelling a driver uses: the bare `nros`,
# the CLI through a variable (`"$NROS_CLI_BIN" build`, `"$nros_bin" build`), or
# an argv array started at the verb (`nargs=(build ...`). A CARGO offline build
# is not the subject: the rule is about the graphs `nros build` resolves, and a
# self-test's `cargo build --offline` of its own scratch crate resolves only
# that crate (check-shared-build-dir.sh). Reaching wider than the rule was
# issue 0196's shape the other way.
OFFLINE_BUILD = re.compile(
    r"""(\bnros["']?|\$\{?\w*(?:nros|NROS|cli|CLI)\w*\}?["']?|\()\s*build\b.*--offline""")
WARM_CALL = re.compile(r"(^|[^_\w])nros_warm_leaf_cache\b(?!\s*\(\))")


def problems(name, text):
    if name.endswith("leaf-cache.sh"):
        return []  # the warm itself (its own dry run is `--dry-run`)
    lines = [l for l in text.splitlines() if not l.lstrip().startswith("#")]
    builds = [l for l in lines if OFFLINE_BUILD.search(l) and "--dry-run" not in l]
    if not builds:
        return []
    if any(WARM_CALL.search(l) for l in lines):
        return []
    return [f"{name}: runs `nros build --offline` and never calls nros_warm_leaf_cache:\n      {builds[0].strip()}"]


def self_test():
    cases = [
        ("an offline build with no warm", "nros build img --workspace . --offline\n", True),
        ("an offline build after the warm", "nros_warm_leaf_cache \"$ws\" img\nnros build img --offline\n", False),
        ("a dry run alone", "nros build img --offline --dry-run\n", False),
        ("the warm named only in a comment", "# nros_warm_leaf_cache\nnros build x --offline\n", True),
        ("the warm DEFINED, not called", "nros_warm_leaf_cache() {\n:\n}\nnros build x --offline\n", True),
        ("no offline build", "nros build img\n", False),
        ("the CLI through a variable", "\"$NROS_CLI_BIN\" build --workspace . --offline\n", True),
        ("an argv array at the verb", "nargs=(build \"$img\" --workspace . --offline -- -d x)\n", True),
        ("a self-test's offline CARGO build of its scratch crate",
         "cargo build -q --release --offline --target-dir \"$tdir\"\n", False),
        ("cargo through a variable", "\"$cargo\" build --offline\n", False),
    ]
    bad = 0
    for desc, text, want in cases:
        got = bool(problems("x.sh", text))
        print(f"  {'ok  ' if got == want else 'FAIL'} {desc}: {'refused' if got else 'accepted'}")
        bad += got != want
    return 1 if bad else 0


def main(argv):
    if argv == ["--self-test"]:
        return self_test()
    if self_test():
        print("check-offline-build-warmed self-test: FAILED", file=sys.stderr)
        return 1
    files = subprocess.run(
        ["git", "ls-files", "-z", "*.sh", "*.just", "justfile"],
        capture_output=True, check=True,
    ).stdout.decode().split("\0")
    found = []
    for f in filter(None, files):
        try:
            found += problems(f, open(f, encoding="utf-8").read())
        except (FileNotFoundError, UnicodeDecodeError):
            continue
    if found:
        print("check-offline-build-warmed: FAILED", file=sys.stderr)
        for p in found:
            print("  " + p, file=sys.stderr)
        print("\nSource scripts/build/leaf-cache.sh and call nros_warm_leaf_cache <workspace> [<image>]\n"
              "before the offline build: it warms every road's graph, west Rust apps included.", file=sys.stderr)
        return 1
    print("check-offline-build-warmed: OK (every offline `nros build` driver warms first)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
