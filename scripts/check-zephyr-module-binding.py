#!/usr/bin/env python3
"""Every `west build` that CONFIGURES a nano-ros image must name THIS checkout's
`nros` module — issue 1379.

THE RULE
--------

A Zephyr image gets the `nros` module from west's project list. A workspace
provisioned before phase-449 W1 binds that list to whichever checkout ran
`just zephyr setup`, and since phase-440 W4 the default workspace is
`$NROS_STORE/workspaces/zephyr/<version>` — ONE directory shared by every
checkout on the host. A build launched from checkout B therefore compiles
checkout A's `zephyr/`, platform sources and public headers next to B's entry
code, and nothing says so. Measured in the build dir issue 1379 was filed from:
every cache entry the invoking build set named the invoking checkout, and
`NROS_REPO_DIR` — the one derived from the MODULE — named another clone.

So a configuring `west build` has to pass `-DZEPHYR_EXTRA_MODULES=<checkout>`,
and it has to take that value from `scripts/lib/zephyr-module.sh` rather than
spelling it again.

WHY A GATE
----------

phase-449 W1 fixed this for three builders. There were seven, and the one issue
1379 reports (`just zephyr build-one`) was not among the three. That is exactly
CLAUDE.md's "fix the CLASS, not the reported site" — a rule with no gate gets
re-broken by the next call site, and this one's failure mode is a WRONG IMAGE
that still links, not a build error anybody would notice.

WHAT COUNTS
-----------

* A `west build` that names a SOURCE DIRECTORY configures. It must carry the
  module.
* A `west build -t <target>` re-enters an existing build directory (`-t run`,
  `-t menuconfig`). It configures nothing and needs nothing.
* `EXEMPT` lists the invocations that genuinely do not consume the module, each
  with the reason. An exemption is a claim about the application being built,
  so it names the file and the marker text, never a bare path.

A NOTE FOR WHOEVER EXTENDS THIS
-------------------------------

Two defects were found in this gate before it merged, and they are the same
defect: an AUTHORED PREDICATE whose reach is not the rule it enforces (issue
0196's shape, which CLAUDE.md indexes and which the 2026-07-28 audit found in
four gates at once). Neither was visible from reading the predicate, and both
printed a confident OK.

  * The file set was an `os.walk` that pruned directories named `build` — so
    it never saw `scripts/build/`, which holds the tree's busiest west builder.
    Reach NARROWER than the rule. Fixed by going through the git index
    (`scripts/lib/tracked.py`), which `check-no-tracked-file-find` requires
    anyway.
  * The array exemption matched any `${x[@]}` anywhere in the command, so it
    fired on the command NAME `"${west_cmd[@]}"` and stopped noticing a
    missing flag in `just zephyr build-one` — the recipe issue 1379 was
    reported from. Reach WIDER than the rule.

So: when you widen or narrow anything here, write the mutant into `selftest()`
FIRST and watch it fail. Every row in there is a real miss, not an imagined
one, and the two above are rows 9-13.

Run: python3 scripts/check-zephyr-module-binding.py [--list] [--selftest]
"""

from __future__ import annotations

import os
import re
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "lib"))
from tracked import tracked  # noqa: E402  (after the sys.path insert, by design)

HELPER = os.path.join("scripts", "lib", "zephyr-module.sh")

# Where an invocation can live. Prose (docs/, book/) is deliberately out: a
# command in a document is an illustration, and gating it would make the gate a
# style checker for English.
SEARCH_DIRS = ("just", "scripts", "tests", "ci")
SEARCH_SUFFIXES = (".just", ".sh", ".bash")

# The west invocation, in every spelling this tree uses: `west build`,
# `"${west_cmd[@]}" build`, `… -m west build`.
WEST_BUILD = re.compile(r"(?:\bwest\b|west_cmd\[@\]\}\"?)\s+build\b")

