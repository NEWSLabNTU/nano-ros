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
# issue 0635 — the ROOT justfile is a scan target too. It was not, so
# `rust-rtos-link-check`'s two `cd <leaf> && cargo build` calls were as invisible
# to this gate as `check-examples` was: a recipe living one directory up from the
# `just/*.just` modules, doing the exact thing the gate exists to forbid. Both
# halves of #0624 in one file, found the same way — by the artifact, one run
# late. Extra files, not a directory, since the repo root holds much else.
SCAN_FILES = ("justfile",)

# Every cargo verb that COMPILES, not just the two that say "build".
#
# issue 0635: `just check`'s example walk runs `cargo clippy`, which writes a
# target dir exactly like `cargo build` does — and the verb list did not name
# it, so the walk was invisible to this scan while it recreated 39 leaf
# `target/debug` trees on every run. `check`/`test`/`bench`/`doc` are here for
# the same reason before someone reaches for them: what matters is that cargo
# writes artifacts, not what the subcommand is called.
# The `(?<![-\w])` matters: `cargo fmt --check` contains the word `check` and
# compiles nothing, so a verb must not be reachable through a flag spelling.
CARGO_BUILD = re.compile(
    r"\bcargo\b[^\n]*?(?<![-\w])(build|rustc|clippy|check|test|bench|doc)\b"
)
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
# issue 0635 — the example walk reaches its leaves through a FILE, so no
# assignment ever holds an `examples/` path:
#
#     git ls-files 'examples/**/Cargo.toml' … > "$toml_list"   # list of leaves
#     while read -r toml; do … done < "$toml_list"             # one leaf
#     dir="$(dirname "$toml")"                                 # its directory
#
# Three narrow rules follow that chain. Each only ADDS marked variables, so
# like the assignment rule above they can make the gate stricter, never looser.
REDIRECT_EXAMPLE = re.compile(r"examples/[^\n]*>\s*[\"']?\$\{?([A-Za-z_][A-Za-z0-9_]*)")
READ_VAR = re.compile(r"\bread\s+(?:-r\s+)?([A-Za-z_][A-Za-z0-9_]*)")
REDIRECT_FROM_VAR = re.compile(r"<\s*[\"']?\$\{?([A-Za-z_][A-Za-z0-9_]*)")
ASSIGN_DIRNAME = re.compile(
    r"([A-Za-z_][A-Za-z0-9_]*)\s*=\s*[\"']?\$\((?:dirname|cd)\s+[\"']?\$\{?([A-Za-z_][A-Za-z0-9_]*)"
)
# A command line EMITTED as data (a make unit, a `parallel` input line). The
# scan cannot follow `%s`, so the rule is stated on the emitted text itself:
# something that cds and then runs cargo must carry a `--target-dir`, wherever
# it eventually runs. `just/px4.just` already writes one that does.
EMITTED_CD_CARGO = re.compile(
    r"\b(?:printf|echo)\b[^\n]*\bcd\s[^\n]*\bcargo\b[^\n]*?"
    r"\b(?:build|rustc|clippy|check|test|bench|doc)\b"
)
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
    example_lists = set()
    cwd_is_example_leaf = False
    lines = join_continuations(text)

    # issue 0635 — mark variables in a PRE-PASS, not as the walk goes.
    #
    # The chain runs backwards in the source: `while read -r toml` comes before
    # the `done < "$toml_list"` that says which file it reads. A single forward
    # pass sees the read first, when nothing is known yet, and marks nothing —
    # which is how the walk stayed invisible with all three rules present.
    # Three rounds reach the fixpoint for a chain this long; a fourth changes
    # nothing (the rules only add).
    for _ in range(3):
        for _lineno, line in lines:
            if line.strip().startswith("#"):
                continue
            for rx, group in ((ASSIGN_EXAMPLE, 1), (REDIRECT_EXAMPLE, 1)):
                m = rx.search(line)
                if m:
                    example_vars.add(m.group(group))
            m = REDIRECT_FROM_VAR.search(line)
            if m and m.group(1) in example_vars:
                example_lists.add(m.group(1))
            m = READ_VAR.search(line)
            if m and example_lists:
                example_vars.add(m.group(1))
            m = ASSIGN_DIRNAME.search(line)
            if m and m.group(2) in example_vars:
                example_vars.add(m.group(1))

    for lineno, line in lines:
        stripped = line.strip()
        if stripped.startswith("#"):
            continue
        if RECIPE_HEAD.match(line):
            cwd_is_example_leaf = False

        if EMITTED_CD_CARGO.search(line) and not TARGET_DIR_FLAG.search(line):
            violations.append(
                (
                    lineno,
                    stripped,
                    "emits a `cd … && cargo …` command with no --target-dir: "
                    "wherever it runs, cargo writes that directory's `target/`",
                )
            )

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
        # issue 0635 — the two idioms `just check`'s example walk uses.
        ('( cd "examples/a/b" && cargo clippy --quiet )', "clippy is a build too"),
        (
            'git ls-files "examples/**/Cargo.toml" > "$list"\n'
            'while read -r toml; do\n'
            'd="$(dirname "$toml")"\n'
            '( cd "$d" && cargo clippy --quiet )\n'
            'done < "$list"',
            "leaf reached through a list file",
        ),
        (
            "printf 'cd %s && cargo clippy --quiet %s\\n' \"$d\" \"$f\"",
            "an EMITTED cd+cargo command",
        ),
    ]
    must_pass = [
        ('( cd "examples/a/b" && cargo build --target-dir target-zenoh )', "literal dir"),
        ('cd packages/testing/x\ncargo build', "not under examples/"),
        ('( cd "examples/a/b" && cargo build $tdir_flag )', "derived flag"),
        (
            'cd "examples/a/b"\nrecipe-head:\n    cargo build',
            "cwd resets at a recipe head",
        ),
        (
            "printf 'cd %s && cargo build --target-dir %s\\n' \"$d\" \"$t\"",
            "an emitted command that carries the flag",
        ),
        ('( cd "examples/a/b" && cargo fmt --check -p x )', "fmt compiles nothing"),
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


def existing_leaf_target_dirs():
    """Plain `examples/**/target/` directories that EXIST on disk.

    The scan above is a static reading of build COMMANDS: it proves no
    recipe/script this gate can parse writes a leaf `target/`. That is a
    property of the sources, and it stayed green while 65 such directories
    (119.9 GiB) sat in the tree — residue from before phase-340 P2, which
    nothing rebuilt and nothing noticed. Measured 2026-08-16, issue 0200.

    An existing dir is one of two things, and both want reporting:

      * residue — delete it, and any disk estimate taken from this tree stops
        being inflated by it (a runner sized from an accumulated tree is sized
        from residue, which is what issue 0200's ≥200 GiB rested on);
      * a LIVE writer this gate cannot see — a Makefile, a python driver, an
        IDE, or an idiom the regexes miss. That is the issue-0196 shape: a gate
        whose coverage is narrower than the rule it enforces. The static scan
        cannot find it; the artifact can.

    So the check is on the artifact, which is the observable the rule is
    actually about.
    """
    found = []
    root_examples = os.path.join(ROOT, "examples")
    for dirpath, dirnames, _ in os.walk(root_examples):
        dirnames[:] = [x for x in dirnames if x not in (".git", "third-party")]
        if os.path.basename(dirpath) != "target":
            continue
        # Do not descend into it; one report per directory.
        dirnames[:] = []
        rel = os.path.relpath(dirpath, ROOT)
        parts = rel.split(os.sep)
        # Only a LEAF's own default dir counts. A `target` nested inside a cmake
        # build tree — `…/build-cyclonedds/corrosion/required_libs/target` — is
        # Corrosion's internal scratch, already covered by the `build*` ignore
        # and NOT the thing phase-340 P2 is about. Scoping this down was forced
        # by the first run, which reported 40+ of them.
        if any(x == "build" or x.startswith("build-") for x in parts[:-1]):
            continue
        # And it must actually be a crate's target dir.
        if not os.path.isfile(os.path.join(os.path.dirname(dirpath), "Cargo.toml")):
            continue
        found.append(rel)
    return sorted(found)


def main():
    self_test()
    failures = []
    scanned = [os.path.join(ROOT, f) for f in SCAN_FILES if os.path.isfile(os.path.join(ROOT, f))]
    for d in SCAN_DIRS:
        for dirpath, dirnames, filenames in os.walk(os.path.join(ROOT, d)):
            dirnames[:] = [x for x in dirnames if x not in (".git", "third-party")]
            for fn in sorted(filenames):
                if not fn.endswith(SCAN_SUFFIXES):
                    continue
                scanned.append(os.path.join(dirpath, fn))
    for path in scanned:
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

    # The other half of the same rule: no such directory may EXIST.
    stale = existing_leaf_target_dirs()
    if stale:
        sys.stderr.write(
            "check-example-leaf-target-dirs: no build writes a leaf `target/`, "
            "but some EXIST.\n\n"
        )
        for rel in stale[:20]:
            sys.stderr.write(f"  {rel}\n")
        if len(stale) > 20:
            sys.stderr.write(f"  … and {len(stale) - 20} more ({len(stale)} total)\n")
        sys.stderr.write(
            "\n  Each is EITHER residue from before phase-340 P2 — delete it, and\n"
            "  stop every disk estimate from being inflated by it (65 such dirs\n"
            "  held 119.9 GiB on the maintainer host, issue 0200) — OR evidence of\n"
            "  a writer this gate cannot see: a Makefile, a python driver, an IDE,\n"
            "  or an idiom the scan above misses. The scan reads COMMANDS; only the\n"
            "  artifact can catch a writer it does not parse (issue 0196).\n"
            "  Fix: rm -rf the directories, then re-run a build. If one comes back,\n"
            "  it is the second case and the writer needs finding.\n"
        )
        sys.exit(1)

    print(f"check-example-leaf-target-dirs: OK (no writer, and none on disk)")


if __name__ == "__main__":
    main()
