#!/usr/bin/env python3
"""A release RECORDS its components, and blocks on exactly one equality.

RFC-0097 D7, phase-443 W2.

## What this gate is for

`release-nros.yml` used to ASSERT three versions equal — the release version,
`NROS_CODEGEN_VERSION`, and `[tool.nros].version` in the index. RFC-0097
measured why that is the wrong shape: on `main`, 2026-09-10, `packages/cli`
moved 710 times in 60 days, `nros-sdk-index.toml` 70, and
`NROS_CODEGEN_VERSION` 2 in the project's life. Only the last can make a user's
existing generated code wrong, so only the last may block anything.

D7 replaces asserting with RECORDING, in `share/nros/manifest.toml`. That change
is easy to make and easy to un-make: the retired assertions are each two lines
and each looks prudent in isolation, which is exactly how a coupling comes back.
So the properties below are gated rather than remembered.

## The five rules

* **R1 — the asset carries the manifest.** The staged prefix must contain
  `share/nros/<FILE_NAME>`, where FILE_NAME is READ OUT of the Rust module that
  parses it. Not a literal here: a gate that spells the filename itself would go
  green while the reader looked somewhere else.
* **R2 — the manifest is stamped BY THE BINARY.** The workflow must invoke
  `toolchain manifest … --write`, and must not compose the file itself with
  `printf`/`echo`/`cat >`. This is what makes the recorded `codegen` unable to
  disagree with the binary's own `abi_guard::EMITTED_VERSION` — it is taken from
  the constant and there is no flag for it.
* **R3 — the surviving equality is present.** The workflow must read
  `NROS_CODEGEN_VERSION` out of `packages/core/nros-core/src/codegen_version.rs`
  and compare it against the number in the manifest it just wrote.
* **R4 — no OTHER version comparison may block a release.** Every `exit 1` in
  the workflow must sit in a paragraph that is about `codegen`. Stated as a
  property of the fatal paths rather than as a ban on the two old wordings,
  because a reintroduction would be written in new words — the shape the
  2026-07-28 audit found in four gates whose reach was narrower than their rule.
* **R5 — the asset carries what a BUILD reads** (phase-447 A1, RFC-0099 D2).
  Two artifacts, both measured absent before phase-447 and each fatal on its
  own:

  - the **SDK root**, staged by `scripts/stage-sdk-root.sh` — that script owns
    the path list and VERIFIES what it wrote, so the rule here is that the
    workflow calls it, not a second copy of its inventory. Without it
    `nros build` bails "no nano-ros SDK root found, so board ids cannot be
    resolved."
  - the **launch resolver** binary, copied into the asset's `bin/`. It is not
    part of the SDK root (its own cargo workspace, embeds CPython) and every
    workspace configure goes through it: with the SDK root staged and this
    missing, `cmake -S . -B build` on a scaffolded project dies at
    `nros codegen entry` with a remedy naming a checkout the user does not have.

  Both NAMES are read out of the Rust that resolves them — `SHIPPED_SUBDIR` in
  `nros_launcher::checkout`, `LAUNCH_RESOLVER` in `cmd::ws` — for R1's reason: a
  gate that spells a path itself goes green while the reader looks elsewhere.

## Buildless

Pure text over one YAML file, three Rust files and one shell script. No CLI, no network, no store —
it passes in a pristine worktree, which is the bar for the fast lane.

Run: python3 scripts/check-release-manifest.py [--self-test]
"""

import argparse
import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WORKFLOW = os.path.join(ROOT, ".github", "workflows", "release-nros.yml")
READER = os.path.join(
    ROOT, "packages", "cli", "nros-cli-core", "src", "orchestration", "release_manifest.rs"
)
CODEGEN_SRC = "packages/core/nros-core/src/codegen_version.rs"
# R5 — the two Rust files that decide where a build looks for what the asset
# must carry. Read, never spelled here (R1's reason).
LAUNCHER = os.path.join(ROOT, "packages", "cli", "nros-launcher", "src", "checkout.rs")
WS = os.path.join(ROOT, "packages", "cli", "nros-cli-core", "src", "cmd", "ws.rs")
# The script that owns the SDK root's path list and verifies what it staged.
STAGE_SCRIPT = "scripts/stage-sdk-root.sh"
STAGE_PATH = os.path.join(ROOT, "scripts", "stage-sdk-root.sh")