# ...and the spelling this gate could not see until issue 1458: west invoked
# with its WHOLE argv in an array — `env "${tc_env[@]}" west "${args[@]}"`,
# which is `scripts/build/west-fixtures.sh`, the builder of all five west
# fixtures and therefore of tier 2's entire west cover.
#
# `build` is not on the line at all, so `WEST_BUILD` never matched and the file
# was never harvested: the gate printed OK over a builder that named no module,
# for as long as the rule has existed. Reach NARROWER than the rule — the third
# instance of issue 0196's shape in this one gate, and the docstring above
# predicted it in as many words.
#
# An argv-array invocation cannot be read from the line, so it is treated as a
# configuring build and must earn the assembled-flags exemption like any other:
# the FILE has to name the flag and call the helper on one line. That is
# strictly what `zephyr-fixture-run-one.sh` already demonstrates.
WEST_ARGV = re.compile(r"(?:\bwest\b|west_cmd\[@\]\}\"?)\s+\"?\$\{(\w+)\[@\]\}")
WEST_INVOCATION = re.compile(
    f"(?:{WEST_BUILD.pattern})|(?:{WEST_ARGV.pattern})"
)

# A west invocation only counts in COMMAND POSITION. The tree mentions
# `west build` ~30 times in comments, `echo`-ed remedies and help text, and a
# gate that cannot tell prose from a command is a gate about prose: it would
# demand a `-D` flag inside a sentence, and every author would learn to reword
# the sentence. The prefix must therefore end at a shell command boundary.
# A BACKTICK is deliberately not a boundary here. Legacy `` `cmd` `` command
# substitution would qualify, and nothing in this tree spells west that way,
# while markdown-style backticks around `west build` in prose are everywhere —
# including inside the block comment at the head of
# `scripts/build/zephyr-fixture-run-one.sh`, which is neither `#`-prefixed nor
# printed and so is invisible to the other two filters.
# `env VAR=… [VAR=…] west …` is command position too. The prefix ends in a
# QUOTE there (`env "${tc_env[@]}" `), so none of the boundaries below reached
# it and `scripts/build/west-fixtures.sh` fell out of the harvest on this test
# as well as on `WEST_ARGV` — two independent reasons the busiest west builder
# in the tree was invisible, either of which alone kept it so.
COMMAND_POSITION = re.compile(
    r"(?:^\s*|[;&|(){}]\s*|\b(?:then|do|else|if|elif|while|until|time)\s+|"
    r"\$\(\s*|!\s*|-m\s+|\benv\s+(?:\S+\s+)*)$"
)
# `echo`/`printf`/`log_*` before the match means the invocation is TEXT being
# printed — a remedy the script is telling a human to run, in another tree.
PRINTING = re.compile(r"\b(?:echo|printf|cat|log_info|log_warn|log_error|die)\b")
# An ellipsis is how this tree writes "and the rest of the flags": documentation
# by construction, and never a command anyone runs.
ELLIPSIS = "..."

# The sanctioned ways to supply the module. All of them route through the
# helper; a hand-written `-DZEPHYR_EXTRA_MODULES=<something>` is a second
# spelling and fails, which is the point of having one.
SANCTIONED = ("nros_module_arg", "nros_zephyr_module_cmake_arg", "nros_zephyr_module_root")

# `-t <target>` — a rebuild inside an existing build dir, never a configure.
TARGET_RUN = re.compile(r"(?:^|\s)-t\s+\S")

