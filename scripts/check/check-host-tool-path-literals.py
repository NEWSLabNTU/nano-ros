#!/usr/bin/env python3
"""check-host-tool-path-literals — issue 1850 (RFC-0103 D4).

Two first-party host tools each have ONE resolver, and code asks it rather
than spelling where the binary sits:

* `nros` — `nros_cli_bin` (scripts/build/cargo.sh). `nros_cli_bin
  --checkout-only` is "this checkout's built CLI (or the one `$NROS_CLI`
  names), or nothing" — what a gate PRECONDITION asks, so a gate that skips
  while this checkout's CLI is unbuilt never runs against PATH or the store
  (issue 1280's class). Python reaches the same function through
  `scripts/lib/source_locate.py`'s `nros_cli_bin`.
* `nros-launch-resolve` — `model_location::launch_resolver_from` (Rust),
  printed by `nros locate tool nros-launch-resolve` and read by the shell's
  `nros_launch_resolve_bin` (scripts/build/launch-resolver-identity.sh);
  `nros_launch_resolve_built_bin` (launch-resolve-stale.sh) is the PRODUCER's
  path and the checkout-only answer.

Before this gate the in-tree CLI's path was spelled on 49 code lines in 23
files beside its resolver, some as `${NROS_CLI:-<path>}` — rungs 1-2 of the
resolver restated without 3-4.

This refuses the spelling in CODE (comments are prose) under `just/`,
`justfile`, `scripts/`, `tests/` and the shell under `packages/testing/`. A
line carrying (or directly under a line carrying) `host-tool-path-ok: <why>`
is the resolver's own definition; an allowlisted file names its reason.

Out of scope, on purpose: `.github/` (a workflow builds the binary and caches
its directory — the producer, before any resolver can run), `activate.*`
(puts the in-tree CLI on PATH, so it cannot ask a CLI), and Rust (the CLI's own
crates, which ARE the ladders).

Buildless, ~0.2 s; self-tests its matcher on every run (negative control: a
re-added literal must fail).
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import comments  # noqa: E402
import git_grep  # noqa: E402  issue 1816 — a failed search is not "no match"

PATTERN = re.compile(
    r"packages/cli/target/release/nros(?![\w-])"
    r"|release/nros-launch-resolve(?![\w-])"
)
MARKER = "host-tool-path-ok:"

# path -> why it may spell the layout.
ALLOWED = {
    "scripts/check/check-host-tool-path-literals.py": "this gate's own pattern and self-tests",
    "scripts/check-build-profile-literals.sh": (
        "excludes these paths from ITS scan — a matcher input, never a path a "
        "program runs"
    ),
    "scripts/check-ci-cli-from-source.py": (
        "the workflow step text that gate requires a workflow to carry — the "
        "producer side (.github/), which is out of this gate's scope"
    ),
    "scripts/check-release-manifest.py": (
        "workflow staging text the release gate matches; the release job is "
        "the producer that copies both binaries into a dist"
    ),
    "tests/codegen-stamp-tests.sh": (
        "builds a SYNTHETIC checkout and writes a fake CLI at the in-tree path, "
        "so the layout is the fixture, not a lookup"
    ),
}

SCOPE = ["just", "justfile", "scripts", "tests", "packages/testing"]

SELF_TESTS = [
    # (text, lang, should_match)
    ('    if [ ! -x packages/cli/target/release/nros ]; then', "just", True),
    ('_cli="${NROS_CLI:-packages/cli/target/release/nros}"', "sh", True),
    ('bin="$root/packages/cli/target/release/nros"', "sh", True),
    ('built = ROOT / "packages/cli/target/release/nros"', "python", True),
    ('[ -x packages/cli/nros-launch-resolve/target/release/nros-launch-resolve ]', "sh", True),
    ('bin="${CARGO_TARGET_DIR:-$crate/target}/release/nros-launch-resolve"', "sh", True),
    ('# packages/cli/target/release/nros is what this replaced', "sh", False),
    ('cli="$(nros_cli_bin --checkout-only)" || cli=""', "sh", False),
    ('bin="$(nros_launch_resolve_bin "$root")"', "sh", False),
    ('x="packages/cli/target/release/nros-foo"', "sh", False),
    ('    printf x  # host-tool-path-ok: the definition\n'
     '    printf "%s" "$r/packages/cli/target/release/nros"', "sh", False),
]


def lang_of(path: str) -> str | None:
    return comments.lang_for(path)


def hits(text: str, lang: str) -> list[int]:
    code = comments.strip_comments(text, lang).splitlines()
    raw = text.splitlines()
    out = []
    for n, line in enumerate(code, 1):
        if not PATTERN.search(line):
            continue
        here = raw[n - 1] if n - 1 < len(raw) else ""
        prev = raw[n - 2] if n >= 2 else ""
        if MARKER in here or MARKER in prev:
            continue
        out.append(n)
    return out


def self_test() -> int:
    bad = 0
    for text, lang, want in SELF_TESTS:
        got = bool(hits(text, lang))
        if got != want:
            bad += 1
            print(f"  self-test FAILED: {text!r} ({lang}) matched={got}, want {want}")
    return bad


def main() -> int:
    if self_test():
        print("check-host-tool-path-literals --self-test: FAILED")
        return 1
    files = git_grep.run(
        ["-lIE", r"packages/cli/target/release/nros|release/nros-launch-resolve",
         "--", *SCOPE],
        ROOT,
    ).split()
    found = []
    seen_allowed = set()
    for f in files:
        if f.startswith("packages/testing/") and not f.endswith(".sh"):
            continue
        lang = lang_of(f)
        if lang is None:
            continue
        try:
            text = (ROOT / f).read_text(errors="replace")
        except OSError:
            continue
        lines = hits(text, lang)
        if not lines:
            continue
        if f in ALLOWED:
            seen_allowed.add(f)
            continue
        found += [f"{f}:{n}" for n in lines]
    stale = sorted(set(ALLOWED) - seen_allowed - {"scripts/check/check-host-tool-path-literals.py"})
    rc = 0
    if found:
        rc = 1
        print("check-host-tool-path-literals: a first-party host tool's path is SPELLED:\n")
        for h in found:
            print(f"  {h}")
        print(
            "\nAsk its resolver instead (issue 1850):"
            "\n  nros:  nros_cli_bin (scripts/build/cargo.sh) — `--checkout-only` for a"
            "\n         gate precondition ('this checkout's CLI, or nothing');"
            "\n         Python: source_locate.nros_cli_bin(checkout_only=...)"
            "\n  nros-launch-resolve: nros_launch_resolve_bin <root> (what sync runs) or"
            "\n         nros_launch_resolve_built_bin <root> (what this checkout built)"
        )
    if stale:
        rc = 1
        print("check-host-tool-path-literals: allowlist entries that no longer spell one "
              "(delete them):")
        for f in stale:
            print(f"  {f}")
    if rc == 0:
        print(f"check-host-tool-path-literals: OK ({len(SELF_TESTS)} self-test cases; "
              f"{len(ALLOWED)} allowlisted)")
    return rc


if __name__ == "__main__":
    sys.exit(main())
