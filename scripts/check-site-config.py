#!/usr/bin/env python3
"""phase-351 W2 — this repo's site config agrees with `just/sdk-env.just`.

RFC-0072 §5 splits board information into board FACTS (the board package), SITE
config (`[deploy.<name>.nros]` in a bringup's `system.toml`), and test-harness
config. For *this* repo the site facts already exist — as `export` lines in
`just/sdk-env.just` — and only in-tree users have them, which is precisely why
an out-of-tree user had nowhere to put SDK roots.

So the two spellings coexist during the migration, and this gate asserts they
agree. It is the phase-347 pattern: while two sources of one fact are live, the
gate is what stops them drifting.

Checks:

  S1  every board that needs an SDK root declares one, as `{env:VAR}` — never a
      literal path, which would not survive a second checkout;
  S2  every VAR a site block names is actually exported by `just/sdk-env.just`,
      so a renamed export cannot leave a dangling reference;
  S3  the declared `netstack` is one the mapping expects for that board.

`--write` renders the missing blocks instead of reporting them. Generator and
gate are one file on purpose: two programs that must agree about the same
rendering is the drift this repo keeps paying for.

Buildless: TOML plus a regex over one justfile.
"""

import argparse
import glob
import os
import re
import sys
import sys as _sys
from pathlib import Path as _Path
_sys.path.insert(0, str(_Path(__file__).resolve().parent / "lib"))
from tracked import tracked  # issue 0721: index lookup, not a walk


try:
    import tomllib  # 3.11+
except ModuleNotFoundError:  # 3.10 backport, as the sibling gates spell it
    import tomli as tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SDK_ENV = os.path.join(ROOT, "just/sdk-env.just")

# board -> sdk name -> env var.
#
# Derived from what each platform's build actually reads, not from a wish list:
# Zephyr and the bare-metal boards appear nowhere because their RTOS (or a Rust
# crate) owns the stack and nano-ros needs no SDK root from the site.
#
# phase-351 W4 — the NETSTACK column is GONE from this table. It used to name
# the stack each board runs, which made this file a second source of truth for a
# board FACT; the boards now declare `supported_netstacks` in their own
# descriptor and this gate reads it. One fact, one home (RFC-0072 §5 A vs B).
BOARDS = {
    "mps2-an385-freertos": {"freertos": "FREERTOS_DIR", "lwip": "LWIP_DIR"},
    "nuttx-qemu-arm": {"nuttx": "NUTTX_DIR", "nuttx_apps": "NUTTX_APPS_DIR"},
    "nuttx-qemu-riscv": {"nuttx": "NUTTX_DIR", "nuttx_apps": "NUTTX_APPS_DIR"},
    "qemu-armv7a-nsh": {"nuttx": "NUTTX_DIR", "nuttx_apps": "NUTTX_APPS_DIR"},
    "threadx-linux": {"threadx": "THREADX_DIR", "netxduo": "NETX_DIR"},
}


def board_netstacks():
    """`supported_netstacks` per board NAME, from the shipped descriptors.

    The descriptor is the SSoT for what a board can be built with (phase-351
    W4); this gate only asserts that a site block stays inside that domain.
    Every alias a descriptor lists maps to the same set, because `[deploy.*]`
    may name any of them.

    Keyed by the descriptor's declared `names` AND its directory, which is the
    same rule `BoardCatalog::resolve_deploy` applies (issue 0606: the field
    carries the DOWNSTREAM ecosystem's board id, the descriptor claims the
    spellings it covers, and the directory is an alias). `check-deploy-board-
    resolves` is what keeps the two in step — this gate only asks whether a
    netstack is inside the resolved board's domain.
    """
    out = {}
    for path in sorted(glob.glob(os.path.join(ROOT, "packages/boards/*/nros-board.toml"))):
        with open(path, "rb") as fh:
            doc = tomllib.load(fh)
        dir_name = os.path.basename(os.path.dirname(path))
        from_dir = dir_name[len("nros-board-"):] if dir_name.startswith("nros-board-") else dir_name
        for entry in doc.get("board", []):
            stacks = entry.get("supported_netstacks", [])
            for name in list(entry.get("names", [])) + [from_dir]:
                # A directory serves several witnesses (the two nuttx boards);
                # union rather than let the last one win.
                out.setdefault(name, [])
                for st in stacks:
                    if st not in out[name]:
                        out[name].append(st)
    return out