# An invocation whose flags are ASSEMBLED elsewhere in the same file —
# `scripts/build/zephyr-fixture-run-one.sh` builds its `-D` list with
# `replace_or_append_arg` and expands it as `"${west_extra[@]}"`, so the
# sanctioned token is a hundred lines from the `west build` line.
#
# This exemption has to be narrow in TWO directions at once, because its first
# version was wide in both and the wideness was invisible:
#
#   * it matched ANY `${x[@]}` in the command, including the command NAME.
#     `just/zephyr-dev.just` invokes west as `"${west_cmd[@]}" build …` (the
#     4.4 line runs west through a venv interpreter), so the exemption fired
#     on the command word and the flag's presence stopped mattering — for the
#     exact recipe issue 1379 was reported from. Deleting `"$nros_module_arg"`
#     from that line left the gate printing OK. So a qualifying expansion must
#     appear AFTER the `build` sub-command, where an ARGUMENT lives.
#   * it accepted any sanctioned token ANYWHERE in the file, including the one
#     on a `nros_module_arg=…` line whose value the mutated command no longer
#     used. So the file must also demonstrably assemble the FLAG: one line
#     naming `ZEPHYR_EXTRA_MODULES` and calling the helper.
#
# Either half alone closes that mutant; both are kept because they fail for
# different reasons and a later edit is unlikely to defeat both at once.
ARRAY_EXPANSION = re.compile(r"\$\{\w+\[@\]\}")
# A line that builds the module flag itself — `replace_or_append_arg
# "-DZEPHYR_EXTRA_MODULES" "$(nros_zephyr_module_root …)"`. The flag NAME and
# the helper on one line is the evidence; either alone is not.
FLAG_NAME = "ZEPHYR_EXTRA_MODULES"

# file -> (marker substring, reason). The marker keeps the exemption pinned to
# the invocation it was written for: move the build and the exemption stops
# matching rather than silently covering a different one.
EXEMPT = {
    os.path.join("just", "zephyr-dev.just"): (
        "tests/zephyr-c-smoke",
        "the C-port smoke app is standalone: its CMakeLists pulls "
        "nros-platform-zephyr's two C files in by relative path and loads no "
        "nros module at all, so there is no module for a second checkout to "
        "supply (tests/zephyr-c-smoke/CMakeLists.txt).",
    ),
}

# A floor, not a count. The harvest is the gate's evidence, so a refactor that
# silently stops finding west invocations must fail rather than report OK over
# an empty set — `check-west-leaf-vocabulary`'s "harvested only N" shape.
MIN_INVOCATIONS = 6


def iter_files(root: str):
    """Through the git INDEX, never a walk (issue 0721 / `check-no-tracked-file-find`).

    `scripts/` and `tests/` both hold build output on a provisioned host, and a
    walk pays for descending it before the filter that discards it ever runs.
    """
    for path in tracked(*SEARCH_DIRS, repo=root):
        if path.name.endswith(SEARCH_SUFFIXES):
            yield os.path.relpath(path, root)


def logical_commands(text: str):
    """Yield (line_number, joined_command) for every west build invocation.

    Continuation lines (`\\` at end) are joined, because the tree writes these
    across four or five lines and the `-D` flags live on the last of them.
    """
    lines = text.splitlines()
    for i, line in enumerate(lines):
        m = WEST_INVOCATION.search(line)
        if not m:
            continue
        if line.lstrip().startswith("#"):
            continue
        prefix = line[: m.start()]
        if PRINTING.search(prefix):
            continue
        if not COMMAND_POSITION.search(prefix):
            continue
        if ELLIPSIS in line:
            continue
        joined = line
        j = i
        while joined.rstrip().endswith("\\") and j + 1 < len(lines):
            j += 1
            joined = joined.rstrip()[:-1] + " " + lines[j]
        yield i + 1, joined


def offenders(root: str):
    """(offences, harvested) — offences are (path, lineno, command)."""
    bad = []
    harvested = []
    for path in iter_files(root):
        full = os.path.join(root, path)
        try:
            text = open(full, encoding="utf-8").read()
        except (OSError, UnicodeDecodeError):
            continue
        for lineno, cmd in logical_commands(text):
            harvested.append((path, lineno, cmd))
            if not is_offence(cmd, path, text):
                continue
            bad.append((path, lineno, " ".join(cmd.split())))
    return bad, harvested


