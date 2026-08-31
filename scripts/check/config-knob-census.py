#!/usr/bin/env python3
"""phase-400 — count where configuration lives, reproducibly.

The phase doc opens with a table (78 Kconfig symbols, 21 `NROS_*` env, 9
`ZPICO_*` env, against 3 knobs in the RFC-0049 ladder) and every wave's gate is
stated as "measured the same way as the table at the top". No method was
recorded, so the gate could not be evaluated: two people counting by hand get
two numbers, and a wave cannot show it moved anything.

This is that method. It is deliberately BUILDLESS — grep and TOML, no cargo —
so it can run on the fast line and so the number does not depend on a build
succeeding.

What each row counts:

  ladder knobs      fields on the typed `[knobs.*]` structs in
                    `nros-board-common`. These are the knobs that have a
                    platform and board rung and that `nros config explain`
                    reports. This is the number the phase exists to RAISE.

  Kconfig symbols   `config NROS_*` DECLARATIONS in Kconfig files — declared,
                    not referenced, because a symbol read in ten places is one
                    knob.

  build-script env  distinct `NROS_*` names a `build.rs` reads from the
                    environment. Not every `NROS_*` in the tree: a name that
                    only cmake sets and cmake reads is not a build-script knob.

  ZPICO_* env       the same, for the zenoh-pico tenant's own namespace.

A knob counted in the ladder MAY also still appear as env — that is the point:
migrating keeps the env name as the front-end. The ladder number rising is the
signal, not the env number falling to zero.
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# The ratchet. "The ladder knob count rises" is the wave gate, and a count that
# only ever rises is enforceable where a count that "should fall" is not: an
# env name legitimately SURVIVES migration as the front-end, so the env rows are
# reported, never gated. Raise this when a tenant lands.
LADDER_FLOOR = 13


def tracked(*globs):
    out = subprocess.check_output(
        ["git", "-C", ROOT, "ls-files", *globs], text=True
    ).split()
    return [os.path.join(ROOT, p) for p in out]


def read(path):
    try:
        with open(path, encoding="utf-8", errors="replace") as fh:
            return fh.read()
    except OSError:
        return ""


def _struct_fields(src, struct):
    """The `pub` field names of one struct. Factored out so the selftest can
    drive it on synthetic input rather than on the real file."""
    m = re.search(r"pub struct " + struct + r"\s*\{(.*?)\n\}", src, re.S)
    if not m:
        return []
    body = re.sub(r"(?m)^\s*(///|//).*$", "", m.group(1))
    return sorted(set(re.findall(r"(?m)^\s*pub ([a-z_][a-z0-9_]*)\s*:", body)))


def ladder_knobs():
    """Fields on the typed `[knobs.*]` structs — the migrated knobs."""
    src = read(os.path.join(ROOT, "packages/boards/nros-board-common/src/platform_config.rs"))
    total = {}
    for struct in ("TxKnobs", "ExecutorKnobs", "TransportKnobs"):
        fields = _struct_fields(src, struct)
        if fields:
            total[struct] = fields
    return total


def kconfig_symbols():
    syms = set()
    for p in tracked("*Kconfig", "*Kconfig.*", "*/Kconfig"):
        for m in re.finditer(r"(?m)^\s*(?:menu)?config\s+(NROS_[A-Z0-9_]+)", read(p)):
            syms.add(m.group(1))
    return syms


def _env_names_in(txt, prefix):
    """`<PREFIX>_*` names READ from the environment in one source string.

    Comments are stripped first: a knob named in a comment is documentation,
    not a reader, and counting it inflates the number this gate exists to
    track. Factored out so the selftest drives it on synthetic input.
    """
    stripped = re.sub(r"(?m)^\s*(///|//).*$", "", txt)
    return set(
        m.group(1)
        for m in re.finditer(
            r'(?:env::var|env::var_os|env|env_usize|env_bool|knob_usize|knob_bool)'
            r'\s*\(\s*&?"(' + prefix + r'_[A-Z0-9_]+)"',
            stripped,
        )
    )


def build_script_env(prefix):
    """`<PREFIX>_*` names a build.rs reads from the environment."""
    names = set()
    for p in tracked("*build.rs"):
        names |= _env_names_in(read(p), prefix)
    return names


def self_test() -> None:
    """Negative controls for both counters, on synthetic input.

    On the normal path, not behind a flag — `check-gate-selftests` requires it,
    on the reasoning that a control nobody runs decays into a comment. This
    census earns the scepticism twice over: it is a COUNTING gate, so a regex
    that silently matches too much or too little still prints a plausible
    number, and a wrong baseline is worse than none because it looks measured.
    """
    # The env matcher must see the idioms build scripts actually use...
    for src in (
        'let n = env_usize("NROS_EXECUTOR_MAX_CBS", 4);',
        'std::env::var("NROS_THING").ok()',
        'nros_zephyr_build::knob_usize("NROS_OTHER", &k, 1)',
    ):
        assert _env_names_in(src, "NROS"), f"selftest: missed a read in {src!r}"
    # ...and must not count a name it merely MENTIONS, or another prefix.
    for src in (
        '// NROS_EXECUTOR_MAX_CBS was read here once',
        'println!("set NROS_THING");',
        'let n = env_usize("ZPICO_TX_BATCH", 0);',
    ):
        assert not _env_names_in(src, "NROS"), f"selftest: false positive on {src!r}"

    # The ladder counter reads STRUCT FIELDS, so it must not count a doc
    # comment, a non-`pub` field, or a field of a struct it was not asked for.
    src = (
        "pub struct TxKnobs {\n"
        "    /// pub not_a_field: usize,\n"
        "    pub batch: Option<bool>,\n"
        "    private: usize,\n"
        "}\n"
        "pub struct Unrelated {\n    pub nope: usize,\n}\n"
    )
    fields = _struct_fields(src, "TxKnobs")
    assert fields == ["batch"], f"selftest: ladder counter read {fields!r}"


def main() -> int:
    self_test()
    ladder = ladder_knobs()
    n_ladder = sum(len(v) for v in ladder.values())
    kconfig = kconfig_symbols()
    nros_env = build_script_env("NROS")
    zpico_env = build_script_env("ZPICO")

    print("phase-400 config census")
    print()
    print(f"  {'where configuration lives':<34} count")
    print(f"  {'-' * 34} -----")
    print(f"  {'knobs in the RFC-0049 ladder':<34} {n_ladder}")
    for struct, fields in sorted(ladder.items()):
        print(f"      {struct:<30} {len(fields)}")
    print(f"  {'Kconfig CONFIG_NROS_* declarations':<34} {len(kconfig)}")
    print(f"  {'NROS_* env read by a build.rs':<34} {len(nros_env)}")
    print(f"  {'ZPICO_* env read by a build.rs':<34} {len(zpico_env)}")

    if "--check" in sys.argv and n_ladder < LADDER_FLOOR:
        sys.stderr.write(
            f"config-knob-census: FAILED — the ladder holds {n_ladder} knob(s), "
            f"below the {LADDER_FLOOR} recorded floor.\n"
            "  A knob leaving the ladder is a knob losing its platform and board\n"
            "  rung and its `nros config explain` row. If that is deliberate,\n"
            "  lower LADDER_FLOOR in this file and say why in the commit.\n"
        )
        return 1

    if "--verbose" in sys.argv:
        print()
        for label, items in (
            ("Kconfig", kconfig),
            ("NROS_* env", nros_env),
            ("ZPICO_* env", zpico_env),
        ):
            print(f"  {label}:")
            for n in sorted(items):
                print(f"    {n}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
