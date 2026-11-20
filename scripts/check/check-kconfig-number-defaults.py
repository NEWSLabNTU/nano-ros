#!/usr/bin/env python3
"""Every `int` / `hex` Kconfig symbol nano-ros defines has an unconditional
default.

Kconfig emits an int or hex symbol that has no value as an EMPTY assignment
(`CONFIG_X=`), and Zephyr's generated `configs.c` turns every int symbol into
`GEN_ABSOLUTE_SYM_KCONFIG(CONFIG_X, <value>)`. Empty, that is `.equ CONFIG_X,`,
which fails to assemble, in EVERY Zephyr image, because the symbol is defined
whether or not anything uses it. No host gate builds a Zephyr image, so the
first sign is a broken build on a board lane. That is how a retired int lost
its default and went unseen through a green `ci gate`.

Two sources are checked, because both reach a Zephyr configure:
  * the tracked Kconfig files under `zephyr/` (the module's `Kconfig`, the
    generated knob prompts, ...), with a symbol's definitions MERGED across
    files, since a prompt file and the module may split one symbol;
  * every registry row (core `knobs.toml` and every in-tree package
    `nros-knobs.toml`), whose `kconfig_attrs` the configure renders into
    `Kconfig.packages`.

A default with an `if` does not count: under the condition's negation the
symbol is empty again. The rule has no allowlist.

    check-kconfig-number-defaults.py           check (self-test first)
    check-kconfig-number-defaults.py --self-test
"""

import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "scripts" / "lib"))

NUMBER = ("int", "hex")
CONFIG = re.compile(r"^\s*(?:menu)?config\s+(\w+)\s*$")
TYPE = re.compile(r"^\s+(bool|tristate|string|int|hex)\b")
DEFAULT = re.compile(r"^\s+default\s+(.*?)\s*$")
BLOCK_END = re.compile(r"^(if|endif|menu|endmenu|choice|endchoice|source|rsource|osource|"
                       r"orsource|comment|mainmenu)\b")


def kconfig_symbols(files):
    """{symbol: (type, [default expressions])}, merged over every definition
    in `files` ([(name, text)])."""
    out = {}
    for _name, text in files:
        cur = None
        for line in text.splitlines():
            m = CONFIG.match(line)
            if m:
                cur = m.group(1)
                out.setdefault(cur, [None, []])
                continue
            if not line.startswith((" ", "\t")) and BLOCK_END.match(line.strip()):
                cur = None
                continue
            if cur is None:
                continue
            t = TYPE.match(line)
            if t and out[cur][0] is None:
                out[cur][0] = t.group(1)
            d = DEFAULT.match(line)
            if d:
                out[cur][1].append(d.group(1))
    return {k: (v[0], v[1]) for k, v in out.items()}


def unconditional(defaults):
    return any(not re.search(r"\sif\s", " " + d + " ") for d in defaults)


def kconfig_problems(files):
    return [f"CONFIG_{sym} ({ty}) has no unconditional `default`; defaults seen: "
            f"{defs or 'none'}"
            for sym, (ty, defs) in sorted(kconfig_symbols(files).items())
            if ty in NUMBER and not unconditional(defs)]


def row_problems(rows):
    out = []
    for r in rows:
        if r.get("type") not in NUMBER:
            continue
        defs = [a[len("default "):] for a in (r.get("kconfig_attrs") or [])
                if a.startswith("default ")]
        if not unconditional(defs):
            where = r.get("package") or "core knobs.toml"
            out.append(f"registry row {r['key']} ({where}, {r['type']}) has no unconditional "
                       f"`default` in its kconfig_attrs")
    return out


def self_test():
    ok = [("K", "config A\n    int\n    default 3\n\nconfig B\n    hex\n    default 0x1\n")]
    split = [("K", "config A\n    int\n    default 3\n"), ("P", 'config A\n    int "prompt"\n')]
    empty = [("K", 'config A\n    int "Retired"\n    help\n      gone\n')]
    cond = [("K", "config A\n    int\n    default 3 if B\n")]
    endif = [("K", "if X\nconfig A\n    int\nendif\nconfig B\n    bool\n    default 4\n")]
    cases = [(ok, 0), (split, 0), (empty, 1), (cond, 1), (endif, 1)]
    for i, (files, want) in enumerate(cases):
        got = len(kconfig_problems(files))
        if got != want:
            return f"self-test kconfig case {i}: {got} problem(s), want {want}"
    rows = [{"key": "a.x", "type": "int", "kconfig_attrs": ["default 1", "range 0 9"]},
            {"key": "a.y", "type": "int", "kconfig_attrs": ["range 0 9"]},
            {"key": "a.z", "type": "hex", "kconfig_attrs": ["default 0x1 if Q"]},
            {"key": "a.s", "type": "string", "kconfig_attrs": []}]
    if len(row_problems(rows)) != 2:
        return f"self-test rows: {row_problems(rows)}"
    return None


def main(argv):
    err = self_test()
    if err:
        print(f"check-kconfig-number-defaults: {err}")
        return 1
    if argv == ["--self-test"]:
        print("check-kconfig-number-defaults self-test: ok")
        return 0
    import knob_registry  # the one reader of the registry
    names = subprocess.run(["git", "ls-files", "zephyr/Kconfig*", "zephyr/**/Kconfig*"],
                           cwd=REPO, capture_output=True, text=True, check=True).stdout.split()
    files = [(n, (REPO / n).read_text()) for n in sorted(set(names))]
    problems = kconfig_problems(files) + row_problems(knob_registry.knob_rows())
    if problems:
        print("check-kconfig-number-defaults: an int/hex Kconfig symbol can be EMPTY, which "
              "breaks configs.c in every Zephyr image:")
        for p in problems:
            print(f"  {p}")
        print("Give it an unconditional `default` (a retired number keeps one; the "
              "refusal reads that default as unset).")
        return 1
    n = sum(1 for _, (t, _) in kconfig_symbols(files).items() if t in NUMBER)
    print(f"check-kconfig-number-defaults: OK — {n} int/hex symbol(s) in {len(files)} "
          f"Kconfig file(s), and every registry row, have an unconditional default")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
