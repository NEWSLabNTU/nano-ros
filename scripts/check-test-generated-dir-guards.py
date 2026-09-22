#!/usr/bin/env python3
"""A generated DIRECTORY's existence may not decide whether a test runs.

Issue 1430, the class behind issue 1411. Buildless, self-testing on the normal
path, whole-test-corpus reach.

## The shape

`build_verb_pipeline.rs`, phase-383 W3.b to issue 1411::

    let entry = tmp.path().join("build/posix-zenoh/native_entry");
    if !entry.is_dir() {
        // the launch resolver is not built -- assert the weaker property
        return;
    }
    let manifest = std::fs::read_to_string(entry.join("Cargo.toml")).unwrap();

`entry.is_dir()` stood in for "the entry PACKAGE was generated". It was a fair
stand-in when it was written, because the entry generator was then the only
thing that ever created `build/<coord>/<entry>/`. RFC-0098 D1 (phase-445 W4/W5,
carried into phase-454) gave that directory a second producer: `cmd::build`
writes the image's `nros-cargo.toml` there **unconditionally**, including on the
path where model resolution has already failed and warned. From that commit the
directory existed either way, the guard could no longer be false, the documented
fallback became unreachable, and the `unwrap()` five lines down panicked on a
file that was never generated.

The defect is not `is_dir()`. It is that **a guard standing in for "the artifact
was produced" keeps reading as correct after the directory it names grows a
producer**, and nothing in the tree notices. 1411 closed the instances; this
closes the class's visibility.

## The rule, and why it is exactly this narrow

A path that names a directory under a GENERATED-OUTPUT root (`build/`,
`target/`, `install/`, `out/`) may not be probed with `is_dir()` / `exists()` /
`try_exists()` in a condition **whose false branch replaces the test** -- i.e.
the probe is negated, or the `if` carries an `else`. Inside an assertion it is
fine: "this directory exists" is a legitimate property to assert, and 20 sites
in this tree assert one.

Three narrowings, each MEASURED against the 336 test targets rather than chosen:

1. **A positive condition with no `else` is NOT flagged.** The one live site of
   that shape is `orchestration_self_bringup_cargo_metadata.rs:195`::

       let preserved = out_root.join("metadata");
       if preserved.is_dir() {
           for entry in fs::read_dir(&preserved).unwrap() { assert!(...) }
       }

   Its property is an ABSENCE ("no synthetic `Cargo.toml` was preserved"), which
   a missing `metadata/` satisfies, so the guard is defensible and flagging it
   would be flagging correct code -- what issue 1430 explicitly says to narrow
   rather than ship. What separates it from 1411 is decidable: 1411's guard
   chose NOT to run the test (it returned), this one IS the test. Recorded as a
   deliberate limit, in the shape of `check-no-vacuous-tests`'s own.

2. **`exists()` / `try_exists()` are in the rule, and cost nothing.** Measured:
   zero of them are conditions on a generated-output directory anywhere in the
   corpus; all nine such uses are assertions. So including them flags nothing
   today and closes a one-token bypass of `is_dir()`.

3. **The path must be EVIDENCED as a directory**, not merely extensionless. A
   probe is in scope when it is `is_dir()` (nobody asks that of a binary) or
   when the same binding is a `join()` / `read_dir()` / `create_dir_all()`
   receiver. Without this, six extensionless BINARY paths under a build root
   (`build/xrce-agent/MicroXRCEAgent`, `build/cyclonedds/bin/idlc`,
   `packages/cli/target/release/nros`, ...) would be flagged for the correct
   spelling -- naming the file is the fix, not the defect.

## Reach

Every tracked test target -- `git ls-files '*/tests/*.rs' 'tests/*.rs'`, which
git matches across directory separators, so nested files like
`nros-cli-core/tests/common/mod.rs` are included. 336 files. The rule is about
test guards generally, not about the one file that had the defect (CLAUDE.md,
issue 0196), and the corpus is the same one `check-no-vacuous-tests` and
`check-test-precondition-guards` read.

## Why a separate gate and not a case in `check-no-vacuous-tests`

Measured, not preferred. That gate's unit of analysis is a `#[test]`-attributed
fn BODY (`test_bodies()` yields nothing else). 20 of the 79 generated-output
directory path bindings in this corpus -- a quarter -- sit in plain helper fns
(`boot_and_connect`, `run_cell`, `spawn_probe`, `stage_project`, ...), so
hosting this rule there would put a quarter of the shape's living space out of
reach by construction, which is the 0196 shape. This gate reads FILES, not test
bodies, and is a sibling of `check-test-precondition-guards` (a helper's
SIGNATURE) rather than a case inside either.

## What to write instead

Name the artifact the test is about::

    let manifest = entry.join("Cargo.toml");
    assert!(manifest.is_file(), "no entry package was generated at {} -- the \\
        settings file may be there (it is written either way)", entry.display());

A directory is a container; its existence is a claim about whoever creates
containers, and that set grows. A file is the artifact. And note that a guard
around `create_dir_all` needs no probe at all -- it is idempotent.
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent

# A path segment that means "generated output". `build` is where issue 1411
# happened; the other three are the tree's remaining output roots, and including
# them was measured to add zero conditions and six assertions -- it changes no
# verdict today and keeps the rule's CLAIM ("this is generated output") as wide
# as the rule's reason.
OUTPUT_ROOTS = ("build", "target", "install", "out")

PROBES = ("is_dir", "exists", "try_exists")

# Evidence that a binding is a DIRECTORY rather than an extensionless file.
DIR_EVIDENCE = ("join", "read_dir", "create_dir_all", "read_dir_sorted")

ASSERT_MACROS = (
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
)


# ---------------------------------------------------------------- source prep


def blank_comments(src: str) -> str:
    """Replace comment bodies with spaces, preserving every byte offset.

    Offsets are load-bearing here: the classifier locates a probe by character
    position inside a condition or an assertion span, so a comment cannot be
    DELETED. Issue 1430's first exploratory sweep reported two findings, one of
    which was the prose in 1411's own explanatory comment quoting the code it
    replaced -- a gate that reads its own fix's comment as a violation.
    """
    out = list(src)
    i, n = 0, len(src)
    while i < n:
        c = src[i]
        if c == '"':
            i += 1
            while i < n and src[i] != '"':
                i += 2 if src[i] == "\\" else 1
            i += 1
            continue
        if src.startswith("//", i):
            while i < n and src[i] != "\n":
                out[i] = " "
                i += 1
            continue
        if src.startswith("/*", i):
            depth = 1
            out[i] = out[i + 1] = " "
            i += 2
            while i < n and depth:
                if src.startswith("/*", i):
                    depth += 1
                    out[i] = out[i + 1] = " "
                    i += 2
                    continue
                if src.startswith("*/", i):
                    depth -= 1
                    out[i] = out[i + 1] = " "
                    i += 2
                    continue
                if src[i] != "\n":
                    out[i] = " "
                i += 1
            continue
        i += 1
    return "".join(out)


# ---------------------------------------------------------------- path naming


def names_a_generated_dir(lit: str) -> bool:
    """Does this string literal name a path under a generated-output root?

    The LAST segment must not carry an extension -- `build/x/CMakeLists.txt`
    names a file and is out of scope entirely.
    """
    segs = [s for s in lit.split("/") if s]
    if not segs:
        return False
    if not any(s in OUTPUT_ROOTS for s in segs):
        return False
    if segs[-1] in OUTPUT_ROOTS:
        return True
    return "." not in segs[-1]


LET_RE = re.compile(r"\blet\s+(?:mut\s+)?(\w+)\s*(?::[^=;]*)?=")
LIT_RE = re.compile(r'"([^"\n]*)"')
JOIN_RE = re.compile(r'\blet\s+(?:mut\s+)?(\w+)\s*(?::[^=;]*)?=\s*&?(\w+)\s*\.join\(\s*"([^"]*)"')


def generated_dir_names(src: str) -> dict[str, str]:
    """`let` bindings whose value names a generated-output DIRECTORY.

    One hop from a literal, then up to three hops through
    `let child = parent.join("sub")` -- enough for the real spellings
    (`out_root` -> `metadata`) without becoming a dataflow analysis.
    """
    names: dict[str, str] = {}
    for line in src.split("\n"):
        m = LET_RE.search(line)
        if not m:
            continue
        for lit in LIT_RE.finditer(line[m.end():]):
            if names_a_generated_dir(lit.group(1)):
                names[m.group(1)] = lit.group(1)
                break
    for _ in range(3):
        for line in src.split("\n"):
            j = JOIN_RE.search(line)
            if not j or j.group(2) not in names or j.group(1) in names:
                continue
            segs = [s for s in j.group(3).split("/") if s]
            if segs and "." not in segs[-1]:
                names[j.group(1)] = names[j.group(2)] + "/" + j.group(3)
    return names


def has_dir_evidence(src: str, name: str) -> bool:
    """Is `name` used as a directory receiver anywhere in the file?

    `p.join(..)`, `read_dir(&p)`, `create_dir_all(&p)`. This is what keeps six
    extensionless BINARY paths under a build root out of the rule.
    """
    if re.search(r"\b" + re.escape(name) + r"\s*\.join\s*\(", src):
        return True
    for f in DIR_EVIDENCE[1:]:
        if re.search(r"\b" + re.escape(f) + r"\s*\(\s*&?" + re.escape(name) + r"\b", src):
            return True
    return False


# ---------------------------------------------------------------- span finding


def _match_paren(src: str, i: int) -> int:
    """Index just past the `)` matching the `(` at `i`; len(src) if unbalanced."""
    depth = 0
    n = len(src)
    while i < n:
        c = src[i]
        if c == '"':
            i += 1
            while i < n and src[i] != '"':
                i += 2 if src[i] == "\\" else 1
        elif c == "(":
            depth += 1
        elif c == ")":
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    return n


def _match_brace(src: str, i: int) -> int:
    """Index just past the `}` matching the `{` at `i`; len(src) if unbalanced."""
    depth = 0
    n = len(src)
    while i < n:
        c = src[i]
        if c == '"':
            i += 1
            while i < n and src[i] != '"':
                i += 2 if src[i] == "\\" else 1
        elif c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                return i + 1
        i += 1
    return n


def assertion_spans(src: str) -> list[tuple[int, int]]:
    """(start, end) of every `assert*!( ... )` argument list.

    A probe inside one is an ASSERTION about existence, which is a legitimate
    property under test and out of the rule. Assertion wins over condition:
    `if x { assert!(p.is_dir()) }` is an assertion, not a guard.
    """
    spans = []
    for m in re.finditer(r"\b(" + "|".join(ASSERT_MACROS) + r")\s*!\s*\(", src):
        spans.append((m.start(), _match_paren(src, m.end() - 1)))
    return spans


def condition_spans(src: str) -> list[tuple[int, int, bool]]:
    """(start, end, has_else) for every `if` / `while` condition in `src`.

    The condition runs from just past the keyword to the `{` that opens the
    block at bracket depth 0, so a closure or a call inside the condition does
    not end it early. `has_else` is whether an `else` follows the block -- the
    other way a condition's FALSE branch can replace the test.
    """
    out = []
    for m in re.finditer(r"\b(if|while)\b", src):
        i, n = m.end(), len(src)
        depth = 0
        while i < n:
            c = src[i]
            if c == '"':
                i += 1
                while i < n and src[i] != '"':
                    i += 2 if src[i] == "\\" else 1
                i += 1
                continue
            if c in "([":
                depth += 1
            elif c in ")]":
                depth -= 1
            elif c == "{" and depth == 0:
                break
            elif c == ";" and depth == 0:
                i = n  # not an `if <cond> {` after all
                break
            i += 1
        if i >= n:
            continue
        body_end = _match_brace(src, i)
        tail = src[body_end:body_end + 40].lstrip()
        out.append((m.end(), i, tail.startswith("else")))
    return out


def in_span(pos: int, spans) -> bool:
    return any(s <= pos < e for s, e in spans)


# ---------------------------------------------------------------- the rule


def negated(cond: str, probe_rel: int) -> bool:
    """Is the probe at `probe_rel` inside `cond` under a `!`?

    Walks back over the receiver chain (identifiers, `.`, `()`, `[]`, `&`,
    whitespace and opening parens) to whatever precedes it. A `!` there negates
    the probe: `!p.is_dir()`, `!(p.exists())`, `&& !p.is_dir()`.
    """
    i = probe_rel - 1
    while i >= 0 and (cond[i].isalnum() or cond[i] in "_.:)]>&? \t\n"):
        if cond[i] == ")":
            # skip a balanced call/group backwards
            depth = 1
            i -= 1
            while i >= 0 and depth:
                if cond[i] == ")":
                    depth += 1
                elif cond[i] == "(":
                    depth -= 1
                i -= 1
            continue
        i -= 1
    while i >= 0 and cond[i] in "( \t\n":
        i -= 1
    return i >= 0 and cond[i] == "!"


def scan_source(rel: str, src_raw: str) -> list[str]:
    src = blank_comments(src_raw)
    names = generated_dir_names(src)
    if not names:
        return []
    asserts = assertion_spans(src)
    conds = condition_spans(src)
    line_of = [0] * (len(src) + 1)
    ln = 1
    for idx, ch in enumerate(src):
        line_of[idx] = ln
        if ch == "\n":
            ln += 1
    line_of[len(src)] = ln

    found = []
    for name, lit in sorted(names.items()):
        joined = has_dir_evidence(src, name)
        pat = re.compile(
            r"\b" + re.escape(name)
            + r"\b((?:\s*\.\w+\s*\([^()]*\))*?)\s*\.(" + "|".join(PROBES) + r")\s*\(\s*\)"
        )
        for m in pat.finditer(src):
            probe = m.group(2)
            # Rule narrowing 3 -- the path must be EVIDENCED as a directory.
            if probe != "is_dir" and not joined:
                continue
            if in_span(m.start(), asserts):
                continue
            for cs, ce, has_else in conds:
                if not cs <= m.start() < ce:
                    continue
                cond = src[cs:ce]
                neg = negated(cond, m.start() - cs)
                if not (neg or has_else):
                    continue
                spelling = f"!{name}.{probe}()" if neg else f"{name}.{probe}()"
                why = "negated" if neg else "the `if` has an `else`"
                found.append(
                    f"{rel}:{line_of[m.start()]}: `{spelling}` guards the test on the "
                    f"EXISTENCE of the generated directory `{lit}` ({why}) — name the "
                    f"artifact instead"
                )
                break
    return found


# ---------------------------------------------------------------- corpus


def tracked_test_files() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", "*/tests/*.rs", "tests/*.rs"],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split()
    return sorted(out)


def scan(files: list[str]) -> list[str]:
    bad: list[str] = []
    for rel in files:
        p = REPO / rel
        if not p.is_file():
            continue
        bad += scan_source(rel, p.read_text(encoding="utf-8", errors="replace"))
    return bad


# ---------------------------------------------------------------- self-test

PRE_1411 = """
#[test]
fn a_cargo_image_generates_an_entry_from_the_launch_file() {
    let tmp = tempfile::tempdir().unwrap();
    let entry = tmp.path().join("build/posix-zenoh/native_entry");
    if !entry.is_dir() {
        eprintln!("the launch resolver is not built");
        assert!(plans[0].handoff.is_some());
        return;
    }
    let manifest = std::fs::read_to_string(entry.join("Cargo.toml")).unwrap();
    assert!(manifest.contains("talker_pkg"));
}
"""

POST_1411 = """
#[test]
fn a_cargo_image_generates_an_entry_from_the_launch_file() {
    let tmp = tempfile::tempdir().unwrap();
    let entry = tmp.path().join("build/posix-zenoh/native_entry");
    let manifest_path = entry.join("Cargo.toml");
    assert!(manifest_path.is_file(), "no entry package at {}", entry.display());
    let manifest = std::fs::read_to_string(&manifest_path).unwrap();
    assert!(manifest.contains("talker_pkg"));
}
"""

# The live positive-no-else site, reproduced. Rule narrowing 1: NOT flagged.
TOLERANT_POSITIVE = """
#[test]
fn plan_system_skips_synthetic_metadata() {
    let out_root = root.join("build/cargo_self_bringup/nros");
    let preserved = out_root.join("metadata");
    if preserved.is_dir() {
        for entry in fs::read_dir(&preserved).unwrap() {
            assert!(!entry.unwrap().path().ends_with("Cargo.toml"));
        }
    }
}
"""

SELF_TESTS: list[tuple[str, str, int]] = [
    ("the pre-1411 guard is caught", PRE_1411, 1),
    ("the post-1411 spelling passes", POST_1411, 0),
    (
        "1411's own explanatory COMMENT quoting the old code is not a finding",
        POST_1411 + """
