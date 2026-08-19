#!/usr/bin/env python3
"""Every Zephyr symbol `zephyr/Kconfig` names must EXIST on every supported line.

Issue 0651. Kconfig's failure mode for a symbol that is not there is a WARNING,
not an error: `select FOO` where `FOO` is undefined builds a kernel without
whatever `FOO` would have enabled, and says so in a line nobody reads. On the
line you can build locally that is survivable, because the runtime failure
follows. On the line you cannot, it is not — nano-ros supports 3.7 (LTS, the
`just ci`/`ci-matrix`/`ci-full` default) and 4.4 (rolling, nightly-only), so a
`select` that is correct on 3.7 and misspelled on 4.4 surfaces a DAY later,
attributed to whatever else moved that night.

The live example is `select POSIX_PRIORITY_SCHEDULING`, added to
`NROS_ZENOH_MULTI_THREAD` for issue 0626: verified on 3.7 by building
`rust/talker` and confirming the symbols, and unverifiable on 4.4 without a
workspace nobody has. Zephyr reorganised its POSIX options between these
releases, which is exactly where a rename hides.

Symbol existence does not need a BUILD — only the Zephyr source. So this reads
the pinned trees and answers the question the nightly currently answers a day
late.

## Contract

* Symbols this repo defines itself (`NROS_*` in `zephyr/Kconfig`) are ours; only
  symbols we expect ZEPHYR to provide are checked.
* A line is checked when its tree is present. Which lines were checked is always
  REPORTED — a gate for an invisible-warning class must not itself be quietly
  partial.
* Checking NO line is a failure, not a pass. Issue 0702's whole subject is
  checks that report success having measured nothing.

## Trees

  3.7   $NROS_ZEPHYR_WORKSPACE/zephyr, else zephyr-workspace/zephyr
  4.4   build/zephyr-kconfig/zephyr-4.4, else ../nano-ros-workspace-4.4/zephyr

The 4.4 tree does not need a west workspace (~20 modules, and issue 0078 filled
a CI disk building one). Kconfig lives in the `zephyr` repo alone:

  git clone --depth 1 --branch v4.4.0 --single-branch \\
      https://github.com/zephyrproject-rtos/zephyr build/zephyr-kconfig/zephyr-4.4

Run:  python3 scripts/check-zephyr-kconfig-symbols.py [--self-test]
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
KCONFIG = os.path.join(ROOT, "zephyr", "Kconfig")

# `select FOO`, `select FOO if BAR`, `depends on FOO && BAR`, `imply FOO`.
REF = re.compile(r"^\s*(?:select|imply|depends\s+on)\s+(.+?)\s*$", re.M)
# A Kconfig symbol: upper-case, digits, underscore. Operators and literals out.
SYMBOL = re.compile(r"\b([A-Z][A-Z0-9_]{2,})\b")
DEFINES = re.compile(r"^\s*(?:menu)?config\s+([A-Z0-9_]+)\s*$", re.M)

# Kconfig keywords / literals that match the symbol shape but are not symbols.
NOT_SYMBOLS = {"IF", "ON", "Y", "N", "M", "AND", "OR", "NOT"}


def referenced_symbols(text: str):
    """Zephyr symbols referenced by this file, minus the ones it defines."""
    ours = set(DEFINES.findall(text))
    out = {}
    for m in REF.finditer(text):
        line_no = text[: m.start()].count("\n") + 1
        for sym in SYMBOL.findall(m.group(1)):
            if sym in NOT_SYMBOLS or sym in ours or sym.startswith("NROS_"):
                continue
            out.setdefault(sym, line_no)
    return out


def tree_symbols(tree: str):
    """Every symbol DEFINED anywhere under a Zephyr tree's Kconfig files."""
    found = set()
    for dirpath, dirnames, filenames in os.walk(tree):
        dirnames[:] = [d for d in dirnames if d not in (".git", "build", "twister-out")]
        for fn in filenames:
            if not (fn == "Kconfig" or fn.startswith("Kconfig.")):
                continue
            try:
                with open(os.path.join(dirpath, fn), encoding="utf8", errors="replace") as fh:
                    found.update(DEFINES.findall(fh.read()))
            except OSError:
                continue
    return found


