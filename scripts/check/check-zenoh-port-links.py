#!/usr/bin/env python3
"""check-zenoh-port-links -- a zenoh port row's `links` agrees with zenoh-pico.

Phase-560 W9.d. `[port.<platform>] links` in the zenoh backend's
`nros-rmw.toml` lists the default-off links (`PORT_LINKS` in
`nros-platform-config/src/manifest.rs`: `can`, `isotp`) whose platform arm
that port compiles, and `nros configure` refuses an image that dials a link
its platform row does not list (`nros-cli-core/src/orchestration/link_arms.rs`).
The list is authored, and the arms are zenoh-pico's: each `_z_open_<link>`
definition under `src/link/transport/<link>/` sits inside a preprocessor guard
naming a platform macro (`ZENOH_LINUX`, `ZENOH_ZEPHYR`). A fork that adds an
arm, or drops one, would leave the list wrong in silence -- an image refused
for a link that works, or let through to `undefined reference to
_z_open_can` at its final link.

So this gate DERIVES the armed links per port and compares, in both
directions:

* the arms: for every `_z_open_<link>` definition, the `ZENOH_*` macros the
  enclosing `#if` / `#ifdef` lines require (an arm under no platform macro
  arms every port);
* a port's macros: its row's `defines` and `defines_conditional` names;
* a port arms `<link>` when one of its macros guards a `_z_open_<link>`.

A row that lists a link it has no arm for, or that leaves out one it has, is
a finding. Buildless; reads the zenoh-pico submodule, and records NOT VERIFIED
(never a pass) when it is not checked out. Self-tests on every run.

Run: python3 scripts/check/check-zenoh-port-links.py
"""

from __future__ import annotations

import re
import sys
try:
    import tomllib
except ModuleNotFoundError:  # Python < 3.11
    import tomli as tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import check_skip  # noqa: E402
from source_locate import located_or_dest  # noqa: E402

NAME = "zenoh-port-links"
ROWS = ROOT / "packages/rmw/zenoh/nros-rmw-zenoh/nros-rmw.toml"
MANIFEST_RS = ROOT / "packages/tooling/nros-platform-config/src/manifest.rs"

MACRO = re.compile(r"defined\s*\(\s*(ZENOH_\w+)\s*\)|#\s*ifdef\s+(ZENOH_\w+)")


def port_links_vocabulary(text: str) -> list[str]:
    """`PORT_LINKS` as `manifest.rs` states it -- one vocabulary."""
    m = re.search(r"pub const PORT_LINKS: &\[&str\] = &\[([^\]]*)\]", text)
    if not m:
        raise SystemExit(f"{NAME}: no `PORT_LINKS` in {MANIFEST_RS.relative_to(ROOT)}")
    return re.findall(r'"([a-z0-9_]+)"', m.group(1))


def arm_macros(c_text: str, link: str) -> list[set[str]]:
    """For each `_z_open_<link>` DEFINITION in `c_text`, the `ZENOH_*` macros
    its enclosing conditionals require (empty set: no platform macro)."""
    # A definition: the name, its parameter list, then a body (`{`), never `;`.
    define = re.compile(rf"^\w[\w \t\*]*\b_z_open_{re.escape(link)}\s*\([^;{{]*\)\s*\{{", re.M)
    def_lines = {c_text.count("\n", 0, m.start()) for m in define.finditer(c_text)}
    stack: list[set[str]] = []
    out: list[set[str]] = []
    pending = ""
    for n, raw in enumerate(c_text.splitlines()):
        line = raw.strip()
        if pending:
            pending += " " + line
            if not line.endswith("\\"):
                line, pending = pending, ""
            else:
                continue
        elif line.startswith("#") and line.endswith("\\"):
            pending = line
            continue
        if re.match(r"#\s*(if|ifdef|ifndef)\b", line):
            pos = set() if re.match(r"#\s*ifndef\b", line) else macros_of(line)
            stack.append(pos)
        elif re.match(r"#\s*elif\b", line):
            if stack:
                stack[-1] = macros_of(line)
        elif re.match(r"#\s*else\b", line):
            if stack:
                stack[-1] = set()
        elif re.match(r"#\s*endif\b", line):
            if stack:
                stack.pop()
        elif n in def_lines:
            req: set[str] = set()
            for s in stack:
                req |= s
            out.append(req)
    return out


def macros_of(line: str) -> set[str]:
    # A negated macro (`!defined(ZENOH_X)`) requires nothing.
    line = re.sub(r"!\s*defined\s*\(\s*ZENOH_\w+\s*\)", "", line)
    return {a or b for a, b in MACRO.findall(line)}


def armed(src: Path, links: list[str]) -> dict[str, list[set[str]]]:
    """link -> the macro sets of its arms, from `src/link/transport/<link>/`."""
    out: dict[str, list[set[str]]] = {}
    for link in links:
        arms: list[set[str]] = []
        d = src / "link" / "transport" / link
        for c in sorted(d.glob("*.c")) if d.is_dir() else []:
            arms += arm_macros(c.read_text(encoding="utf-8", errors="replace"), link)
        out[link] = arms
    return out