// Issue 1411 -- this read `if !entry.is_dir()` and fell back to a weaker
// assertion. Do not reintroduce it.
""",
        0,
    ),
    (
        "a block comment quoting it is not a finding either",
        POST_1411 + "/* if !entry.is_dir() { return; } */\n",
        0,
    ),
    ("a positive condition with no else is a deliberate limit, not a finding",
     TOLERANT_POSITIVE, 0),
    (
        "a positive condition WITH an else is caught",
        """
#[test]
fn t() {
    let entry = tmp.path().join("build/posix-zenoh/native_entry");
    if entry.is_dir() {
        assert!(entry.join("Cargo.toml").is_file());
    } else {
        eprintln!("not generated");
    }
}
""",
        1,
    ),
    (
        "an ASSERTION about the directory's existence passes",
        """
#[test]
fn t() {
    let entry = tmp.path().join("build/posix-zenoh/native_entry");
    assert!(entry.is_dir(), "the entry directory must exist");
    let _ = entry.join("Cargo.toml");
}
""",
        0,
    ),
    (
        "a negated assertion about ABSENCE passes (codegen_system_basic's shape)",
        """
#[test]
fn t() {
    let bake = dir.join("build/demo_bringup/nros-system");
    assert!(!bake.join("system_main.c").exists());
    assert!(!bake.exists());
}
""",
        0,
    ),
    (
        "a directory-WALK filter passes — the loop variable is not a build literal",
        """
