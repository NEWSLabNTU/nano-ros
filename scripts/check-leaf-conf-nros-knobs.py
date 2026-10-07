#!/usr/bin/env python3
"""A leaf's conf files hold no nano-ros knob — phase-481 W2 (RFC-0098 D10/D11).

On Zephyr an image's nano-ros configuration has ONE source, its `system.toml`:
the nano-ros module's `module_ext_root` hook renders it into a Kconfig fragment
placed last in `EXTRA_CONF_FILE` (`zephyr/cmake/nros_image_kconfig.cmake`,
`nros ws leaf-system --kconfig-out`). A `CONFIG_NROS_*` line in a leaf's
`prj.conf` is therefore a SECOND statement of the same fact — the shape issue
1721 found, where `examples/zephyr/c/talker/system.toml` said `rmw = "zenoh"`
while the same leaf built XRCE from `prj-xrce.conf`. Ranking the two only
decides which one silently loses, so this gate removes the second source.

WHAT IT REFUSES, in a tracked `examples/**/*.conf` (leaves, workspace images,
templates):

  rmw       the RMW choice (`choice NROS_RMW_BACKEND`'s members) — write the
            image's `rmw = "<x>"`;
  api       the language API (`choice NROS_API`'s members) — derived from the
            package (`Cargo.toml` is Rust; a `CMakeLists.txt` package's entry is
            the TYPED C++ carrier), so there is nothing to write: delete it;
  endpoint  `CONFIG_NROS_ZENOH_LOCATOR` / `CONFIG_NROS_XRCE_AGENT_{ADDR,PORT}` —
            write the image's `locator = "…"`;
  knob      every other `CONFIG_NROS_<X>` the module's Kconfig defines, which is
            exactly what an `[image.<id>] env` row renders to: through
            `nros_zephyr_build::KCONFIG_PAIRS` when the two names are different
            words (read from that table, never restated), else `NROS_<X>`.

`CONFIG_NROS=y` (enabling the module) is Zephyr's own switch and stays. So do
the shared board fragments under `cmake/zephyr/` — the BOARD layer, not a leaf
file — and the per-run `-DCONFIG_*` values fixture scripts pass, which are
command-line, not files.

A RATCHET, because today's tree still holds the W3 migration's inventory: the
baseline records each offending file's line count, and BOTH directions fail
(`scripts/lib/ratchet.py`) — a new line anywhere is a rise, and a file W3
drains must lower or delete its row in the same change, so the debt can only
shrink. The refusal names the `system.toml` and the line to write there.

Usage::

    check-leaf-conf-nros-knobs.py [--write-baseline] [--selftest] [--list]
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
from ratchet import fell_instructions, judge  # noqa: E402  phase-472 W9

BASELINE = os.path.join(ROOT, ".config", "leaf-conf-nros-knobs-baseline.txt")
KCONFIG = os.path.join("zephyr", "Kconfig")
PAIRS_SRC = os.path.join("packages", "tooling", "nros-zephyr-build", "src", "lib.rs")
# The renderer's endpoint symbols (`leaf_kconfig::render`, from `locator`).
ENDPOINT = ("CONFIG_NROS_ZENOH_LOCATOR", "CONFIG_NROS_XRCE_AGENT_ADDR",
            "CONFIG_NROS_XRCE_AGENT_PORT")
RMW_WORD = {
    "CONFIG_NROS_RMW_ZENOH": "zenoh",
    "CONFIG_NROS_RMW_XRCE": "xrce",
    "CONFIG_NROS_RMW_CYCLONEDDS": "cyclonedds",
}
ASSIGN = re.compile(r"^\s*(CONFIG_NROS_[A-Za-z0-9_]+)\s*=\s*(.*?)\s*$")
HEADER = (
    "# phase-481 W2 — tracked examples/**/*.conf files that still assign a\n"
    "# nano-ros knob (`check-leaf-conf-nros-knobs`), with the number of such\n"
    "# lines. A RATCHET: it may only SHRINK, and a file that drains must lower or\n"
    "# delete its row in the same change. W3 empties it.\n"
    "#\n"
    "# Regenerate ONLY to record lines that moved into system.toml:\n"
    "#     python3 scripts/check-leaf-conf-nros-knobs.py --write-baseline\n"
)


def kconfig_model(text: str):
    """(defined NROS symbols, rmw choice members, api choice members) from
    the module's Kconfig text — all with the `CONFIG_` prefix."""
    defined, rmw, api = set(), set(), set()
    choice = None
    for raw in text.splitlines():
        words = raw.split()
        if not words:
            continue
        if words[0] == "choice":
            choice = words[1] if len(words) > 1 else ""
        elif words[0] == "endchoice":
            choice = None
        elif words[0] in ("config", "menuconfig") and len(words) > 1:
            sym = "CONFIG_" + words[1]
            defined.add(sym)
            if choice == "NROS_RMW_BACKEND":
                rmw.add(sym)
            elif choice == "NROS_API":
                api.add(sym)
    return defined, rmw, api


