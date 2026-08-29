#!/usr/bin/env python3
"""A Kconfig line a later fragment overrides is a line that does nothing.

Issue 0876, the half no build could catch. Zephyr merges `CONF_FILE` fragments
LAST-WINS, so a leaf that sets a symbol which a shared board or line fragment
also sets never reaches the image with its own value — and nothing anywhere says
so. Someone then edits that line, measures the board where it is dead, and
reports a saving the fixture set never had.

Concretely: phase-391 W3 changed `CONFIG_HEAP_MEM_POOL_SIZE` in the C talker's
conf and measured the result on mps2_an385. `cmake/zephyr/mps2-an385.conf`
merges after it and sets the same symbol, so the edit changed nothing there. Its
only live effect was on native_sim, where it broke the build.

WHAT THIS IS, AND IS NOT

A RATCHET, not a prohibition. Shared board fragments are legitimately an
override layer — mps2_an385 genuinely wants smaller net buffers than the leaf
asks for — so "no symbol may ever be overridden" would be false. What must not
happen silently is the set CHANGING: a new override appearing, or an existing
one changing the value it discards. Both mean somebody edited a line whose
effect is not what the file suggests.

The baseline records (symbol, loser file, winner file, loser value, winner
value). A new tuple fails; an unchanged one passes. Refresh deliberately with
`--update`, and say in the commit why the override is correct.

It would not have BLOCKED W3 — that pair already existed, and only the discarded
value changed, which the baseline does record. What it buys is that anyone
touching such a line is TOLD the line is dead on some board, which is the fact
whose absence made the change look safe.

Usage::

    check-kconfig-overridden-values.py [--update] [--selftest]
"""

import argparse
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
BASELINE = os.path.join(ROOT, ".config", "kconfig-overrides.json")
LEAVES = os.path.join(ROOT, "scripts", "build", "zephyr-fixture-leaves.sh")

ASSIGN = re.compile(r"^\s*(CONFIG_[A-Za-z0-9_]+)\s*=\s*(.+?)\s*$")


def parse_fragment(path):
    """{symbol: value} for one .conf, or None when it does not exist."""
    try:
        fh = open(path, encoding="utf8", errors="replace")
    except OSError:
        return None
    out = {}
    with fh:
        for line in fh:
            if line.lstrip().startswith("#"):
                continue
            m = ASSIGN.match(line)
            if m:
                out[m.group(1)] = m.group(2)
    return out


def overrides(rows, read=parse_fragment, root=ROOT):
    """Find every assignment a later fragment in the same CONF_FILE overrides.

    `rows` is [(build_name, src_dir, [conf paths in merge order])].
    Returns a sorted list of dicts — the baseline's own shape.
    """
    found = {}
    for build_name, _src, files in rows:
        seen = {}
        for path in files:
            frag = read(path)
            if frag is None:
                continue
            for sym, val in frag.items():
                if sym in seen and seen[sym][1] != val:
                    loser_path, loser_val = seen[sym]
                    key = (sym, rel(loser_path, root), rel(path, root), loser_val, val)
                    found.setdefault(key, set()).add(build_name)
                seen[sym] = (path, val)
    out = [
        {
            "symbol": k[0], "dead_in": k[1], "beaten_by": k[2],
            "dead_value": k[3], "live_value": k[4], "rows": sorted(v),
        }
        for k, v in found.items()
    ]
    return sorted(out, key=lambda d: (d["dead_in"], d["symbol"]))


def rel(path, root):
    try:
        return os.path.relpath(path, root)
    except ValueError:
        return path


def fixture_rows():
    """[(build_name, src_dir, [abs conf paths])] from the leaf records."""
    out = subprocess.run(
        ["bash", LEAVES, "--emit", "records"],
        cwd=ROOT, capture_output=True, text=True, timeout=600,
    )
    if out.returncode != 0:
        raise SystemExit(
            "check-kconfig-overridden-values: could not read the zephyr leaf "
            f"records (exit {out.returncode}).\n{out.stderr.strip()[:400]}"
        )
    rows = []
    for line in out.stdout.splitlines():
        f = line.split("\t")
        if len(f) < 18 or f[0] != "fixture":
            continue
        build_name, src_dir, conf = f[10], f[9], f[16]
        if not conf:
            continue
        files = [n if os.path.isabs(n) else os.path.join(src_dir, n)
                 for n in conf.split(";")]
        rows.append((build_name, src_dir, files))
    return rows


def load_baseline():
    if not os.path.exists(BASELINE):
        return None
    with open(BASELINE, encoding="utf8") as fh:
        return json.load(fh)["overrides"]