def carries_assembled_flags(cmd: str) -> bool:
    """Does this command expand an array in ARGUMENT position?

    Positionally, not by name: the array carrying the `-D` list is called
    `west_extra` here and would be called something else in the next file, so
    keying on the name would be a second authored list. What is structural is
    that the command NAME sits before the `build` sub-command and an ARGUMENT
    sits after it.
    """
    m = WEST_BUILD.search(cmd)
    if not m:
        return False
    return bool(ARRAY_EXPANSION.search(cmd[m.end():]))


def file_assembles_module_flag(file_text: str) -> bool:
    """One line naming the flag AND calling the helper — the flag is built here."""
    return any(
        FLAG_NAME in line and any(t in line for t in SANCTIONED)
        for line in file_text.splitlines()
    )


def argv_array_carries_module(array: str, file_text: str) -> bool:
    """Does the argv array this west call expands get the module flag PUT IN IT?

    The evidence the `-- "$flag"` shape gives on one line has to come from two
    here, so it is keyed on the ARRAY NAME the invocation actually expands: a
    line that assigns or appends to that name AND calls the helper.

    Not `file_assembles_module_flag` (FLAG_NAME + helper on one line), which is
    the right evidence when the caller spells the `-D` itself. A caller that
    goes through `nros_zephyr_module_cmake_arg` never writes the flag's NAME —
    that is the whole point of the helper — so demanding the literal here would
    demand exactly the second spelling this gate exists to prevent.

    Keyed on the name rather than on "some array somewhere" for issue 1379's
    reason: the helper called on a line whose value the command never uses is
    what let the first version of this gate pass the recipe it was written for.
    """
    assigns = re.compile(rf"\b{re.escape(array)}\+?=\(")
    return any(
        assigns.search(line) and any(t in line for t in SANCTIONED)
        for line in file_text.splitlines()
    )


def is_offence(cmd: str, path: str, file_text: str) -> bool:
    if TARGET_RUN.search(cmd):
        return False
    if any(token in cmd for token in SANCTIONED):
        return False
    argv = WEST_ARGV.search(cmd)
    if argv:
        # `west "${args[@]}"` — nothing about this call is readable from the
        # line, so the only safe reading is "it configures", and the file must
        # show the module flag entering that very array.
        return not argv_array_carries_module(argv.group(1), file_text)
    if carries_assembled_flags(cmd) and file_assembles_module_flag(file_text):
        return False
    exempt = EXEMPT.get(path)
    if exempt and exempt[0] in cmd:
        return False
    return True