def kconfig_pairs(text: str) -> dict:
    """{CONFIG symbol: env name} for every `KCONFIG_PAIRS` row — scoped to the
    const, so a tuple in a doc comment or test is not read as a row."""
    m = re.search(r"const KCONFIG_PAIRS[^=]*=\s*&\[(.*?)\];", text, re.S)
    if not m:
        raise SystemExit(f"check-leaf-conf-nros-knobs: no KCONFIG_PAIRS in {PAIRS_SRC}")
    body = re.sub(r"//[^\n]*", "", m.group(1))
    rows = re.findall(r'"([A-Z0-9_]+)"\s*,\s*"(CONFIG_[A-Z0-9_]+)"', body)
    if not rows:
        raise SystemExit("check-leaf-conf-nros-knobs: KCONFIG_PAIRS harvested empty")
    return {sym: env for env, sym in rows}


def classify(sym: str, value: str, model, pairs):
    """(kind, the system.toml spelling) for one assignment, or None when the
    line is not a nano-ros knob (CONFIG_NROS itself, or a symbol the module
    does not define — Zephyr refuses that on its own)."""
    defined, rmw, api = model
    if sym == "CONFIG_NROS" or sym not in defined:
        return None
    if sym in rmw:
        word = RMW_WORD.get(sym, sym[len("CONFIG_NROS_RMW_"):].lower())
        return "rmw", f'rmw = "{word}"' if value.strip() == "y" else "(an unselected RMW: delete the line)"
    if sym in api:
        return "api", "nothing — the API follows the package (delete the line)"
    if sym in ENDPOINT:
        v = value.strip().strip('"')
        spell = {
            "CONFIG_NROS_ZENOH_LOCATOR": f'locator = "{v}"',
            "CONFIG_NROS_XRCE_AGENT_ADDR": f'locator = "udp/{v}:<port>"',
            "CONFIG_NROS_XRCE_AGENT_PORT": f'locator = "udp/<host>:{v}"',
        }[sym]
        return "endpoint", spell
    env = pairs.get(sym, sym[len("CONFIG_"):])
    v = value.strip()
    if v in ("y", "n"):
        v = "1" if v == "y" else "0"
    v = v.strip('"')
    return "knob", f'env = {{ {env} = "{v}" }}'


def system_toml_for(conf_path: str, root: str) -> str | None:
    """The nearest `system.toml` above a conf file, repo-relative."""
    d = os.path.dirname(os.path.join(root, conf_path))
    while True:
        cand = os.path.join(d, "system.toml")
        if os.path.isfile(cand):
            return os.path.relpath(cand, root)
        if os.path.abspath(d) in (os.path.abspath(root), "/"):
            return None
        d = os.path.dirname(d)


def image_label(system_toml: str | None, root: str) -> str:
    if not system_toml:
        return "[image.<id>]"
    try:
        ids = re.findall(r"^\s*\[image\.([A-Za-z0-9_\-]+)\]", open(os.path.join(root, system_toml),
                                                                    encoding="utf8").read(), re.M)
    except OSError:
        ids = []
    return f"[image.{ids[0]}]" if len(ids) == 1 else "[image.<id>]"