def key(d):
    return (d["symbol"], d["dead_in"], d["beaten_by"], d["dead_value"], d["live_value"])


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--update", action="store_true",
                    help="rewrite the baseline from the tree (deliberate act; say why)")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()

    if args.selftest:
        return selftest(verbose=True)
    selftest()

    current = overrides(fixture_rows())

    if args.update:
        os.makedirs(os.path.dirname(BASELINE), exist_ok=True)
        with open(BASELINE, "w", encoding="utf8") as fh:
            json.dump({
                "_comment": (
                    "Kconfig assignments a LATER fragment in the same CONF_FILE "
                    "overrides — see scripts/check-kconfig-overridden-values.py. "
                    "Each entry is a line that does NOTHING for the listed rows. "
                    "Regenerate with --update, and say in the commit why the "
                    "override is correct."
                ),
                "overrides": current,
            }, fh, indent=2, sort_keys=True)
            fh.write("\n")
        print(f"check-kconfig-overridden-values: baseline written — {len(current)} override(s).")
        return 0

    base = load_baseline()
    if base is None:
        print(f"check-kconfig-overridden-values: no baseline at "
              f"{rel(BASELINE, ROOT)} — create it with --update.", file=sys.stderr)
        return 1

    have, want = {key(d): d for d in current}, {key(d): d for d in base}
    new = [have[k] for k in have.keys() - want.keys()]
    gone = [want[k] for k in want.keys() - have.keys()]

    if new:
        print("check-kconfig-overridden-values: a Kconfig line has become DEAD, "
              "or a dead one changed:\n", file=sys.stderr)
        for d in sorted(new, key=lambda x: (x["dead_in"], x["symbol"]))[:25]:
            print(f"  {d['symbol']}", file=sys.stderr)
            print(f"     set    {d['dead_value']:<12} in {d['dead_in']}", file=sys.stderr)
            print(f"     BEATEN by {d['live_value']:<9} in {d['beaten_by']}", file=sys.stderr)
            print(f"     affects {len(d['rows'])} row(s), e.g. {d['rows'][0]}", file=sys.stderr)
        print(
            "\n  Zephyr merges CONF_FILE fragments LAST-WINS, so the first value\n"
            "  above never reaches the image for those rows. Editing it changes\n"
            "  nothing there — which is issue 0876: a heap size was tuned and\n"
            "  measured on the one board where the line was already dead.\n"
            "\n"
            "  Either move the setting to a fragment that merges later, or accept\n"
            "  the override and record it:\n"
            "      python3 scripts/check-kconfig-overridden-values.py --update\n"
            "  and say in the commit why the override is the intended behaviour.",
            file=sys.stderr,
        )
        return 1

    msg = f"check-kconfig-overridden-values OK — {len(current)} known override(s)"
    if gone:
        msg += (f"; {len(gone)} baseline entr(y/ies) no longer present "
                f"(fine — refresh with --update when convenient)")
    print(msg + ".")
    return 0


def selftest(verbose=False):
    """Prove the finder can fail. Runs on every invocation."""
    ok = fail = 0

    def chk(desc, cond):
        nonlocal ok, fail
        if verbose or not cond:
            print(f"  {'ok   ' if cond else 'FAIL '} {desc}")
        ok += 1 if cond else 0
        fail += 0 if cond else 1

    frags = {
        "/p/leaf.conf": {"CONFIG_A": "1", "CONFIG_B": "2"},
        "/p/board.conf": {"CONFIG_A": "9"},
        "/p/same.conf": {"CONFIG_B": "2"},
        "/p/other.conf": {"CONFIG_C": "3"},
    }
    read = frags.get
    row = lambda *f: [("cell", "/p", list(f))]  # noqa: E731

    r = overrides(row("/p/leaf.conf", "/p/board.conf"), read=read, root="/p")
    chk("an overridden value is found", len(r) == 1 and r[0]["symbol"] == "CONFIG_A")
    chk("it records BOTH values, not just the symbol",
        r[0]["dead_value"] == "1" and r[0]["live_value"] == "9")
    chk("it names the losing and winning files",
        r[0]["dead_in"] == "leaf.conf" and r[0]["beaten_by"] == "board.conf")
    chk("re-stating the SAME value is not an override",
        overrides(row("/p/leaf.conf", "/p/same.conf"), read=read, root="/p") == [])
    chk("disjoint fragments produce nothing",
        overrides(row("/p/leaf.conf", "/p/other.conf"), read=read, root="/p") == [])
    chk("merge ORDER matters — reversed, the other value is the dead one",
        overrides(row("/p/board.conf", "/p/leaf.conf"), read=read, root="/p")[0]["dead_value"] == "9")
    chk("a missing fragment is skipped, not crashed on",
        overrides(row("/p/leaf.conf", "/p/nope.conf"), read=read, root="/p") == [])
    # The property the ratchet turns on: W3 changed only the DISCARDED value.
    a = overrides(row("/p/leaf.conf", "/p/board.conf"), read=read, root="/p")
    frags2 = dict(frags, **{"/p/leaf.conf": {"CONFIG_A": "0", "CONFIG_B": "2"}})
    b = overrides(row("/p/leaf.conf", "/p/board.conf"), read=frags2.get, root="/p")
    chk("changing only the DEAD value yields a different baseline key",
        key(a[0]) != key(b[0]))
    rows2 = row("/p/leaf.conf", "/p/board.conf") + [("cell2", "/p", ["/p/leaf.conf", "/p/board.conf"])]
    chk("two rows hitting the same override collapse to one entry, both listed",
        len(overrides(rows2, read=read, root="/p")) == 1
        and overrides(rows2, read=read, root="/p")[0]["rows"] == ["cell", "cell2"])

    if verbose:
        print(f"\n{ok} passed, {fail} failed")
    if fail:
        print("check-kconfig-overridden-values self-test: FAILED", file=sys.stderr)
        raise SystemExit(1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