def board_aliases():
    """Every legal spelling of a board -> the BOARDS key it resolves to.

    `[board_config.<key>]` is matched by RESOLUTION, not by text (issue 0951),
    because a board has several legal spellings: the descriptor's `names`, its
    directory, and the downstream framework id. The Rust side does this through
    `BoardCatalog::resolve_deploy`; this is the same rule, so the gate and the
    resolver cannot disagree about which block describes which board.

    Resolution is per BOARD ENTRY, not per directory: `nros-board-nuttx/`
    declares two distinct boards (`nuttx-qemu-arm` and `nuttx-qemu-riscv`), so
    folding a directory's entries together would map both spellings onto
    whichever one was seen first — two boards collapsed into one, with the
    riscv site block silently answering for the arm build. The directory alias
    is therefore only honoured when the directory holds exactly one entry.
    """
    out = {}
    for path in sorted(glob.glob(os.path.join(ROOT, "packages/boards/*/nros-board.toml"))):
        with open(path, "rb") as fh:
            doc = tomllib.load(fh)
        dir_name = os.path.basename(os.path.dirname(path))
        from_dir = dir_name[len("nros-board-"):] if dir_name.startswith("nros-board-") else dir_name
        entries = doc.get("board", [])
        for entry in entries:
            spellings = set(entry.get("names", []))
            if len(entries) == 1:
                spellings.add(from_dir)
            # Canonicalise onto the BOARDS key when this board has one;
            # otherwise onto its own first spelling, so a board with no SDK
            # roots still RESOLVES (S1/S3 simply have nothing to say about it)
            # rather than reading as an unknown name.
            canonical = next(
                (b for b in BOARDS if b in spellings),
                min(spellings) if spellings else None,
            )
            if canonical is None:
                continue
            for sp in spellings:
                out[sp] = canonical
    return out


def exported_vars():
    """Names `just/sdk-env.just` exports."""
    with open(SDK_ENV, encoding="utf-8") as fh:
        return set(re.findall(r"^export\s+([A-Z0-9_]+)\s*:=", fh.read(), re.M))


def system_tomls():
    out = []
    # issue 0721 / 0726 — index, not walk. Same hazard as
    # check-deploy-board-resolves: `examples/` and `packages/` are the two trees
    # holding build output, so a recursive glob pays for every target/ tree to
    # find a handful of tracked files.
    out += [str(q) for q in tracked("examples", "packages", name="system.toml")]
    return sorted(out)