def findings(files, read, model, pairs):
    """[(path, lineno, symbol, value, kind, spelling)] over `files`."""
    out = []
    for path in files:
        text = read(path)
        if text is None:
            continue
        for n, line in enumerate(text.splitlines(), 1):
            if line.lstrip().startswith("#"):
                continue
            m = ASSIGN.match(line)
            if not m:
                continue
            c = classify(m.group(1), m.group(2), model, pairs)
            if c:
                out.append((path, n, m.group(1), m.group(2), c[0], c[1]))
    return out


def tracked_confs(root: str):
    r = subprocess.run(["git", "-C", root, "ls-files", "-z", "--", "examples/**/*.conf",
                        "examples/*.conf"], capture_output=True, text=True, check=True)
    return sorted(set(p for p in r.stdout.split("\0") if p))


def read_file(root):
    def read(path):
        try:
            return open(os.path.join(root, path), encoding="utf8", errors="replace").read()
        except OSError:
            return None
    return read


def load_baseline(path=BASELINE) -> dict:
    out = {}
    if not os.path.exists(path):
        return out
    for line in open(path, encoding="utf8"):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        count, f = line.split(None, 1)
        out[f] = int(count)
    return out


def counts(found) -> dict:
    out = {}
    for f in found:
        out[f[0]] = out.get(f[0], 0) + 1
    return out


def render_baseline(c: dict) -> str:
    return HEADER + "".join(f"{n} {p}\n" for p, n in sorted(c.items()))


def verdict(found, base, root):
    """Failure lines (empty when the tree matches its baseline)."""
    now = counts(found)
    rose, fell = judge(now, base)
    lines = []
    if rose:
        lines.append("check-leaf-conf-nros-knobs: a conf file assigns a nano-ros knob "
                     "that system.toml owns (RFC-0098 D10/D11):\n")
        risen = {m.key for m in rose}
        for path, n, sym, val, kind, spelling in found:
            if path not in risen:
                continue
            st = system_toml_for(path, root)
            lines.append(f"  {path}:{n}: {sym}={val}  [{kind}]")
            lines.append(f"      write in {st or '<the leaf>/system.toml'} "
                         f"{image_label(st, root)}: {spelling}")
        lines.append(
            "\n  The nano-ros module renders the image's system.toml into a Kconfig\n"
            "  fragment merged after every conf file (zephyr/cmake/nros_image_kconfig.cmake),\n"
            "  so a conf line here is a second statement of the same fact, and the\n"
            "  two can disagree silently (issue 1721).")
    if fell:
        lines.extend(fell_instructions(
            fell, os.path.relpath(BASELINE, ROOT),
            lambda k, n: f"{n} {k}" if n else None,
            "python3 scripts/check-leaf-conf-nros-knobs.py --write-baseline"))
    return lines


def model_and_pairs(root=ROOT):
    model = kconfig_model(open(os.path.join(root, KCONFIG), encoding="utf8").read())
    pairs = kconfig_pairs(open(os.path.join(root, PAIRS_SRC), encoding="utf8").read())
    return model, pairs