def line_trees():
    """(line, [roots]) for every supported line whose tree is present.

    A line's symbol universe is the zephyr repo PLUS the modules west pins —
    `RUST` lives in `zephyr-lang-rust`, not in zephyr, so checking the zephyr
    tree alone reports it missing on every line. That was this gate's first
    false positive, and it is why `roots` is a list.
    """
    kc = os.path.join(ROOT, "build", "zephyr-kconfig")
    ws = os.environ.get("NROS_ZEPHYR_WORKSPACE") or os.path.join(ROOT, "zephyr-workspace")
    candidates = [
        # A west workspace already holds zephyr + every module: walk zephyr and
        # `modules/` rather than the whole tree (bootloader/tools carry no
        # symbols we reference, and skipping them keeps this seconds, not minutes).
        ("3.7", [os.path.join(ws, "zephyr"), os.path.join(ws, "modules")]),
        (
            "3.7",
            [
                os.path.join(ROOT, "zephyr-workspace", "zephyr"),
                os.path.join(ROOT, "zephyr-workspace", "modules"),
            ],
        ),
        # A bare pair of clones is enough, and far cheaper than a west workspace
        # (issue 0078 filled a CI disk building one).
        ("4.4", [os.path.join(kc, "zephyr-4.4"), os.path.join(kc, "zephyr-lang-rust-4.4")]),
        (
            "4.4",
            [
                os.path.join(ROOT, "..", "nano-ros-workspace-4.4", "zephyr"),
                os.path.join(ROOT, "..", "nano-ros-workspace-4.4", "modules"),
            ],
        ),
    ]
    seen, out = set(), []
    for line, roots in candidates:
        if line in seen:
            continue
        if not os.path.isfile(os.path.join(roots[0], "Kconfig")):
            continue
        present = [os.path.normpath(r) for r in roots if os.path.isdir(r)]
        seen.add(line)
        out.append((line, present))
    return out


def tree_version(tree: str):
    try:
        with open(os.path.join(tree, "VERSION"), encoding="utf8") as fh:
            v = dict(
                (k.strip(), val.strip())
                for k, _, val in (ln.partition("=") for ln in fh if "=" in ln)
            )
        return f"{v.get('VERSION_MAJOR', '?')}.{v.get('VERSION_MINOR', '?')}"
    except OSError:
        return "?"


def self_test():
    text = (
        "config NROS_THING\n\tbool\n\tselect CPP\n\tdepends on NET_SOCKETS && POSIX_API\n"
        "config NROS_OTHER\n\tdepends on NROS_THING\n"
    )
    refs = referenced_symbols(text)
    for want in ("CPP", "NET_SOCKETS", "POSIX_API"):
        if want not in refs:
            sys.stderr.write(f"self-test: {want} was not extracted\n")
            sys.exit(2)
    for never in ("NROS_THING", "NROS_OTHER"):
        if never in refs:
            sys.stderr.write(f"self-test: our own {never} was treated as Zephyr's\n")
            sys.exit(2)
    # `if` in `select FOO if BAR` is a keyword, and BAR is a real reference.
    refs = referenced_symbols("config NROS_X\n\tselect FOO if BAR\n")
    if "IF" in refs or "FOO" not in refs or "BAR" not in refs:
        sys.stderr.write("self-test: `select FOO if BAR` mis-parsed\n")
        sys.exit(2)
    sys.stdout.write("check-zephyr-kconfig-symbols self-test: OK\n")


def main():
    if "--self-test" in sys.argv:
        self_test()
        return
    self_test()

    with open(KCONFIG, encoding="utf8") as fh:
        text = fh.read()
    refs = referenced_symbols(text)
    trees = line_trees()

    if not trees:
        sys.stderr.write(
            "error: no Zephyr tree to check against — this gate verified NOTHING.\n\n"
            "  3.7:  just zephyr setup            (or set NROS_ZEPHYR_WORKSPACE)\n"
            "  4.4:  git clone --depth 1 --branch v4.4.0 --single-branch \\\n"
            "          https://github.com/zephyrproject-rtos/zephyr \\\n"
            "          build/zephyr-kconfig/zephyr-4.4\n\n"
            "Exiting non-zero rather than passing: a check that measured nothing\n"
            "must not report success (issue 0702).\n"
        )
        sys.exit(1)

    missing = []
    for line, roots in trees:
        have = set()
        for r in roots:
            have |= tree_symbols(r)
        for sym, at in sorted(refs.items()):
            if sym not in have:
                missing.append((line, roots[0], sym, at))

    checked = ", ".join(f"{ln} ({tree_version(r[0])})" for ln, r in trees)
    unchecked = sorted({"3.7", "4.4"} - {ln for ln, _ in trees})

    if missing:
        sys.stderr.write(
            "error: %d Kconfig symbol reference(s) name a symbol the Zephyr line "
            "does not define.\n\n" % len(missing)
        )
        for line, tree, sym, at in missing:
            sys.stderr.write(f"  zephyr/Kconfig:{at}  {sym}  — absent on {line} ({tree})\n")
        sys.stderr.write(
            "\nKconfig treats an undefined symbol as a WARNING, so this does not fail\n"
            "the build — it silently drops whatever the symbol would have enabled.\n"
            "Check the spelling on that line; Zephyr reorganises option groups\n"
            "between releases (issue 0651).\n"
        )
        sys.exit(1)

    sys.stdout.write(
        "zephyr-kconfig-symbols OK — %d referenced symbol(s), lines checked: %s\n"
        % (len(refs), checked)
    )
    if unchecked:
        sys.stdout.write(
            "  NOT checked: %s — no tree present. This gate exists because that\n"
            "  line's failures are invisible until the nightly (issue 0651).\n"
            % ", ".join(unchecked)
        )


if __name__ == "__main__":
    main()