#[test]
fn t() {
    let root = tmp.path().join("build/posix-zenoh");
    for e in fs::read_dir(&root).unwrap() {
        let p = e.unwrap().path();
        if !p.is_dir() {
            continue;
        }
        assert!(p.join("Cargo.toml").is_file());
    }
}
""",
        0,
    ),
    (
        "a SOURCE-tree fixture precondition passes — no generated-output root",
        """
#[test]
fn t() {
    let bins = root.join("packages/testing/nros-tests/bins");
    if !bins.is_dir() {
        nros_tests::skip!("bins dir missing");
    }
    let _ = bins.join("x");
}
""",
        0,
    ),
    (
        "`exists()` on a generated directory is the same rule",
        """
#[test]
fn t() {
    let entry = tmp.path().join("build/posix-zenoh/native_entry");
    if !entry.exists() {
        return;
    }
    let _ = entry.join("Cargo.toml");
}
""",
        1,
    ),
    (
        "`try_exists()` too",
        """
#[test]
fn t() {
    let entry = tmp.path().join("build/posix-zenoh/native_entry");
    if !entry.try_exists() {
        return;
    }
    let _ = entry.join("Cargo.toml");
}
""",
        1,
    ),
    (
        "an extensionless BINARY under a build root passes (narrowing 3)",
        """