# The Rust reader's own name for the file. Read, never spelled here.
FILE_NAME_RE = re.compile(r'pub const FILE_NAME: &str = "([^"]+)"')
SHIPPED_SUBDIR_RE = re.compile(r'pub const SHIPPED_SUBDIR: &str = "([^"]+)"')
LAUNCH_RESOLVER_RE = re.compile(r'const LAUNCH_RESOLVER: &str = "([^"]+)"')

# A shell line that composes the manifest by hand instead of asking the binary.
HAND_WRITTEN_RE = re.compile(
    r"^\s*(printf|echo|cat)\b.*(codegen\s*=|>\s*\S*share/nros/manifest\.toml)"
)
STAMP_RE = re.compile(r"toolchain\s+manifest\b")


def manifest_file_name(reader_text):
    """FILE_NAME as the Rust reader declares it."""
    m = FILE_NAME_RE.search(reader_text)
    return m.group(1) if m else None


def sdk_root_violations(workflow_text, launcher_text, ws_text, stage_text):
    """R5 — the asset carries what a build reads (phase-447 A1, RFC-0099 D2).

    Every clause below is about the STAGED PREFIX (`"$stage"`), never about the
    bare name appearing somewhere in the file. The first version of this rule
    asked `f"bin/{resolver}" in workflow_text` and stayed GREEN when the staging
    `cp` was deleted, because the install probe two steps later mentions the same
    path — a gate satisfied by the sentence that checks the thing rather than by
    the thing.
    """
    bad = []

    m = SHIPPED_SUBDIR_RE.search(launcher_text)
    if not m:
        bad.append(
            ("R5", f"{LAUNCHER} declares no `pub const SHIPPED_SUBDIR` — nothing "
                   "says where an installed toolchain's SDK root lives")
        )
    else:
        subdir = m.group(1)
        # The script owns the path list AND verifies what it wrote, so the
        # workflow's job is to CALL it against the staged prefix.
        if f'{STAGE_SCRIPT} "$stage"' not in workflow_text:
            bad.append(
                ("R5", f'the release does not run `{STAGE_SCRIPT} "$stage"` — without '
                       "the SDK root in the asset a released `nros` resolves its board "
                       "catalog from a checkout it does not have, and `nros build` "
                       "bails on the first release ever cut")
            )
        # …and the script must write where the CLI looks. This is the pair that
        # can drift silently: both sides are correct in isolation and the asset
        # lands one directory away from `shipped_sdk_root_beside`.
        sm = re.search(r'^SHIPPED_SUBDIR="([^"]+)"', stage_text, re.M)
        if not sm:
            bad.append(
                ("R5", f"{STAGE_SCRIPT} declares no SHIPPED_SUBDIR — it and "
                       f"{LAUNCHER} must name one directory")
            )
        elif sm.group(1) != subdir:
            bad.append(
                ("R5", f"{STAGE_SCRIPT} stages into {sm.group(1)} but the CLI looks in "
                       f"{subdir} — the asset would land one directory away from "
                       "`shipped_sdk_root_beside`, which answers None and reads as "
                       "'no SDK root found'")
            )

    m = LAUNCH_RESOLVER_RE.search(ws_text)
    if not m:
        bad.append(
            ("R5", f"{WS} declares no `const LAUNCH_RESOLVER` — nothing names the "
                   "launch resolver binary")
        )
    else:
        resolver = m.group(1)
        if f'"$stage/bin/{resolver}"' not in workflow_text:
            bad.append(
                ("R5", f'the release copies nothing to "$stage/bin/{resolver}" — it is '
                       "not part of the SDK root (own workspace, embeds CPython), and "
                       "every workspace configure resolves its launch file through it, "
                       "so a scaffolded project dies at `nros codegen entry`")
            )
    return bad


