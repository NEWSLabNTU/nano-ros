#!/usr/bin/env python3
"""Phase-287 W6 (workspace slice 2) — migrate workspace NODE MEMBER packages
to the ament shape, mirroring `9c20918fc`'s hand-migration of the 6 C members:

  * the `nano_ros_workspace_pkg_guard()` preamble → `find_package(nano_ros
    REQUIRED)` + one `find_package(<dep> REQUIRED)` per package.xml <depend>
  * explicit `nros_find_interfaces(...)` lines dropped (the verb owns codegen)
  * `nano_ros_node_register(NAME n CLASS c [LANGUAGE l] [TYPED] [SHAPE s]
    [HEADER h] [CALLBACK_GROUPS g...] SOURCES s... [DEPLOY d...])`
    → `nano_ros_add_node(n CLASS c [SHAPE s] [HEADER h] [TYPED]
       [CALLBACK_GROUPS g...] s... [DEPLOY d...])`
    (LANGUAGE dropped — the verb infers from source extensions)

Entry packages (`*_entry`) and workspace roots are the composition layer and
are NOT touched. Usage: pass member CMakeLists.txt paths as argv.
"""

import re
import sys
from pathlib import Path

GUARD_RE = re.compile(
    r"if\(NOT COMMAND nano_ros_workspace_pkg_guard\).*?"
    r"nano_ros_workspace_pkg_guard\(\)\n",
    re.S,
)
FIND_IFACES_RE = re.compile(r"[ \t]*nros_find_interfaces\([^)]*\)\n")
REGISTER_RE = re.compile(r"nano_ros_node_register\(([^)]*)\)", re.S)


def deps_from_package_xml(pkg_dir: Path) -> list[str]:
    xml = pkg_dir / "package.xml"
    if not xml.is_file():
        return []
    return re.findall(r"<depend>([^<]+)</depend>", xml.read_text())


def parse_register(body: str) -> dict:
    toks = body.split()
    out = {
        "name": None,
        "class": None,
        "typed": False,
        "shape": None,
        "header": None,
        "groups": [],
        "sources": [],
        "deploy": [],
    }
    i = 0
    cur = None
    while i < len(toks):
        t = toks[i]
        if t in ("NAME", "CLASS", "LANGUAGE", "SHAPE", "HEADER"):
            cur = t
        elif t == "TYPED":
            out["typed"] = True
            cur = None
        elif t in ("SOURCES", "DEPLOY", "CALLBACK_GROUPS"):
            cur = t
        else:
            if cur == "NAME":
                out["name"] = t
            elif cur == "CLASS":
                out["class"] = t
            elif cur == "SHAPE":
                out["shape"] = t
            elif cur == "HEADER":
                out["header"] = t
            elif cur == "SOURCES":
                out["sources"].append(t)
            elif cur == "DEPLOY":
                out["deploy"].append(t)
            elif cur == "CALLBACK_GROUPS":
                out["groups"].append(t)
            # LANGUAGE value dropped
        i += 1
    return out


def render_verb(r: dict) -> str:
    parts = [r["name"], "CLASS", r["class"]]
    if r["shape"]:
        parts += ["SHAPE", r["shape"]]
    if r["header"]:
        parts += ["HEADER", r["header"]]
    if r["typed"]:
        parts.append("TYPED")
    if r["groups"]:
        parts += ["CALLBACK_GROUPS", *r["groups"]]
    parts += r["sources"]
    if r["deploy"]:
        parts += ["DEPLOY", *r["deploy"]]
    one_line = f"nano_ros_add_node({' '.join(parts)})"
    if len(one_line) <= 100:
        return one_line
    # long form: keyword-aligned continuation
    head = f"nano_ros_add_node({r['name']} CLASS {r['class']}"
    rest = []
    if r["shape"]:
        rest.append(f"SHAPE {r['shape']}")
    if r["header"]:
        rest.append(f"HEADER {r['header']}")
    if r["typed"]:
        rest.append("TYPED")
    if r["groups"]:
        rest.append("CALLBACK_GROUPS " + " ".join(r["groups"]))
    rest.append(" ".join(r["sources"]))
    if r["deploy"]:
        rest.append("DEPLOY " + " ".join(r["deploy"]))
    body = "\n    ".join(rest)
    return f"{head}\n    {body})"


def migrate(path: Path) -> bool:
    s = path.read_text()
    if "nano_ros_node_register" not in s:
        return False
    m = REGISTER_RE.search(s)
    if not m:
        return False
    reg = parse_register(m.group(1))
    if not reg["name"] or not reg["class"]:
        print(f"SKIP (unparsed register): {path}")
        return False

    deps = deps_from_package_xml(path.parent)
    finds = ["find_package(nano_ros REQUIRED)"]
    finds += [f"find_package({d} REQUIRED)" for d in deps]
    if not GUARD_RE.search(s):
        print(f"SKIP (no guard preamble): {path}")
        return False
    s = GUARD_RE.sub("\n".join(finds) + "\n", s, count=1)
    s = FIND_IFACES_RE.sub("", s)
    s = REGISTER_RE.sub(lambda _: render_verb(reg), s, count=1)
    path.write_text(s)
    return True


def main() -> None:
    n = 0
    for arg in sys.argv[1:]:
        p = Path(arg)
        if migrate(p):
            n += 1
            print(f"migrated {p}")
    print(f"{n} members migrated")


if __name__ == "__main__":
    main()