fn agent_binary() -> Option<PathBuf> {
    let p = project_root().join("build/xrce-agent/MicroXRCEAgent");
    if !p.exists() {
        return None;
    }
    Some(p)
}
""",
        0,
    ),
    (
        "...and `is_dir()` on it is still caught — nobody asks that of a binary",
        """
fn agent_binary() -> Option<PathBuf> {
    let p = project_root().join("build/xrce-agent/MicroXRCEAgent");
    if !p.is_dir() {
        return None;
    }
    Some(p)
}
""",
        1,
    ),
    (
        "a FILE path under a build root is out of scope",
        """
#[test]
fn t() {
    let plan = tmp.path().join("build/nros/nros-plan.json");
    if !plan.exists() {
        return;
    }
    let _ = plan.join("x");
}
""",
        0,
    ),
    (
        "the guard in a plain HELPER fn is caught — this is why the rule is not a\n"
        "        case inside check-no-vacuous-tests, whose unit is a #[test] body",
        """
fn boot_and_connect(entry: &str) {
    let dir = project_root().join("build/cargo-fixtures/freertos");
    if !dir.is_dir() {
        nros_tests::skip!("fixture missing");
    }
    let _ = dir.join("generated");
}
""",
        1,
    ),
    (
        "a second hop through `join` is tracked (out_root -> metadata)",
        """