def fatal_paragraphs(text):
    """Every `exit 1` and the contiguous non-blank block it ends.

    A paragraph is the run of non-blank lines up to and including the `exit 1`.
    Comments count: the reason a guard exists is written above it, and a guard
    whose paragraph never says `codegen` is a guard about something else.
    """
    lines = text.splitlines()
    out = []
    for i, line in enumerate(lines):
        if "exit 1" not in line:
            continue
        start = i
        while start > 0 and lines[start - 1].strip():
            start -= 1
        out.append((i + 1, "\n".join(lines[start : i + 1])))
    return out


def violations(workflow_text, reader_text, launcher_text, ws_text, stage_text):
    """Every rule broken, as (rule, message) pairs."""
    bad = []
    bad += sdk_root_violations(workflow_text, launcher_text, ws_text, stage_text)
    name = manifest_file_name(reader_text)
    if not name:
        bad.append(("R1", f"{READER} declares no `pub const FILE_NAME` to read the asset path from"))
        return bad

    staged = f"share/nros/{name}"
    if staged not in workflow_text:
        bad.append(
            (
                "R1",
                f"the release stages no {staged} — the Rust reader looks there and "
                f"would find nothing (`nros pin` could not answer the re-emit question)",
            )
        )

    if not STAMP_RE.search(workflow_text) or "--write" not in workflow_text:
        bad.append(
            (
                "R2",
                "the manifest is not stamped by `nros toolchain manifest … --write`; "
                "a manifest the workflow composes itself can record a codegen the "
                "binary does not emit",
            )
        )
    for n, line in enumerate(workflow_text.splitlines(), 1):
        if HAND_WRITTEN_RE.match(line):
            bad.append(("R2", f"line {n} composes the manifest by hand: {line.strip()[:90]}"))

    if CODEGEN_SRC not in workflow_text:
        bad.append(
            (
                "R3",
                f"the release never reads {CODEGEN_SRC} — the one equality that "
                "survives (manifest codegen == the tree's NROS_CODEGEN_VERSION) is gone",
            )
        )
    elif not any("codegen" in p for _, p in fatal_paragraphs(workflow_text)):
        bad.append(
            (
                "R3",
                "the tree's NROS_CODEGEN_VERSION is read but no failing path depends "
                "on it — a check that cannot fail is a check that is not there",
            )
        )

    for n, para in fatal_paragraphs(workflow_text):
        if "codegen" not in para:
            bad.append(
                (
                    "R4",
                    f"line {n}: a release-blocking failure that is not about codegen:\n"
                    + "\n".join("        " + x for x in para.splitlines()[-4:]),
                )
            )
    return bad


# --------------------------------------------------------------------------
# self-test — runs on the NORMAL path too, so a gate that cannot fail cannot
# report OK.
# --------------------------------------------------------------------------

GOOD = """
      - name: Record what this release is made of
        run: |
          set -euo pipefail
          "$nros" toolchain manifest --store-version "$v" --write "$stage/share/nros/manifest.toml"
          # THE ONE EQUALITY THAT SURVIVES.
          manifest_codegen="$(sed -n 's/^codegen = \\([0-9]\\+\\)$/\\1/p' "$stage/share/nros/manifest.toml")"
          tree_codegen="$(sed -n 's/.*= \\([0-9]\\+\\);.*/\\1/p' packages/core/nros-core/src/codegen_version.rs)"
          if [ "$manifest_codegen" != "$tree_codegen" ]; then
              echo "::error::codegen mismatch" >&2
              exit 1
          fi
"""

