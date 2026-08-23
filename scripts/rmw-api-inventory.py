#!/usr/bin/env python3
"""Enumerate the functions an rmw implementation is expected to provide.

The upstream `rmw` package declares its API across ~40 headers, and the
declarations are multi-line by convention:

    RMW_PUBLIC
    RMW_WARN_UNUSED
    rmw_ret_t
    rmw_create_node(rmw_context_t * context, const char * name, ...);

so a line-oriented grep undercounts badly — `^rmw_[a-z_]*\\(` found 68 of them in
`rmw.h` alone while the file has 126 declaration starts, and it silently drops
every function whose return type shares the name's line.

This walks each header, strips comments and preprocessor lines, then takes the
identifier immediately before the parameter list of every declaration that is
marked `RMW_PUBLIC`. That marker is what makes a declaration part of the ABI a
middleware must supply, which is exactly the set worth comparing against.

Usage:
    scripts/rmw-api-inventory.py [--include DIR] [--json]
    scripts/rmw-api-inventory.py --self-test

`--include` defaults to the rmw package found through AMENT_PREFIX_PATH, the
same resolution order RFC-0075 uses for the router: a ROS install is located by
the environment that a sourced setup.bash exports, never by a hardcoded path.
"""

import argparse
import json
import os
import re
import sys

# A declaration the ABI must supply. Everything else in these headers is a type,
# a macro, or a static inline convenience.
PUBLIC = "RMW_PUBLIC"

# `<name>(` where <name> is the last identifier before the parameter list.
DECL = re.compile(r"\b(rmw_[A-Za-z0-9_]+)\s*\(")

# Attributes that sit between RMW_PUBLIC and the return type.
ATTRS = ("RMW_WARN_UNUSED", "RMW_DEPRECATED", "RMW_PUBLIC_TYPE")


def strip_noise(text):
    """Comments and preprocessor lines. Both carry `rmw_*(` shapes that are not
    declarations — a doc block naming `rmw_create_node()` is the classic one,
    and counting it would be issue 0719's trap in a new file."""
    text = re.sub(r"/\*.*?\*/", " ", text, flags=re.S)
    text = re.sub(r"(?m)//.*$", " ", text)
    text = re.sub(r"(?m)^\s*#.*$", " ", text)
    return text


def functions_in(text):
    """Every RMW_PUBLIC-marked function name, in declaration order."""
    body = strip_noise(text)
    out = []
    for chunk in body.split(PUBLIC)[1:]:
        # The declaration ends at the first `;` — a struct body or a later
        # function must not leak into this one's match.
        decl = chunk.split(";", 1)[0]
        for attr in ATTRS:
            decl = decl.replace(attr, " ")
        m = DECL.search(decl)
        if m:
            out.append(m.group(1))
    return out


def resolve_include(explicit=None):
    """Where the rmw headers are. AMENT_PREFIX_PATH, never a hardcoded path."""
    if explicit:
        return explicit
    for prefix in os.environ.get("AMENT_PREFIX_PATH", "").split(os.pathsep):
        if not prefix:
            continue
        for cand in (
            os.path.join(prefix, "include", "rmw", "rmw"),
            os.path.join(prefix, "include", "rmw"),
        ):
            if os.path.isdir(cand) and os.path.exists(os.path.join(cand, "rmw.h")):
                return cand
    return None


def inventory(include_dir):
    """{name: header} for every RMW_PUBLIC function under `include_dir`."""
    found = {}
    # walk-ok: an installed ROS include tree, not a repo path
    for root, _dirs, files in os.walk(include_dir):
        for f in sorted(files):
            if not f.endswith(".h"):
                continue
            path = os.path.join(root, f)
            try:
                text = open(path, encoding="utf-8", errors="replace").read()
            except OSError:
                continue
            rel = os.path.relpath(path, include_dir)
            for name in functions_in(text):
                found.setdefault(name, rel)
    return found


def self_test():
    """The multi-line shape, and the two traps."""
    bad = []
    sample = """
/** Create a node. Calls rmw_fake_from_a_comment() internally. */
RMW_PUBLIC
RMW_WARN_UNUSED
rmw_node_t *
rmw_create_node(rmw_context_t * context, const char * name);

// not public — no marker
rmw_ret_t
rmw_internal_helper(void);

RMW_PUBLIC
rmw_ret_t
rmw_destroy_node(rmw_node_t * node);

typedef struct rmw_thing_s { int x; } rmw_thing_t;
"""
    got = functions_in(sample)
    if got != ["rmw_create_node", "rmw_destroy_node"]:
        bad.append(f"expected the two RMW_PUBLIC decls, got {got}")

    # A declaration whose return type shares the name's line — the shape the
    # line-oriented grep dropped.
    one_line = "RMW_PUBLIC\nrmw_ret_t rmw_init(const rmw_init_options_t * o, rmw_context_t * c);"
    if functions_in(one_line) != ["rmw_init"]:
        bad.append(f"one-line return type not matched: {functions_in(one_line)}")

    if bad:
        for b in bad:
            sys.stderr.write("rmw-api-inventory --self-test: " + b + "\n")
        return 2
    print("rmw-api-inventory --self-test: OK (3 case(s))")
    return 0


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--include", help="rmw include dir (default: via AMENT_PREFIX_PATH)")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    args = ap.parse_args(argv)

    if args.self_test:
        return self_test()

    inc = resolve_include(args.include)
    if not inc:
        sys.stderr.write(
            "rmw-api-inventory: no rmw headers found.\n"
            "  Source a ROS install first (`source /opt/ros/<distro>/setup.bash`),\n"
            "  which exports AMENT_PREFIX_PATH, or pass --include.\n"
        )
        return 2

    found = inventory(inc)
    if args.json:
        print(json.dumps({"include": inc, "functions": found}, indent=2, sort_keys=True))
        return 0
    print(f"# rmw API inventory — {inc}")
    print(f"# {len(found)} RMW_PUBLIC function(s)")
    for name in sorted(found):
        print(f"{name}\t{found[name]}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
