#!/usr/bin/env python3
"""phase-351 W2, rewritten by phase-484 W3 — a project's site config states only
what nothing else can.

RFC-0072 §5 splits board information into board FACTS (the board package), SITE
config (`[board_config.<board>]` in a `system.toml`), and test-harness config.
Phase-351 put every in-tree board's SDK roots in the site block as
`sdk = { freertos = "{env:FREERTOS_DIR}", … }` and this gate kept that copy in
step with `just/sdk-env.just`. Phase-484 W3 (RFC-0103 D4) DERIVES those roots
instead: the index's `[board.<name>]` lists the sources a board needs and
`nros ws board-facts` locates each through the one ladder. So the rule inverted:

  S1  a site block must NOT restate an index source's own `env` variable as
      `{env:VAR}` — that value is derived, and the copy read the variable raw
      (no re-root, issue 1280). A project's OWN tree (a vendor SDK under its
      own name) is still stated here;
  S3  the declared `netstack` is inside the board's declared domain;
  S4  a site block for a board the file never builds is dead config.

Buildless: TOML only.
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

INDEX = os.path.join(ROOT, "nros-sdk-index.toml")


def index_source_envs():
    """`env` names the index declares on `[source.*]` rows — derived roots."""
    with open(INDEX, "rb") as fh:
        doc = tomllib.load(fh)
    return {
        row["env"]: name
        for name, row in (doc.get("source") or {}).items()
        if isinstance(row, dict) and row.get("env")
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
    declares two distinct boards (`qemu-armv7a-nuttx` and `rv-virt-nuttx`), so
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
            canonical = min(spellings) if spellings else None
            if canonical is None:
                continue
            for sp in spellings:
                out[sp] = canonical
    return out


def system_tomls():
    out = []
    # issue 0721 / 0726 — index, not walk. Same hazard as
    # check-deploy-board-resolves: `examples/` and `packages/` are the two trees
    # holding build output, so a recursive glob pays for every target/ tree to
    # find a handful of tracked files.
    out += [str(q) for q in tracked("examples", "packages", name="system.toml")]
    return sorted(out)


def main():
    argparse.ArgumentParser().parse_args()
    derived = index_source_envs()
    netstacks = board_netstacks()
    if not derived:
        sys.exit("check-site-config: no [source.*] env names in nros-sdk-index.toml")

    aliases = board_aliases()
    problems, checked = [], 0

    for path in system_tomls():
        rel = os.path.relpath(path, ROOT)
        with open(path, "rb") as fh:
            try:
                doc = tomllib.load(fh)
            except Exception as e:  # noqa: BLE001 — report, do not raise
                problems.append(f"{rel}: not valid TOML: {e}")
                continue

        # Which boards does this file build for? Any board a deploy or an
        # image names (`[image.*]` is the buildable unit, RFC-0065 D6).
        in_scope = {}
        for table in ("deploy", "image"):
            for name, blk in (doc.get(table) or {}).items():
                board = aliases.get(blk.get("board"))
                if board is not None:
                    in_scope.setdefault(board, f"[{table}.{name}]")

        site_blocks = doc.get("board_config") or {}
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

        for board, (key, site) in sorted(by_board.items()):
            checked += 1
            section = f"board_config.{key}"

            # S1 — no restatement of a derived root.
            for sdk_key, val in (site.get("sdk") or {}).items():
                for var in re.findall(r"\{env:([A-Z0-9_]+)\}", str(val)):
                    if var in derived:
                        problems.append(
                            f"{rel}: [{section}].sdk.{sdk_key} = {val!r} restates "
                            f"`[source.{derived[var]}]`'s own ${var}. That root is "
                            f"DERIVED (the index lists the board's sources and "
                            f"`nros ws board-facts` locates them, RFC-0103 D4) — "
                            f"delete the entry; to use another tree, set ${var} "
                            f"(the row's one override name)."
                        )

            # S3 — phase-351 W4: the netstack is inside the BOARD's domain.
            stacks = netstacks.get(board, [])
            want = site.get("netstack")
            if want is not None and want not in stacks:
                problems.append(
                    f"{rel}: [{section}].netstack = {want!r}, which board "
                    f"`{board}` does not support. Its descriptor declares: "
                    + (", ".join(stacks) if stacks else
                       "NONE (its RTOS or host owns the stack — drop the key)")
                )

            # S4 — a site block for a board this file never builds is dead.
            if board not in in_scope:
                problems.append(
                    f"{rel}: [board_config.{key}] describes a board no "
                    f"[deploy.*] or [image.*] in this file targets"
                )

    if problems:
        sys.stderr.write("check-site-config: FAILED\n")
        for p in problems:
            sys.stderr.write(f"  {p}\n")
        return 1

    print(
        f"site config: OK ({checked} board_config block(s); none restates a derived "
        f"root ({len(derived)} index env names); netstacks inside the domain "
        f"declared by {len(netstacks)} board name(s))"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
