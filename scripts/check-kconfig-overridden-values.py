#!/usr/bin/env python3
"""A Kconfig line a later fragment overrides is a line that does nothing.

Issue 0876, the half no build could catch. Zephyr merges `CONF_FILE` fragments
LAST-WINS, so a leaf that sets a symbol which a shared board or line fragment
also sets never reaches the image with its own value — and nothing anywhere says
so. Someone then edits that line, measures the board where it is dead, and
reports a saving the fixture set never had.

Concretely: phase-391 W3 changed `CONFIG_HEAP_MEM_POOL_SIZE` in the C talker's
conf and measured the result on mps2_an385. `packages/boards/nros-board-mps2-an385-zephyr/prj.conf`
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
value). A new tuple fails; an unchanged one passes; a recorded tuple the tree no
longer has ALSO fails until its entry is deleted (phase-472 W9,
`scripts/lib/ratchet.py`) — a stale entry is an override the tree could
re-acquire silently, which is the same line going dead twice. Refresh
deliberately with `--update`, and say in the commit why the override is correct.

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
from pathlib import Path

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
from ratchet import fell_instructions, judge  # noqa: E402  phase-472 W9
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


# The west platforms a `[[workspace_fixture]]` row builds through `nros build`
# — `scripts/build/fixtures-manifest.py`'s `ZEPHYR_WEST_PLATFORMS`.
ZEPHYR_WEST_PLATFORMS = ("zephyr", "zephyr-cortex-m", "zephyr-nucleo-f767zi")
DTS_EXTENSIONS = (".overlay", ".dts", ".dtsi")


def _load_toml(path):
    try:
        import tomllib  # Python 3.11+
    except ModuleNotFoundError:
        import tomli as tomllib
    with open(path, "rb") as fh:
        return tomllib.load(fh)


def _image(system, image_id):
    """`[image.<id>]` over the `[image]` base table, the specific block winning
    per field (`ImageBlock::with_base`)."""
    images = system.get("image") or {}
    base = {k: v for k, v in images.items() if not isinstance(v, dict)}
    spec = images.get(image_id)
    if not isinstance(spec, dict):
        return None
    return {**base, **spec}


def image_conf_files(bringup, image, west_board, board_dir):
    """The fragments `nros build` merges for one west image, in merge order.

    The same composition `cmd/build.rs` hands Zephyr, minus the generated
    image rendering (merged last, and owned by nobody to edit):

    1. the bringup's per-board `prj.conf` — `<bringup>/boards/<west board>/`
       is `APPLICATION_CONFIG_DIR` (`builder::zephyr::resolve_in`), so its
       `prj.conf` is Zephyr's CONF_FILE and merges before every extra;
    2. the board package's own `prj.conf` and `boards/<flat>.conf`
       (`board_projection::board_files`), ahead of the image's fragments;
    3. the image's `os_config` fragments, each looked up in the per-board
       directory, then the bringup (`resolve_fragment`).
    """
    config_dir = os.path.join(bringup, "boards", west_board.replace("/", "_"))
    files = []
    base = os.path.join(config_dir, "prj.conf")
    if not os.path.isdir(config_dir):
        base = os.path.join(bringup, "prj.conf")
    files.append(base)
    if board_dir:
        flat = re.sub(r"[/@.]", "_", west_board)
        files.append(os.path.join(board_dir, "prj.conf"))
        files.append(os.path.join(board_dir, "boards", f"{flat}.conf"))
    for name in image.get("os_config") or []:
        if name.endswith(".toml") or name.endswith(DTS_EXTENSIONS):
            continue
        hit = next((c for c in (os.path.join(config_dir, name), os.path.join(bringup, name))
                    if os.path.isfile(c)), os.path.join(bringup, name))
        files.append(os.path.normpath(hit))
    return files


def workspace_rows(manifest=None, root=ROOT, index=None):
    """[(build_name, bringup, [abs conf paths])] for every west
    `[[workspace_fixture]]` row — the rows `fixture_rows` cannot see.

    The leaf records emit `[[fixture]]` rows only, so when phase-531 W6 moved
    the Zephyr talkers to workspace rows the boot-report image's
    `CONFIG_NET_LOG=n` override left this gate's reach (issue 0196's shape).
    """
    from board_registry import descriptor_index

    manifest = manifest or os.path.join(root, "examples", "fixtures.toml")
    index = index if index is not None else descriptor_index(Path(root))
    rows = []
    for row in _load_toml(manifest).get("workspace_fixture", []):
        if row.get("platform") not in ZEPHYR_WEST_PLATFORMS:
            continue
        image_id, west_board = row.get("image"), row.get("board")
        if not image_id or not west_board:
            continue
        bringup = os.path.join(root, row["dir"], row.get("bringup", ""))
        system_toml = os.path.join(bringup, "system.toml")
        if not os.path.isfile(system_toml):
            continue
        image = _image(_load_toml(system_toml), image_id)
        if image is None:
            continue
        key = index["by_name"].get(image.get("board", ""))
        board_dir = os.path.join(root, os.path.dirname(key[0])) if key else None
        rows.append((row.get("west_build_name") or row["id"], bringup,
                     image_conf_files(bringup, image, west_board, board_dir)))
    return rows


def load_baseline():
    if not os.path.exists(BASELINE):
        return None
    with open(BASELINE, encoding="utf8") as fh:
        return json.load(fh)["overrides"]


def key(d):
    return (d["symbol"], d["dead_in"], d["beaten_by"], d["dead_value"], d["live_value"])


def _spell(k):
    return (f"{{symbol: {k[0]}, dead_in: {k[1]}, beaten_by: {k[2]}, "
            f"dead_value: {k[3]}, live_value: {k[4]}}}")


def verdict(current, base):
    """(new overrides, failure lines for entries the tree no longer has).

    One comparison on the shared ratchet: the baseline is a SET, so each key
    counts 1 — a new override is a rise, a vanished one a fall, and a fall must
    be recorded in the change that made it, like any other ratchet's.
    """
    have, want = {key(d): d for d in current}, {key(d): d for d in base}
    rose, fell = judge({k: 1 for k in have}, {k: 1 for k in want})
    new = [have[m.key] for m in rose]
    stale = fell_instructions(
        fell, rel(BASELINE, ROOT),
        lambda k, n: _spell(k) if n else None,
        "python3 scripts/check-kconfig-overridden-values.py --update") if fell else []
    return new, stale


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--update", action="store_true",
                    help="rewrite the baseline from the tree (deliberate act; say why)")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args()

    if args.selftest:
        return selftest(verbose=True)
    selftest()

    current = overrides(fixture_rows() + workspace_rows())

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

    new, stale = verdict(current, base)

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

    if stale:
        print("check-kconfig-overridden-values: the baseline records override(s) "
              "the tree no longer has:\n", file=sys.stderr)
        for line in stale:
            print(line, file=sys.stderr)
        return 1

    print(f"check-kconfig-overridden-values OK — {len(current)} known override(s), "
          f"every one still present.")
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

    # The ratchet, through `verdict` — the function `main` runs. The stale
    # case is the phase-472 W9 hole: a vanished override used to print OK and
    # leave its entry, so the same line could go dead again unreported.
    one = overrides(row("/p/leaf.conf", "/p/board.conf"), read=read, root="/p")
    chk("an unchanged set passes", verdict(one, one) == ([], []))
    chk("a new override fails", verdict(one, [])[0] == one)
    chk("a baseline entry the tree no longer has fails, naming the entry",
        verdict([], one)[1] != [] and any("CONFIG_A" in ln for ln in verdict([], one)[1]))

    # The workspace road (phase-531 W6): the composition `workspace_rows`
    # feeds the same finder. Built on disk, because the lookups are file
    # existence — the per-board directory decides where CONF_FILE comes from.
    import tempfile
    with tempfile.TemporaryDirectory() as tmp:
        bringup = os.path.join(tmp, "bringup")
        cfg = os.path.join(bringup, "boards", "mps2_an385")
        board = os.path.join(tmp, "board")
        os.makedirs(cfg)
        os.makedirs(os.path.join(board, "boards"))
        for path, text in ((os.path.join(cfg, "prj.conf"), "CONFIG_NET_LOG=y\n"),
                           (os.path.join(bringup, "prj-zenoh.conf"), "CONFIG_X=1\n"),
                           (os.path.join(board, "prj.conf"), "CONFIG_X=2\n"),
                           (os.path.join(tmp, "quiet.conf"), "CONFIG_NET_LOG=n\n")):
            with open(path, "w", encoding="utf8") as fh:
                fh.write(text)
        image = {"os_config": ["prj-zenoh.conf", "../quiet.conf", "x.overlay", "k.toml"]}
        files = image_conf_files(bringup, image, "mps2/an385", board)
        chk("a workspace image merges per-board prj.conf, board package, then os_config",
            [rel(f, tmp) for f in files] == [
                "bringup/boards/mps2_an385/prj.conf", "board/prj.conf",
                "board/boards/mps2_an385.conf", "bringup/prj-zenoh.conf", "quiet.conf"])
        found = {(d["symbol"], d["dead_in"], d["beaten_by"])
                 for d in overrides([("ws", bringup, files)], root=tmp)}
        chk("an image fragment overriding the per-board prj.conf is found",
            ("CONFIG_NET_LOG", "bringup/boards/mps2_an385/prj.conf", "quiet.conf") in found)
        chk("an image fragment overriding the BOARD package is found, the board losing",
            ("CONFIG_X", "board/prj.conf", "bringup/prj-zenoh.conf") in found)
        chk("devicetree and .toml entries are not Kconfig fragments",
            not any(f.endswith((".overlay", ".toml")) for f in files))

        # The reach itself: a manifest row reaches the composition, and a
        # non-west or image-less row does not.
        with open(os.path.join(bringup, "system.toml"), "w", encoding="utf8") as fh:
            fh.write('[image]\nrmw = "zenoh"\n[image.mps2]\nboard = "b"\n'
                     'os_config = ["prj-zenoh.conf", "../quiet.conf"]\n')
        manifest = os.path.join(tmp, "fixtures.toml")
        with open(manifest, "w", encoding="utf8") as fh:
            fh.write('[[workspace_fixture]]\nid = "w"\nplatform = "zephyr-cortex-m"\n'
                     'dir = "."\nbringup = "bringup"\nimage = "mps2"\nboard = "mps2/an385"\n'
                     'west_build_name = "build-w"\n'
                     '[[workspace_fixture]]\nid = "n"\nplatform = "native"\n'
                     'dir = "."\nbringup = "bringup"\nimage = "mps2"\nboard = "mps2/an385"\n')
        idx = {"by_name": {"b": ("board/nros-board.toml", 0)}}
        ws = workspace_rows(manifest, root=tmp, index=idx)
        chk("a west workspace row is reached, a native one is not",
            [r[0] for r in ws] == ["build-w"])
        chk("the reached row carries the image's composition",
            ws and [rel(f, tmp) for f in ws[0][2]] == [rel(f, tmp) for f in files])

    if verbose:
        print(f"\n{ok} passed, {fail} failed")
    if fail:
        print("check-kconfig-overridden-values self-test: FAILED", file=sys.stderr)
        raise SystemExit(1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
