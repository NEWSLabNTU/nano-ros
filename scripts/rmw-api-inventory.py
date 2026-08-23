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


def normalise_params(raw):
    """`(rmw_node_t * node, const char * name)` -> `rmw_node_t *, const char *`.

    Parameter NAMES are dropped and whitespace collapsed, so the comparison is
    about the types a caller must supply. Keeping the names would report a
    difference every time upstream renamed an argument, which is not a
    difference in the ABI and would train people to skim the output.
    """
    raw = raw.strip()
    if not raw or raw == "void":
        return []
    out = []
    depth = 0
    cur = ""
    for ch in raw:
        if ch == "(":
            depth += 1
        elif ch == ")":
            depth -= 1
        if ch == "," and depth == 0:
            out.append(cur)
            cur = ""
        else:
            cur += ch
    out.append(cur)

    types = []
    for p in out:
        p = " ".join(p.split())
        if not p:
            continue
        # A function-pointer parameter keeps its shape; anything else loses a
        # trailing identifier.
        if "(*" in p:
            types.append(re.sub(r"\(\*\s*[A-Za-z_][A-Za-z0-9_]*\s*\)", "(*)", p))
            continue
        p = re.sub(r"\[\s*\]$", " []", p)
        m = re.match(r"^(.*?[\s*])([A-Za-z_][A-Za-z0-9_]*)((\s*\[\s*\])?)$", p)
        if m:
            p = (m.group(1) + m.group(3)).strip()
        types.append(" ".join(p.replace("*", " * ").split()))
    return types


def functions_in(text, with_signature=False):
    """Every RMW_PUBLIC-marked function, in declaration order.

    With `with_signature`, yields `(name, return_type, [param types])`.
    """
    body = strip_noise(text)
    out = []
    for chunk in body.split(PUBLIC)[1:]:
        # The declaration ends at the first `;` — a struct body or a later
        # function must not leak into this one's match.
        decl = chunk.split(";", 1)[0]
        for attr in ATTRS:
            decl = decl.replace(attr, " ")
        m = DECL.search(decl)
        if not m:
            continue
        if not with_signature:
            out.append(m.group(1))
            continue
        ret = " ".join(decl[: m.start(1)].replace("*", " * ").split())
        # Params run from the matched `(` to its matching `)`.
        rest = decl[m.end() :]
        depth = 1
        params = ""
        for ch in rest:
            if ch == "(":
                depth += 1
            elif ch == ")":
                depth -= 1
                if depth == 0:
                    break
            params += ch
        out.append((m.group(1), ret, normalise_params(params)))
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


def inventory(include_dir, with_signature=False):
    """{name: header} for every RMW_PUBLIC function under `include_dir`.

    With `with_signature`, values are `(header, return_type, [param types])`."""
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
            for item in functions_in(text, with_signature):
                if with_signature:
                    name, ret, params = item
                    found.setdefault(name, (rel, ret, params))
                else:
                    found.setdefault(item, rel)
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

    # Signature extraction: parameter NAMES go, types stay, and a function
    # pointer parameter keeps its shape.
    sig = functions_in(
        "RMW_PUBLIC\nrmw_ret_t\nrmw_x(rmw_node_t * node, const char * n, void (*cb)(void *), int a[]);",
        with_signature=True,
    )
    # A function-pointer parameter keeps its own spelling — it returns before
    # the `*`-spacing pass, deliberately, since `void ( * )(void *)` would be a
    # worse thing to print at every call site than the shape as written.
    want = ("rmw_x", "rmw_ret_t", ["rmw_node_t *", "const char *", "void (*)(void *)", "int []"])
    if sig != [want]:
        bad.append(f"signature extraction: got {sig}, want [{want}]")
    if normalise_params("void") != []:
        bad.append("`void` must normalise to no parameters")

    if bad:
        for b in bad:
            sys.stderr.write("rmw-api-inventory --self-test: " + b + "\n")
        return 2
    print("rmw-api-inventory --self-test: OK (5 case(s))")
    return 0


def main(argv):
    ap = argparse.ArgumentParser()
    ap.add_argument("--include", help="rmw include dir (default: via AMENT_PREFIX_PATH)")
    ap.add_argument("--json", action="store_true")
    ap.add_argument(
        "--signatures", action="store_true",
        help="emit `name<TAB>return<TAB>param, param<TAB>header` — the form the "
             "shape comparison consumes offline",
    )
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

    found = inventory(inc, with_signature=args.signatures)
    if args.signatures:
        for name in sorted(found):
            header, ret, params = found[name]
            print(f"{name}\t{ret}\t{', '.join(params)}\t{header}")
        return 0
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