def selftest(root: str, quiet: bool = False) -> int:
    """Mutants that must be caught. Each is the shape a real regression takes.

    Run on the NORMAL path (phase-395 / `check-gate-selftests`): a negative
    control nobody runs decays into a comment, and this gate's whole value is
    that it can tell a configuring `west build` from prose describing one — a
    distinction made by three regexes that a refactor can quietly widen into
    "everything passes".
    """
    fails = 0

    def expect(name, text, path, should_fail):
        nonlocal fails
        found = [
            (lineno, cmd)
            for lineno, cmd in logical_commands(text)
            if is_offence(cmd, path, text)
        ]
        got = bool(found)
        if got != should_fail:
            print(
                f"  SELFTEST FAIL: {name} — expected "
                f"{'an offence' if should_fail else 'no offence'}, got "
                f"{'an offence' if got else 'none'}",
                file=sys.stderr,
            )
            fails += 1
        elif not quiet:
            print(f"  ok: {name}")

    expect(
        "a bare configuring build is an offence",
        '    west build -b native_sim "$src" -- -DCONF_FILE="$conf"\n',
        "just/x.just",
        True,
    )
    expect(
        "the helper's variable satisfies it",
        '    west build -b native_sim "$src" -- "$nros_module_arg"\n',
        "just/x.just",
        False,
    )
    expect(
        "the flag on a CONTINUATION line still counts",
        '    west build -b native_sim \\\n        "$src" -- \\\n'
        '        "$nros_module_arg"\n',
        "just/x.just",
        False,
    )
    expect(
        "a hand-written value is a second spelling and fails",
        '    west build -b native_sim "$src" -- -DZEPHYR_EXTRA_MODULES="$root"\n',
        "just/x.just",
        True,
    )
    expect(
        "-t run needs no module",
        "    west build -d build-fvp-ws-entry -t run\n",
        "just/x.just",
        False,
    )
    expect(
        "`${west_cmd[@]}` is the same invocation",
        '    "${west_cmd[@]}" build -b "$board" -d "$bd" "$src" -- -DCONF_FILE=x\n',
        "just/x.just",
        True,
    )
    expect(
        "the smoke-app exemption only covers its own marker",
        "    west build -b native_sim/native/64 "
        "-d tests/zephyr-c-smoke/build tests/zephyr-c-smoke -- -DMAKE=x\n",
        os.path.join("just", "zephyr-dev.just"),
        False,
    )
    expect(
        "...and does not cover a DIFFERENT build in the same file",
        '    west build -b native_sim "$src" -- -DCONF_FILE="$conf"\n',
        os.path.join("just", "zephyr-dev.just"),
        True,
    )

    expect(
        "flags assembled into an array pass when the file ASSEMBLES the flag",
        '    replace_or_append_arg "-DZEPHYR_EXTRA_MODULES" '
        '"$(nros_zephyr_module_root "$r")"\n'
        '    build_argv=(west build -b "$b" -d "$d" "$src" "${west_extra[@]}")\n',
        "scripts/x.sh",
        False,
    )
    expect(
        "...and fail when the file never names the helper",
        '    build_argv=(west build -b "$b" -d "$d" "$src" "${west_extra[@]}")\n',
        "scripts/x.sh",
        True,
    )
    expect(
        "...and fail when the helper is called but never wired to the FLAG",
        '    nros_module_arg="$(nros_zephyr_module_cmake_arg "$r")"\n'
        '    build_argv=(west build -b "$b" -d "$d" "$src" "${west_extra[@]}")\n',
        "scripts/x.sh",
        True,
    )
    # THE MUTANT THE FIRST VERSION MISSED, and the reason this row exists at
    # all: `"${west_cmd[@]}"` is the command NAME, so an exemption keyed on
    # "any array expansion anywhere" fired on it and the module flag's absence
    # stopped being detectable — in `just zephyr build-one`, the very recipe
    # issue 1379 was reported from. The file DOES call the helper on the line
    # above, which is what made the old second condition pass too.
    expect(
        "an array expansion in the COMMAND NAME does not earn the exemption",
        '    nros_module_arg="$(nros_zephyr_module_cmake_arg "$(pwd)")"\n'
        '    "${west_cmd[@]}" build -b "$board" -d "$bd" "$src" -- \\\n'
        '        -DCONF_FILE="$conf"\n',
        "just/x.just",
        True,
    )
    expect(
        "...and the same command WITH the flag still passes",
        '    nros_module_arg="$(nros_zephyr_module_cmake_arg "$(pwd)")"\n'
        '    "${west_cmd[@]}" build -b "$board" -d "$bd" "$src" -- \\\n'
        '        -DCONF_FILE="$conf" "$nros_module_arg"\n',
        "just/x.just",
        False,
    )
    # THE MUTANT THIS GATE MISSED FOR ITS WHOLE LIFE — issue 1458. Not an
    # imagined shape: it is `scripts/build/west-fixtures.sh` verbatim, the
    # builder of all five west fixtures, and the gate reported OK over it while
    # tier 2 failed every run for seventeen days on the module it never named.
    expect(
        "west invoked with its WHOLE argv in an array is still a build",
        '    args=(build -d "$bld" -b "$board" "$src")\n'
        '    args+=(-- "$extra")\n'
        '    env "${tc_env[@]}" west "${args[@]}" || true\n',
        "scripts/x.sh",
        True,
    )
    expect(
        "...and passes once the helper's output enters THAT array",
        '    args=(build -d "$bld" -b "$board" "$src")\n'
        '    args+=(-- "$extra")\n'
        '    args+=("$(nros_zephyr_module_cmake_arg "$repo_root")")\n'
        '    env "${tc_env[@]}" west "${args[@]}" || true\n',
        "scripts/x.sh",
        False,
    )
    expect(
        "...and fails when the helper feeds a DIFFERENT array",
        '    args=(build -d "$bld" -b "$board" "$src")\n'
        '    unused+=("$(nros_zephyr_module_cmake_arg "$repo_root")")\n'
        '    env "${tc_env[@]}" west "${args[@]}" || true\n',
        "scripts/x.sh",
        True,
    )
    expect(
        "an `env`-wrapped `west build` is in command position",
        '    env FOO=1 west build -b native_sim "$src" -- -DCONF_FILE=x\n',
        "scripts/x.sh",
        True,
    )
    expect(
        "an argv-array west call inside an echo is still prose",
        '    echo "  env \\"${tc_env[@]}\\" west \\"${args[@]}\\""\n',
        "scripts/x.sh",
        False,
    )

    expect(
        "a backticked mention in prose is not a command",
        "  application and the image's overlays and execs the same `west build` "
        "— the\n",
        "scripts/x.sh",
        False,
    )

    if not os.path.isfile(os.path.join(root, HELPER)):
        print(f"  SELFTEST FAIL: {HELPER} is missing", file=sys.stderr)
        fails += 1
    elif not quiet:
        print(f"  ok: {HELPER} exists")

    if not quiet or fails:
        print(
            f"check-zephyr-module-binding selftest: {'FAILED' if fails else 'OK'}",
            file=sys.stderr if fails else sys.stdout,
        )
    return 1 if fails else 0


