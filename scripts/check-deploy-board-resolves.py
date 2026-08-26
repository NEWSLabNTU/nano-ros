#!/usr/bin/env python3
"""issue 0606 — every `[deploy.*].board` in the tree resolves to ONE descriptor.

`[deploy.<name>].board` carries the DOWNSTREAM ecosystem's board id: Zephyr's
`native_sim/native/64`, PlatformIO's `esp32dev`, NuttX's `qemu-armv7a-nsh`. A
descriptor's `names` is what nano-ros calls the board. Most values are in both,
which is why the gap stayed invisible: `BoardCatalog::resolve_deploy` matched
`names` only, so the values that were NOT there resolved to nothing and
`nros sync` skipped those leaves — reporting a COUNT at the end, never a name.

Three consumers had each grown their own directory fallback before this was
filed. The rule now lives in one place (`resolve_deploy`: names, then the
directory alias, then the platform) and the descriptors carry the downstream
ids they cover. This gate keeps that true: a new deploy value that no
descriptor claims fails HERE, naming it, instead of becoming a silent skip
three layers down.

Buildless: reads the descriptors and every `system.toml` / entry `Cargo.toml`.
"""

import glob
import os
import sys
import sys as _sys
from pathlib import Path as _Path
_sys.path.insert(0, str(_Path(__file__).resolve().parent / "lib"))
from tracked import tracked  # issue 0721: index lookup, not a walk


try:
    import tomllib
except ModuleNotFoundError:  # 3.10 backport, as the sibling gates spell it
    import tomli as tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))


def descriptors():
    """`alias -> {directory}`, from `names` plus the directory itself."""
    out = {}
    for path in sorted(glob.glob(os.path.join(ROOT, "packages/boards/*/nros-board.toml"))):
        with open(path, "rb") as fh:
            doc = tomllib.load(fh)
        dir_name = os.path.basename(os.path.dirname(path))
        alias = dir_name[len("nros-board-"):] if dir_name.startswith("nros-board-") else dir_name
        for entry in doc.get("board", []):
            for name in list(entry.get("names", [])) + [alias]:
                out.setdefault(name, set()).add(alias)
    return out


def deploy_values():
    """`board value -> [where it is declared]`, both site homes."""
    out = {}
    # issue 0721 / 0726 — index, not walk; same reason as the Cargo.toml scan
    # below. This one is the site the WIDENED gate caught after the other was
    # converted, which is the argument for widening it: the file had two
    # recursive globs and fixing the one I had measured would have left the
    # other paying the same cold-walk cost.
    for path in tracked("examples", "packages", name="system.toml"):
        if True:
            try:
                with open(path, "rb") as fh:
                    doc = tomllib.load(fh)
            except Exception:
                continue
            for name, blk in (doc.get("deploy") or {}).items():
                if isinstance(blk, dict) and blk.get("board"):
                    out.setdefault(blk["board"], []).append(
                        f"{os.path.relpath(path, ROOT)} [deploy.{name}]"
                    )
    # Standalone leaves: the deploy KEY is the board (no `board =` there).
    # issue 0721 / 0726 — the INDEX, not a walk. `examples/` holds build output
    # (measured 5769 Cargo.toml on disk against 237 git tracks), and a recursive
    # glob must descend every `target/` and `build-*/` tree to produce the paths
    # it then discards. Warm that costs seconds; cold it is minutes, and this
    # gate was measured at 23-24 MINUTES inside `check-fast -P32` twice.
    for path in tracked("examples", "packages/testing", name="Cargo.toml"):
        if True:
            try:
                with open(path, "rb") as fh:
                    doc = tomllib.load(fh)
            except Exception:
                continue
            nros = (doc.get("package", {}).get("metadata", {}) or {}).get("nros", {}) or {}
            key = (nros.get("entry") or {}).get("deploy")
            if key:
                out.setdefault(key, []).append(
                    f"{os.path.relpath(path, ROOT)} [entry] deploy"
                )
            for dname, blk in (nros.get("deploy") or {}).items():
                if isinstance(blk, dict) and blk.get("board"):
                    out.setdefault(blk["board"], []).append(
                        f"{os.path.relpath(path, ROOT)} [deploy.{dname}]"
                    )
    return out


def main():
    known = descriptors()
    values = deploy_values()
    if not values:
        sys.exit("check-deploy-board-resolves: found no [deploy.*] values — wrong root?")

    unknown, ambiguous = [], []
    for value, wheres in sorted(values.items()):
        dirs = known.get(value)
        if not dirs:
            unknown.append((value, wheres))
        elif len(dirs) > 1:
            ambiguous.append((value, sorted(dirs), wheres))

    if unknown or ambiguous:
        sys.stderr.write("check-deploy-board-resolves: FAILED\n")
        for value, wheres in unknown:
            sys.stderr.write(f"  `{value}` — no descriptor claims it\n")
            for w in wheres[:3]:
                sys.stderr.write(f"      {w}\n")
        for value, dirs, wheres in ambiguous:
            sys.stderr.write(f"  `{value}` — claimed by {len(dirs)}: {', '.join(dirs)}\n")
            for w in wheres[:2]:
                sys.stderr.write(f"      {w}\n")
        sys.stderr.write(
            "\n  A `[deploy.*].board` names the DOWNSTREAM ecosystem's board (Zephyr's\n"
            "  `native_sim/native/64`, PlatformIO's `esp32dev`, NuttX's `qemu-armv7a-nsh`).\n"
            "  The nano-ros descriptor that covers it must CLAIM that spelling in its\n"
            "  `names`, or the deploy resolves to nothing and `nros sync` skips the leaf\n"
            "  with a count rather than a name (issue 0606).\n"
        )
        return 1

    print(
        f"deploy boards resolve: OK ({len(values)} distinct value(s), "
        f"{len(known)} descriptor alias(es))"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
