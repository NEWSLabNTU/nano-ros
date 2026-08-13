#!/usr/bin/env python3
"""phase-340 P2 — no cargo build may write an `examples/**/target/` dir.

# What this protects

`examples/**/target-*/` (with a dash) is covered by a repo-root `.gitignore`
pattern; a plain `examples/**/target/` is NOT. It is ignored only by 391
per-leaf `.gitignore` files, and phase-340 item 7 / P4 exists to delete those.
They can only go once nothing writes a leaf `target/` — so this gate is the
invariant P4 stands on, and it is stated as a property of the BUILD, not of the
ignore file.

# The defect it was written for

`just freertos build-examples` prebuilt six Entry pkgs with a bare
`cd <leaf> && cargo build`. A build with no `[[fixture]]` row has no
coordinate, so it has no shared cargo group (RFC-0070 R2 names a cache
`<kind>/<coordinate>`) — which is exactly why phase-340 B2/B3's migration
never reached it. Measured on an already-migrated platform: the shared group
`build/cargo-fixtures/freertos` was written at 01:53 and
`examples/qemu-arm-freertos/rust/talker/target/nros-minsizerel` at 01:55, from
one `lane=all` run. Every gate was green while that happened, because no gate
asked this question.

# The rule

A `cargo build` / `cargo rustc` whose working directory is a leaf under
`examples/` MUST pass a `--target-dir`. It may be a literal (`target-zenoh`,
an authored per-variant dir the group machinery strips and replaces) or a
resolver-derived flag variable (`$tdir_flag`, `$qemu_tdir_flag`, …) — what is
banned is the DEFAULT, which is the leaf's own `target/`.

Derived is better than literal, and a flag variable is only accepted when the
file also assigns it from `nros_fixture_target_dir_flag`; a variable that
merely LOOKS like the flag is not a way past this gate.

# Detection

Shell `cd` is stateful, and the sites use all three idioms:

    ( cd "$dir" && cargo build … )        # same line
    cd "examples/x/y" && \\                # line continuation
        cargo build …
    cd "examples/x/y"                     # bare cd, cargo two lines later
    cargo build …

so continuations are joined and a per-block `cwd is an example leaf` flag is
carried forward. The flag resets at each `just` recipe head, which is the only
place a justfile's cwd is guaranteed back at the repo root.

Variables are tracked when assigned a value containing `examples/` — that
over-approximates (a var assigned an example path in one recipe is treated as
one everywhere later in the file), which can only make the gate STRICTER, never
looser.

Run: python3 scripts/check-example-leaf-target-dirs.py
"""

import os
import re
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# Sites that still write a per-leaf dir and are NOT this gate's business,
# each with the reason it is out of scope. Keyed on path so a file cannot be
# exempted by accident: the entry must name the file AND a substring of the
# offending line.
#
# Nothing is allowlisted today. The residual second-path sites found by
# phase-340 P2's sweep all sit OUTSIDE `examples/` (`packages/testing/**`
# leaves) or already pass a `--target-dir`, so they are outside this rule by
# construction rather than by exemption — see issue 0488, which owns them.
ALLOWLIST: "list[tuple[str, str]]" = []

SCAN_DIRS = ("just", "scripts")
SCAN_SUFFIXES = (".just", ".sh")

CARGO_BUILD = re.compile(r"\bcargo\b[^\n]*?\b(build|rustc)\b")
TARGET_DIR_FLAG = re.compile(r"--target-dir\b")
# A resolver-derived flag variable. Deliberately narrow: the name must end in
# `tdir_flag`, and `assigns_derived_flag` below still requires the file to
# derive it from the shared resolver.
FLAG_VAR = re.compile(r"\$\{?(?:[A-Za-z_][A-Za-z0-9_]*_)?tdir_flag\b")
DERIVES_FLAG = re.compile(r"nros_fixture_target_dir_flag\b")

# `cd` to a literal path under examples/, quoted or not.
CD_LITERAL_EXAMPLE = re.compile(r"\bcd\s+[\"']?(?:\./)?examples/")
# `cd` to a variable.
CD_VAR = re.compile(r"\bcd\s+[\"']?\$\{?([A-Za-z_][A-Za-z0-9_]*)\}?")
# Any `cd` at all (used to invalidate a carried flag when we cd somewhere else).
CD_ANY = re.compile(r"\bcd\s+\S")
# Assignment whose value mentions examples/ — VAR=…examples/… or VAR := …
ASSIGN_EXAMPLE = re.compile(r"([A-Za-z_][A-Za-z0-9_]*)\s*:?=\s*[^\n]*examples/")
# A `just` recipe head: column 0, `name[ params]:`.
RECIPE_HEAD = re.compile(r"^[a-zA-Z_][a-zA-Z0-9_-]*(\s[^\n]*)?:")


def join_continuations(text):
    """Join backslash-continued lines, keeping the FIRST line's number."""
    out = []
    buf = None
    start = 0
    for i, raw in enumerate(text.splitlines(), start=1):
        line = raw
        if buf is None:
            buf, start = line, i
        else:
            buf += " " + line.strip()
        if buf.rstrip().endswith("\\"):
            buf = buf.rstrip()[:-1]
            continue
        out.append((start, buf))
        buf = None
    if buf is not None:
        out.append((start, buf))
    return out