def render_block(board, sdk_map, netstack):
    lines = [
        "",
        f"# SITE config for this board (RFC-0072 §5): where THIS checkout keeps the",
        f"# SDK roots it builds against. Keyed by BOARD, not by a deploy or image",
        f"# name, because that is what the fact is about (issue 0951). Generated",
        f"# from `just/sdk-env.just`; this gate keeps the two in step. `{{env:…}}`",
        f"# rather than a literal so the value survives a second checkout.",
        f'[board_config."{board}"]',
    ]
    if netstack:
        lines.append(f'netstack = "{netstack}"')
    pairs = ", ".join(f'{k} = "{{env:{v}}}"' for k, v in sdk_map.items())
    lines.append(f"sdk = {{ {pairs} }}")
    return "\n".join(lines) + "\n"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--write", action="store_true", help="render missing blocks")
    args = ap.parse_args()

    exported = exported_vars()
    netstacks = board_netstacks()
    if not exported:
        sys.exit("check-site-config: no exports found in just/sdk-env.just")

    aliases = board_aliases()
    problems, wrote, checked = [], 0, 0

    for path in system_tomls():
        rel = os.path.relpath(path, ROOT)
        with open(path, "rb") as fh:
            try:
                doc = tomllib.load(fh)
            except Exception as e:  # noqa: BLE001 — report, do not raise
                problems.append(f"{rel}: not valid TOML: {e}")
                continue

        # Which boards does this file build for? Any board a deploy or an image
        # names. `[image.*]` is the buildable unit now (RFC-0065 D6), so a
        # workspace that has finished migrating off `[deploy.*]` still needs its
        # SDK roots checked.
        in_scope = {}
        for table in ("deploy", "image"):
            for name, blk in (doc.get(table) or {}).items():
                board = aliases.get(blk.get("board"))
                if board is not None:
                    in_scope.setdefault(board, f"[{table}.{name}]")
        # Only boards with SDK roots to declare are MISSING a block when they
        # have none; the rest are merely in scope, so a site block for them is
        # legitimate rather than dead.
        needs_roots = {b: c for b, c in in_scope.items() if b in BOARDS}

        site_blocks = doc.get("board_config") or {}
        # Resolve the authored keys the same way the Rust side does.
        by_board = {}
        for key, val in site_blocks.items():
            board = aliases.get(key)
            if board is None:
                problems.append(
                    f"{rel}: [board_config.{key!r}] names no known board — "
                    f"the key is a board spelling, resolved like every other "
                    f"`board = ` value"
                )
                continue
            if board in by_board:
                problems.append(
                    f"{rel}: two [board_config.*] blocks resolve to board "
                    f"`{board}` — one board, one block"
                )
                continue
            by_board[board] = (key, val)

        missing = []
        for board, cited_by in sorted(needs_roots.items()):
            sdk_map = BOARDS[board]
            stacks = netstacks.get(board, [])
            found = by_board.get(board)
            if found is None:
                missing.append((board, sdk_map, stacks[0] if stacks else None))
                continue
            key, site = found
            checked += 1
            section = f"board_config.{key}"

            # S1 — every needed root declared, and as {env:VAR}
            declared = site.get("sdk") or {}
            for sdk_key, var in sdk_map.items():
                got = declared.get(sdk_key)
                if got is None:
                    problems.append(
                        f"{rel}: [{section}].sdk is missing `{sdk_key}` "
                        f"(board `{board}` needs it; expected \"{{env:{var}}}\")"
                    )
                elif got != f"{{env:{var}}}":
                    problems.append(
                        f"{rel}: [{section}].sdk.{sdk_key} = {got!r}, expected "
                        f'"{{env:{var}}}" — a literal path does not survive a '
                        f"second checkout"
                    )

            # S2 — every referenced var is really exported
            for sdk_key, val in declared.items():
                for var in re.findall(r"\{env:([A-Z0-9_]+)\}", str(val)):
                    if var not in exported:
                        problems.append(
                            f"{rel}: [{section}].sdk.{sdk_key} names ${var}, "
                            f"which just/sdk-env.just does not export — renamed?"
                        )

            # S3 — phase-351 W4: the netstack is inside the BOARD's declared
            # domain. Not "equals the one value this script knew": a board may
            # support several, and the descriptor is what says so.
            want = site.get("netstack")
            if want is not None and want not in stacks:
                problems.append(
                    f"{rel}: [{section}].netstack = {want!r}, which board "
                    f"`{board}` does not support. Its descriptor declares: "
                    + (", ".join(stacks) if stacks else
                       "NONE (its RTOS or host owns the stack — drop the key)")
                )

        # S4 — a site block for a board this file never builds is dead config:
        # nothing resolves it, so a wrong value in it is invisible.
        for board, (key, _) in sorted(by_board.items()):
            if board not in in_scope:
                problems.append(
                    f"{rel}: [board_config.{key}] describes a board no "
                    f"[deploy.*] or [image.*] in this file targets"
                )

        if missing and args.write:
            with open(path, "a", encoding="utf-8") as fh:
                for board, sdk_map, netstack in missing:
                    fh.write(render_block(board, sdk_map, netstack))
                    wrote += 1
        elif missing:
            for board, _, _ in missing:
                problems.append(
                    f"{rel}: {needs_roots[board]} targets board `{board}`, which needs "
                    f"SDK roots, but the file has no [board_config.\"{board}\"] "
                    f"block — run `python3 scripts/check-site-config.py --write`"
                )

    if args.write:
        print(f"site config: wrote {wrote} block(s)")
        return 0

    if problems:
        sys.stderr.write("check-site-config: FAILED\n")
        for p in problems:
            sys.stderr.write(f"  {p}\n")
        return 1

    print(
        f"site config: OK ({checked} board_config block(s) across {len(BOARDS)} board(s) "
        f"agree with just/sdk-env.just; netstacks inside the domain declared by "
        f"{len(netstacks)} board name(s))"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