#[test]
fn t() {
    let out_root = root.join("build/self_bringup/nros");
    let meta = out_root.join("metadata");
    if !meta.is_dir() {
        return;
    }
    let _ = meta.join("x");
}
""",
        1,
    ),
    (
        "negation through `&&` is seen",
        """
#[test]
fn t() {
    let entry = tmp.path().join("build/posix-zenoh/native_entry");
    if ready && !entry.is_dir() {
        return;
    }
    let _ = entry.join("Cargo.toml");
}
""",
        1,
    ),
    (
        "a `while` condition counts too",
        """
fn wait() {
    let entry = tmp.path().join("build/posix-zenoh/native_entry");
    while !entry.is_dir() {
        sleep();
    }
    let _ = entry.join("Cargo.toml");
}
""",
        1,
    ),
    (
        "`if let` with no probe is not mis-parsed into a finding",
        """
#[test]
fn t() {
    let entry = tmp.path().join("build/posix-zenoh/native_entry");
    if let Some(x) = entry.join("Cargo.toml").to_str() {
        assert!(!x.is_empty());
    }
    assert!(entry.is_dir());
}
""",
        0,
    ),
    (
        "a probe on a generated dir OUTSIDE any condition is not a guard",
        """
#[test]
fn t() {
    let entry = tmp.path().join("build/posix-zenoh/native_entry");
    let generated = entry.is_dir();
    assert!(generated, "the entry was not generated");
    let _ = entry.join("Cargo.toml");
}
""",
        0,
    ),
    (
        "a file with no generated-output path is out of scope",
        "#[test]\nfn t() {\n    assert!(true);\n}\n",
        0,
    ),
]


def self_test(verbose: bool = False) -> int:
    ok = fail = 0
    for label, src, expected in SELF_TESTS:
        got = scan_source("case.rs", src)
        if len(got) == expected:
            ok += 1
            if verbose:
                print(f"  [OK]   {label}")
        else:
            fail += 1
            print(f"  [FAIL] {label}", file=sys.stderr)
            print(f"         expected {expected} finding(s), got {len(got)}", file=sys.stderr)
            for g in got:
                print(f"           {g}", file=sys.stderr)

    # The corpus must be non-empty: "OK (0 files)" is what a gate that has
    # stopped covering anything prints, and it is indistinguishable from a real
    # green without a floor (check-test-precondition-guards' argument).
    corpus = tracked_test_files()
    if len(corpus) >= 100 and any(
        not c.startswith("packages/testing/") for c in corpus
    ):
        ok += 1
        if verbose:
            print(f"  [OK]   the corpus reaches {len(corpus)} test targets beyond nros-tests")
    else:
        fail += 1
        print(
            f"  [FAIL] the corpus collapsed to {len(corpus)} file(s) — the glob no "
            "longer reaches the tree",
            file=sys.stderr,
        )

    if verbose:
        print(f"\n{ok} passed, {fail} failed")
    if fail:
        print("check-test-generated-dir-guards self-test: FAILED", file=sys.stderr)
        raise SystemExit(1)
    return 0


# ---------------------------------------------------------------- main


ADVICE = """
A directory is a CONTAINER: its existence is a claim about everything that
creates containers there, and that set grows. `build/<coord>/<entry>/` had one
producer when issue 1411's guard was written and two after RFC-0098 D1 —
`cmd::build` writes the image's `nros-cargo.toml` into it unconditionally, on
the model-resolution FAILURE path as well — so the guard could no longer be
false, its documented fallback became unreachable, and the read below it panicked
on a file that was never generated.

