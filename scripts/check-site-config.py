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

try:
    import tomllib  # 3.11+
except ModuleNotFoundError:  # 3.10 backport, as the sibling gates spell it
    import tomli as tomllib

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SDK_ENV = os.path.join(ROOT, "just/sdk-env.just")

# board -> (sdk name -> env var), plus the netstack that board runs in-tree.
#
# Derived from what each platform's build actually reads, not from a wish list:
# Zephyr and the bare-metal boards appear nowhere because their RTOS (or a Rust
# crate) owns the stack and nano-ros needs no SDK root from the site.
BOARDS = {
    "mps2-an385-freertos": (
        {"freertos": "FREERTOS_DIR", "lwip": "LWIP_DIR"},
        "lwip",
    ),
    "nuttx-qemu-arm": ({"nuttx": "NUTTX_DIR", "nuttx_apps": "NUTTX_APPS_DIR"}, None),
    "nuttx-qemu-riscv": ({"nuttx": "NUTTX_DIR", "nuttx_apps": "NUTTX_APPS_DIR"}, None),
    "qemu-armv7a-nsh": ({"nuttx": "NUTTX_DIR", "nuttx_apps": "NUTTX_APPS_DIR"}, None),
    "threadx-linux": (
        {"threadx": "THREADX_DIR", "netxduo": "NETX_DIR"},
        "netxduo",
    ),
}


def exported_vars():
    """Names `just/sdk-env.just` exports."""
    with open(SDK_ENV, encoding="utf-8") as fh:
        return set(re.findall(r"^export\s+([A-Z0-9_]+)\s*:=", fh.read(), re.M))


def system_tomls():
    out = []
    for pat in ("examples/**/system.toml", "packages/**/system.toml"):
        out += glob.glob(os.path.join(ROOT, pat), recursive=True)
    return sorted(out)


def render_block(deploy, sdk_map, netstack):
    lines = [
        "",
        f"# phase-351 W2 — SITE config for this deploy (RFC-0072 §5): where THIS",
        f"# checkout's SDKs live and which stack it uses. Generated from",
        f"# `just/sdk-env.just`; `scripts/check-site-config.py` keeps them in step.",
        f"# `{{env:…}}` rather than a literal so the value survives a second checkout.",
        f"[deploy.{deploy}.nros]",
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
    if not exported:
        sys.exit("check-site-config: no exports found in just/sdk-env.just")

    problems, wrote, checked = [], 0, 0

    for path in system_tomls():
        rel = os.path.relpath(path, ROOT)
        with open(path, "rb") as fh:
            try:
                doc = tomllib.load(fh)
            except Exception as e:  # noqa: BLE001 — report, do not raise
                problems.append(f"{rel}: not valid TOML: {e}")
                continue

        missing = []
        for name, blk in (doc.get("deploy") or {}).items():
            board = blk.get("board")
            if board not in BOARDS:
                continue
            checked += 1
            sdk_map, netstack = BOARDS[board]
            site = blk.get("nros")

            if site is None:
                missing.append((name, sdk_map, netstack))
                continue

            # S1 — every needed root declared, and as {env:VAR}
            declared = site.get("sdk") or {}
            for key, var in sdk_map.items():
                got = declared.get(key)
                if got is None:
                    problems.append(
                        f"{rel}: [deploy.{name}.nros].sdk is missing `{key}` "
                        f"(board `{board}` needs it; expected \"{{env:{var}}}\")"
                    )
                elif got != f"{{env:{var}}}":
                    problems.append(
                        f"{rel}: [deploy.{name}.nros].sdk.{key} = {got!r}, expected "
                        f'"{{env:{var}}}" — a literal path does not survive a '
                        f"second checkout"
                    )

            # S2 — every referenced var is really exported
            for key, val in declared.items():
                for var in re.findall(r"\{env:([A-Z0-9_]+)\}", str(val)):
                    if var not in exported:
                        problems.append(
                            f"{rel}: [deploy.{name}.nros].sdk.{key} names ${var}, "
                            f"which just/sdk-env.just does not export — renamed?"
                        )

            # S3 — netstack agrees with what the board runs here
            if netstack and site.get("netstack") not in (None, netstack):
                problems.append(
                    f"{rel}: [deploy.{name}.nros].netstack = "
                    f"{site.get('netstack')!r}, but board `{board}` runs "
                    f"{netstack!r} in this tree"
                )

        if missing and args.write:
            with open(path, "a", encoding="utf-8") as fh:
                for name, sdk_map, netstack in missing:
                    fh.write(render_block(name, sdk_map, netstack))
                    wrote += 1
        elif missing:
            for name, _, _ in missing:
                problems.append(
                    f"{rel}: [deploy.{name}] targets a board needing SDK roots but "
                    f"has no [deploy.{name}.nros] block — run "
                    f"`python3 scripts/check-site-config.py --write`"
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
        f"site config: OK ({checked} deploy block(s) across "
        f"{len(BOARDS)} board(s) agree with just/sdk-env.just)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