REINTRODUCED_INDEX_ASSERT = GOOD + """
          indexed="$(sed -n 's/^version = "\\(.*\\)"/\\1/p' nros-sdk-index.toml)"
          if [ "$indexed" != "$want_version" ]; then
              echo "::error::bump the index first" >&2
              exit 1
          fi
"""

REINTRODUCED_CRATE_ASSERT = GOOD + """
          crate="$("$nros" --version | awk '{print $NF}')"
          case "$want_version" in
              "$crate"-nros*) ;;
              *) echo "::error::not <crate>-nrosN" >&2; exit 1 ;;
          esac
"""

HAND_WRITTEN = """
      - name: Record
        run: |
          printf 'codegen = %s\\n' "$n" >"$stage/share/nros/manifest.toml"
          tree_codegen="$(sed -n 'p' packages/core/nros-core/src/codegen_version.rs)"
          if [ "$a" != "$tree_codegen" ]; then
              echo "codegen bad"
              exit 1
          fi
"""

NO_EQUALITY = """
      - name: Record
        run: |
          "$nros" toolchain manifest --store-version "$v" --write "$stage/share/nros/manifest.toml"
"""

READER_STUB = 'pub const FILE_NAME: &str = "manifest.toml";'
LAUNCHER_STUB = 'pub const SHIPPED_SUBDIR: &str = "share/nano-ros";'
WS_STUB = 'const LAUNCH_RESOLVER: &str = "nros-launch-resolve";'

# R5's half of the workflow, appended to whichever body a case is about, so the
# R1–R4 cases keep asserting exactly what they always did.
STAGES_WHAT_A_BUILD_READS = """
          sh scripts/stage-sdk-root.sh "$stage"
          cp packages/cli/nros-launch-resolve/target/release/nros-launch-resolve \\
              "$stage/bin/nros-launch-resolve"
"""

# The staging script's half of the R5 pair.
STAGE_STUB = 'SHIPPED_SUBDIR="share/nano-ros"\n'

# The shape that fooled R5's first version: the staging `cp` is GONE, and the
# install probe still names the path. A substring rule reads this as OK.
PROBE_ONLY = """
          sh scripts/stage-sdk-root.sh "$stage"
      - name: Prove the asset installs
        run: |
          "$RUNNER_TEMP/probe/bin/nros-launch-resolve" --version
"""