def main() -> int:
    root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

    verbose = "--selftest" in sys.argv
    # The negative control runs EVERY time, before anything is asserted.
    if selftest(root, not verbose):
        return 1
    if verbose:
        return 0

    bad, harvested = offenders(root)

    if "--list" in sys.argv:
        for path, lineno, cmd in harvested:
            print(f"{path}:{lineno}: {' '.join(cmd.split())[:140]}")
        return 0

    if len(harvested) < MIN_INVOCATIONS:
        print(
            f"check-zephyr-module-binding: harvested only {len(harvested)} "
            f"`west build` invocation(s), expected at least {MIN_INVOCATIONS}.\n"
            "  The harvest IS the evidence — a pattern that stopped matching "
            "reports OK over an empty set, which is the failure this floor "
            "exists to refuse. Run with --list and fix WEST_BUILD.",
            file=sys.stderr,
        )
        return 1

    if bad:
        print(
            f"check-zephyr-module-binding: {len(bad)} `west build` "
            "invocation(s) configure a nano-ros image without naming this "
            "checkout's nros module (issue 1379):\n",
            file=sys.stderr,
        )
        for path, lineno, cmd in bad:
            print(f"  {path}:{lineno}", file=sys.stderr)
            print(f"    {cmd[:160]}", file=sys.stderr)
        print(
            "\nWithout the module flag the image takes whatever `nros` module "
            "the west workspace is bound to. Since phase-440 W4 that workspace "
            "is shared by every checkout on the host, so the module can be "
            "ANOTHER clone's — half of one tree, half of another, linked "
            "silently. Issue 1379 measured exactly that.\n"
            "\nFix, in the invocation above:\n"
            f"  source {HELPER}\n"
            '  nros_module_arg="$(nros_zephyr_module_cmake_arg "$repo_root")"\n'
            '  west build … -- … "$nros_module_arg"\n'
            "\nResolve it BEFORE any `cd` into the workspace. If the "
            "application genuinely loads no nros module, add it to EXEMPT in "
            "this script with the reason.",
            file=sys.stderr,
        )
        return 1

    print(
        f"check-zephyr-module-binding: OK — {len(harvested)} `west build` "
        f"invocation(s), every configuring one names this checkout's module."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