def row_macros(row: dict) -> set[str]:
    m = {d for d in row.get("defines", []) if isinstance(d, str)}
    m |= {c.get("name", "") for c in row.get("defines_conditional", [])}
    return {x for x in m if x.startswith("ZENOH_")}


def problems(rows: dict, arms: dict[str, list[set[str]]]) -> list[str]:
    bad: list[str] = []
    for name, row in sorted(rows.items()):
        mine = row_macros(row)
        derived = sorted(
            link
            for link, sets in arms.items()
            if any(not s or s & mine for s in sets)
        )
        stated = sorted(row.get("links", []))
        for link in sorted(set(stated) - set(derived)):
            bad.append(
                f"[port.{name}] links lists `{link}`, but no `_z_open_{link}` in zenoh-pico's "
                f"src/link/transport/{link}/ is armed by its macros ({', '.join(sorted(mine)) or 'none'}) "
                f"-- drop it, or the image builds and fails at its final link"
            )
        for link in sorted(set(derived) - set(stated)):
            bad.append(
                f"[port.{name}] links leaves out `{link}`, which zenoh-pico arms for its macros "
                f"({', '.join(sorted(mine))}) -- add it, or `nros configure` refuses a link that works"
            )
    return bad


def self_test() -> None:
    linux = """
#include "x.h"
#if defined(ZENOH_LINUX) && Z_FEATURE_LINK_CAN == 1
#include <linux/can.h>
static int helper(void);
z_result_t _z_open_can(_z_can_socket_t *sock, const char *dev,
                       uint32_t id) {
    return helper();
}
#endif
"""
    zephyr = """
#ifdef ZENOH_ZEPHYR
#if Z_FEATURE_LINK_CAN == 1
z_result_t _z_open_can(_z_can_socket_t *sock) { return 0; }
#endif
#else
z_result_t _z_open_can(_z_can_socket_t *sock) { return 1; }
#endif
"""
    decl = "z_result_t _z_open_can(_z_can_socket_t *sock);\n"
    assert arm_macros(linux, "can") == [{"ZENOH_LINUX"}], arm_macros(linux, "can")
    assert arm_macros(zephyr, "can") == [{"ZENOH_ZEPHYR"}, set()], arm_macros(zephyr, "can")
    assert arm_macros(decl, "can") == [], "a declaration is not an arm"
    assert macros_of("#if !defined(ZENOH_LINUX)") == set()
    arms = {"can": [{"ZENOH_LINUX"}], "isotp": [{"ZENOH_ZEPHYR"}]}
    rows = {
        "posix": {"defines_conditional": [{"name": "ZENOH_LINUX"}], "links": ["can"]},
        "zephyr": {"defines": ["ZENOH_ZEPHYR"], "links": ["isotp"]},
        "freertos": {"defines": ["ZENOH_GENERIC"]},
    }
    assert problems(rows, arms) == [], problems(rows, arms)
    # Drift in each direction is a finding.
    rows["freertos"]["links"] = ["can"]
    rows["zephyr"]["links"] = []
    got = problems(rows, arms)
    assert len(got) == 2 and "[port.freertos] links lists `can`" in got[0], got
    assert "[port.zephyr] links leaves out `isotp`" in got[1], got
    # An arm under no platform macro arms every port.
    assert problems({"freertos": {"links": []}}, {"can": [set()]})


def main() -> int:
    self_test()
    if "--self" in sys.argv[1:]:
        print(f"check-{NAME}: self-test OK")
        return 0
    links = port_links_vocabulary(MANIFEST_RS.read_text(encoding="utf-8"))
    rows = tomllib.loads(ROWS.read_text(encoding="utf-8")).get("port", {})
    src = located_or_dest("zenoh-pico") / "src"
    if not (src / "link" / "transport").is_dir():
        return check_skip.unverified(
            NAME, f"zenoh-pico is not checked out at {src.parent} (`nros setup --source zenoh-pico`)"
        )
    arms = armed(src, links)
    for link, sets in arms.items():
        if not sets:
            print(
                f"check-{NAME}: no `_z_open_{link}` found under {src}/link/transport/{link}/ -- "
                "the arm search no longer matches zenoh-pico; fix this gate",
                file=sys.stderr,
            )
            return 1
    bad = problems(rows, arms)
    if bad:
        print(f"check-{NAME}: {len(bad)} finding(s):", file=sys.stderr)
        for b in bad:
            print(f"  {b}", file=sys.stderr)
        return 1
    summary = ", ".join(
        f"{n}={sorted(r.get('links', []))}" for n, r in sorted(rows.items()) if r.get("links")
    )
    print(f"check-{NAME} OK -- port links agree with zenoh-pico's arms ({summary})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