Name the artifact:

    let manifest = entry.join("Cargo.toml");
    assert!(
        manifest.is_file(),
        "no entry package was generated at {} — the settings file may be there \\
         (it is written either way), but `Cargo.toml` is the artifact this test \\
         is about",
        entry.display()
    );

An existence ASSERTION is fine and is not what this gate reads. If the branch
only creates the directory, drop the probe — `create_dir_all` is idempotent.
"""


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--selftest", "--self-test", dest="selftest", action="store_true")
    ap.add_argument(
        "--sweep",
        action="store_true",
        help="report every generated-output directory probe, classified (never fails)",
    )
    args = ap.parse_args()
    if args.selftest:
        return self_test(verbose=True)
    # On the NORMAL path, every time — a negative control nobody runs decays
    # into a comment (`check-gate-selftests`).
    self_test()

    files = tracked_test_files()
    if args.sweep:
        return sweep(files)

    bad = scan(files)
    if bad:
        print("check-test-generated-dir-guards: FAILED", file=sys.stderr)
        for b in bad:
            print(f"  {b}", file=sys.stderr)
        print(ADVICE, file=sys.stderr)
        return 1
    print(
        f"check-test-generated-dir-guards: OK ({len(files)} test targets; no "
        "generated-output directory decides whether a test runs)"
    )
    return 0


def sweep(files: list[str]) -> int:
    """The measurement behind the rule's width, re-runnable."""
    cond = asserted = other = 0
    for rel in files:
        p = REPO / rel
        if not p.is_file():
            continue
        src = blank_comments(p.read_text(encoding="utf-8", errors="replace"))
        names = generated_dir_names(src)
        if not names:
            continue
        asserts = assertion_spans(src)
        conds = condition_spans(src)
        for name in sorted(names):
            pat = re.compile(
                r"\b" + re.escape(name) + r"\b(?:\s*\.\w+\s*\([^()]*\))*?\s*\.("
                + "|".join(PROBES) + r")\s*\(\s*\)"
            )
            for m in pat.finditer(src):
                line = src[:m.start()].count("\n") + 1
                if in_span(m.start(), asserts):
                    asserted += 1
                    kind = "assertion"
                elif any(s <= m.start() < e for s, e, _ in conds):
                    cond += 1
                    kind = "CONDITION"
                else:
                    other += 1
                    kind = "plain"
                print(f"  {kind:9} {rel}:{line}  {name} ({names[name]}) .{m.group(1)}()")
    print(
        f"\nsweep: {cond} condition(s), {asserted} assertion(s), {other} other, "
        f"over {len(files)} test targets"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