def scan_text(text, *, derives_flag):
    """Return [(lineno, line, reason)] for every violation in one file body.

    Split out from the filesystem walk so the self-test can drive it directly.
    """
    violations = []
    example_vars = set()
    cwd_is_example_leaf = False
    for lineno, line in join_continuations(text):
        stripped = line.strip()
        if stripped.startswith("#"):
            continue
        if RECIPE_HEAD.match(line):
            cwd_is_example_leaf = False

        m = ASSIGN_EXAMPLE.search(line)
        if m:
            example_vars.add(m.group(1))

        # Does THIS line put us in an example leaf?
        here = bool(CD_LITERAL_EXAMPLE.search(line))
        if not here:
            mv = CD_VAR.search(line)
            if mv and mv.group(1) in example_vars:
                here = True

        in_leaf = here or cwd_is_example_leaf
        if CARGO_BUILD.search(line) and in_leaf:
            has_literal = TARGET_DIR_FLAG.search(line)
            has_var = FLAG_VAR.search(line)
            if not has_literal and not (has_var and derives_flag):
                if has_var and not derives_flag:
                    reason = (
                        "passes a `*tdir_flag` variable, but this file never "
                        "derives one from `nros_fixture_target_dir_flag`"
                    )
                else:
                    reason = "no --target-dir: writes the leaf's own `target/`"
                violations.append((lineno, stripped, reason))

        # Carry the cwd forward only for a BARE `cd` (not `( cd … && … )`,
        # which is scoped to its subshell, and not `cd … && …`, whose effect
        # ends with the command list on that line).
        if here and "&&" not in line and not stripped.startswith("("):
            cwd_is_example_leaf = True
        elif CD_ANY.search(line) and not here and "&&" not in line:
            cwd_is_example_leaf = False
    return violations


def allowlisted(relpath, line):
    return any(f == relpath and frag in line for f, frag in ALLOWLIST)


def self_test():
    """Both directions, on synthetic bodies — a gate whose classifier silently
    stopped classifying looks exactly like a passing gate (issue 0485)."""
    must_flag = [
        ('( cd "examples/a/b" && cargo build --quiet )', "same-line cd &&"),
        ('D="examples/a/b"\n( cd "$D" && cargo build )', "cd through a variable"),
        ('cd "examples/a/b"\ncargo build $args', "bare cd, cargo later"),
        ('cd "examples/a/b" && \\\n    cargo build $args', "line continuation"),
    ]
    must_pass = [
        ('( cd "examples/a/b" && cargo build --target-dir target-zenoh )', "literal dir"),
        ('cd packages/testing/x\ncargo build', "not under examples/"),
        ('( cd "examples/a/b" && cargo build $tdir_flag )', "derived flag"),
        (
            'cd "examples/a/b"\nrecipe-head:\n    cargo build',
            "cwd resets at a recipe head",
        ),
    ]
    bad = []
    for body, label in must_flag:
        if not scan_text(body, derives_flag=True):
            bad.append(f"self-test: expected a violation for {label!r}")
    for body, label in must_pass:
        v = scan_text(body, derives_flag=True)
        if v:
            bad.append(f"self-test: unexpected violation for {label!r}: {v}")
    # A `*tdir_flag` variable is NOT a free pass when nothing derives it.
    if not scan_text('( cd "examples/a/b" && cargo build $tdir_flag )', derives_flag=False):
        bad.append("self-test: undeclared tdir_flag variable must not pass")
    if bad:
        for b in bad:
            sys.stderr.write(b + "\n")
        sys.exit(2)


def main():
    self_test()
    failures = []
    for d in SCAN_DIRS:
        for dirpath, dirnames, filenames in os.walk(os.path.join(ROOT, d)):
            dirnames[:] = [x for x in dirnames if x not in (".git", "third-party")]
            for fn in sorted(filenames):
                if not fn.endswith(SCAN_SUFFIXES):
                    continue
                path = os.path.join(dirpath, fn)
                rel = os.path.relpath(path, ROOT)
                try:
                    text = open(path, encoding="utf-8").read()
                except (OSError, UnicodeDecodeError):
                    continue
                derives = bool(DERIVES_FLAG.search(text))
                for lineno, line, reason in scan_text(text, derives_flag=derives):
                    if allowlisted(rel, line):
                        continue
                    failures.append((rel, lineno, line, reason))

    if failures:
        sys.stderr.write(
            "check-example-leaf-target-dirs: a cargo build inside an "
            "`examples/` leaf must pass --target-dir.\n\n"
        )
        for rel, lineno, line, reason in failures:
            sys.stderr.write(f"  {rel}:{lineno}: {reason}\n      {line}\n")
        sys.stderr.write(
            "\n  Why: `examples/**/target-*/` is globally ignored but a plain\n"
            "  `target/` is not — it survives only through 391 per-leaf\n"
            "  .gitignore files that phase-340 P4 deletes. A build with no\n"
            "  manifest row also has no coordinate and therefore no shared\n"
            "  cargo group, which is how the freertos second path survived\n"
            "  B2/B3 (phase-340 P2).\n"
            "  Fix: give the build a `[[fixture]]` row (preferred — it gets a\n"
            "  coordinate and a group), or derive the dir from\n"
            "  `nros_fixture_target_dir_flag` in\n"
            "  scripts/build/fixtures-target-dir.sh. Never a new literal.\n"
        )
        sys.exit(1)
    print("check-example-leaf-target-dirs: OK")


if __name__ == "__main__":
    main()