def self_test():
    G = GOOD + STAGES_WHAT_A_BUILD_READS
    S = STAGE_STUB
    cases = [
        ("the shipped shape passes", G, READER_STUB, LAUNCHER_STUB, WS_STUB, S, set()),
        (
            "a reintroduced index assertion",
            REINTRODUCED_INDEX_ASSERT + STAGES_WHAT_A_BUILD_READS,
            READER_STUB, LAUNCHER_STUB, WS_STUB, S, {"R4"},
        ),
        (
            "a reintroduced crate-prefix assertion",
            REINTRODUCED_CRATE_ASSERT + STAGES_WHAT_A_BUILD_READS,
            READER_STUB, LAUNCHER_STUB, WS_STUB, S, {"R4"},
        ),
        (
            "a hand-composed manifest",
            HAND_WRITTEN + STAGES_WHAT_A_BUILD_READS,
            READER_STUB, LAUNCHER_STUB, WS_STUB, S, {"R2"},
        ),
        (
            "no surviving codegen equality",
            NO_EQUALITY + STAGES_WHAT_A_BUILD_READS,
            READER_STUB, LAUNCHER_STUB, WS_STUB, S, {"R3"},
        ),
        (
            "a renamed manifest file the workflow did not follow",
            G,
            'pub const FILE_NAME: &str = "components.toml";',
            LAUNCHER_STUB, WS_STUB, S, {"R1"},
        ),
        # R5 — one case per artifact, because they fail independently and a
        # rule written for one of them would have caught only that one.
        ("no SDK root staged", GOOD, READER_STUB, LAUNCHER_STUB, WS_STUB, S, {"R5"}),
        (
            "the SDK root staged but no launch resolver",
            GOOD + '\n          sh scripts/stage-sdk-root.sh "$stage"\n',
            READER_STUB, LAUNCHER_STUB, WS_STUB, S, {"R5"},
        ),
        # The mutant that beat R5's first version: staging deleted, the install
        # probe still naming the path. A bare-substring rule reads this as OK.
        (
            "only the install probe names the resolver",
            GOOD + PROBE_ONLY,
            READER_STUB, LAUNCHER_STUB, WS_STUB, S, {"R5"},
        ),
        (
            "the resolver copied but the SDK root inlined instead of scripted",
            GOOD + '\n          cp -r cmake config packages "$stage/share/nano-ros/"\n'
                 + '          cp a/nros-launch-resolve "$stage/bin/nros-launch-resolve"\n',
            READER_STUB, LAUNCHER_STUB, WS_STUB, S, {"R5"},
        ),
        (
            "the shipped subdir renamed and the workflow left behind",
            G, READER_STUB,
            'pub const SHIPPED_SUBDIR: &str = "share/nros-sdk";', WS_STUB, S, {"R5"},
        ),
        (
            "the resolver renamed and the workflow left behind",
            G, READER_STUB, LAUNCHER_STUB,
            'const LAUNCH_RESOLVER: &str = "nros-resolve-launch";', S, {"R5"},
        ),
        (
            "the staging script and the CLI name different directories",
            G, READER_STUB, LAUNCHER_STUB, WS_STUB,
            'SHIPPED_SUBDIR="share/nros/sdk"\n', {"R5"},
        ),
    ]
    failures = 0
    for label, wf, reader, launcher, ws, stage, want in cases:
        got = {rule for rule, _ in violations(wf, reader, launcher, ws, stage)}
        if got != want:
            print(f"  self-test FAIL [{label}]: expected {sorted(want) or 'no violations'}, got {sorted(got)}")
            failures += 1
    if failures:
        print(f"check-release-manifest self-test: {failures} case(s) FAILED")
        return 1
    print(f"check-release-manifest self-test: OK ({len(cases)} cases)")
    return 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args()
    if args.self_test:
        return self_test()
    if self_test() != 0:
        return 1

    for path in (WORKFLOW, READER, LAUNCHER, WS, STAGE_PATH):
        if not os.path.isfile(path):
            print(f"check-release-manifest: {path} is missing — nothing records the release")
            return 1
    with open(WORKFLOW, encoding="utf-8") as fh:
        workflow_text = fh.read()
    with open(READER, encoding="utf-8") as fh:
        reader_text = fh.read()
    with open(LAUNCHER, encoding="utf-8") as fh:
        launcher_text = fh.read()
    with open(WS, encoding="utf-8") as fh:
        ws_text = fh.read()
    with open(STAGE_PATH, encoding="utf-8") as fh:
        stage_text = fh.read()

    bad = violations(workflow_text, reader_text, launcher_text, ws_text, stage_text)
    if bad:
        print("check-release-manifest: the release does not record its components (RFC-0097 D7):")
        for rule, msg in bad:
            print(f"  [{rule}] {msg}")
        print()
        print("  A release DECLARES what it is made of; it does not assert three")
        print("  versions equal. Only `codegen` can invalidate a user's existing")
        print("  generated code, so only `codegen` may block a release.")
        return 1

    name = manifest_file_name(reader_text)
    fatal = len(fatal_paragraphs(workflow_text))
    subdir = SHIPPED_SUBDIR_RE.search(launcher_text).group(1)
    resolver = LAUNCH_RESOLVER_RE.search(ws_text).group(1)
    print(
        f"check-release-manifest: OK — the asset records share/nros/{name}, "
        f"stamped by the binary; {fatal} release-blocking path(s), all about codegen; "
        f"it carries {subdir} and bin/{resolver}."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