def selftest(verbose=False) -> int:
    """Prove the gate can fail. Runs on every invocation."""
    ok = fail = 0

    def chk(desc, cond):
        nonlocal ok, fail
        if verbose or not cond:
            print(f"  {'ok   ' if cond else 'FAIL '} {desc}")
        ok += 1 if cond else 0
        fail += 0 if cond else 1

    kc = ("menuconfig NROS\n bool \"n\"\nif NROS\nchoice NROS_API\nconfig NROS_C_API\n bool \"c\"\n"
          "config NROS_CPP_API\n bool \"cpp\"\nendchoice\nchoice NROS_RMW_BACKEND\n"
          "config NROS_RMW_ZENOH\n bool \"z\"\nconfig NROS_RMW_XRCE\n bool \"x\"\nendchoice\n"
          "config NROS_EXECUTOR_MAX_CBS\n int \"c\"\nconfig NROS_MAX_QUERYABLES\n int \"q\"\n"
          "config NROS_XRCE_AGENT_PORT\n int \"p\"\nendif\n")
    model = kconfig_model(kc)
    pairs = kconfig_pairs('pub const KCONFIG_PAIRS: &[(&str, &str)] = &[\n'
                          '    // a comment ("NOT_A", "CONFIG_ROW")\n'
                          '    ("ZPICO_MAX_QUERYABLES", "CONFIG_NROS_MAX_QUERYABLES"),\n];\n')
    chk("KCONFIG_PAIRS harvest is scoped and skips comments",
        pairs == {"CONFIG_NROS_MAX_QUERYABLES": "ZPICO_MAX_QUERYABLES"})
    files = {
        "examples/a/prj.conf": ("CONFIG_NROS=y\nCONFIG_NROS_RMW_XRCE=y\nCONFIG_NET_TCP=y\n"
                                "# CONFIG_NROS_C_API=y\nCONFIG_NROS_C_API=y\n"
                                "CONFIG_NROS_EXECUTOR_MAX_CBS=9\nCONFIG_NROS_MAX_QUERYABLES=4\n"
                                "CONFIG_NROS_XRCE_AGENT_PORT=2018\nCONFIG_NROS_UNDEFINED=1\n"),
        "examples/b/prj.conf": "CONFIG_NROS=y\nCONFIG_NET_TCP=y\n",
    }
    found = findings(sorted(files), files.get, model, pairs)
    kinds = {(f[2], f[4], f[5]) for f in found}
    chk("negative control: the RMW choice is refused, naming `rmw`",
        ("CONFIG_NROS_RMW_XRCE", "rmw", 'rmw = "xrce"') in kinds)
    chk("the language API is refused", any(f[2] == "CONFIG_NROS_C_API" and f[4] == "api" for f in found))
    chk("a derived-identical knob names its env row",
        ("CONFIG_NROS_EXECUTOR_MAX_CBS", "knob", 'env = { NROS_EXECUTOR_MAX_CBS = "9" }') in kinds)
    chk("a KCONFIG_PAIRS knob names the PAIRED env word",
        ("CONFIG_NROS_MAX_QUERYABLES", "knob", 'env = { ZPICO_MAX_QUERYABLES = "4" }') in kinds)
    chk("the endpoint names `locator`", any(f[4] == "endpoint" for f in found))
    chk("CONFIG_NROS, a Zephyr symbol, a comment and an undefined symbol pass",
        len(found) == 5 and not any(f[0] == "examples/b/prj.conf" for f in found))
    lines = verdict(found, {}, ROOT)
    chk("an unbaselined offender FAILS", bool(lines) and "write in" in "\n".join(lines))
    chk("a matching baseline passes", verdict(found, counts(found), ROOT) == [])
    fell = verdict(found[:2], counts(found), ROOT)
    chk("a drained line not recorded in the baseline FAILS, naming the edit",
        any("replace:" in x for x in fell))
    with tempfile.TemporaryDirectory() as td:
        p = os.path.join(td, "b.txt")
        open(p, "w").write(render_baseline(counts(found)))
        chk("the baseline round-trips", load_baseline(p) == counts(found))
    if verbose or fail:
        print(f"check-leaf-conf-nros-knobs selftest: {ok} ok, {fail} failed")
    return 1 if fail else 0


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--write-baseline", action="store_true",
                    help="rewrite the baseline from the tree (only to record lines that moved)")
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("--list", action="store_true", help="print every offending line")
    args = ap.parse_args()
    if args.selftest:
        return selftest(verbose=True)
    if selftest():
        return 1

    model, pairs = model_and_pairs()
    found = findings(tracked_confs(ROOT), read_file(ROOT), model, pairs)
    if args.list:
        for path, n, sym, val, kind, spelling in found:
            print(f"{path}:{n}: {sym}={val} [{kind}] -> {spelling}")
        return 0
    if args.write_baseline:
        with open(BASELINE, "w", encoding="utf8") as fh:
            fh.write(render_baseline(counts(found)))
        print(f"check-leaf-conf-nros-knobs: baseline written — {len(found)} line(s) in "
              f"{len(counts(found))} file(s).")
        return 0
    lines = verdict(found, load_baseline(), ROOT)
    if lines:
        for line in lines:
            print(line, file=sys.stderr)
        return 1
    print(f"check-leaf-conf-nros-knobs: OK — {len(found)} baselined line(s) in "
          f"{len(counts(found))} file(s) left for phase-481 W3, nothing new.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
