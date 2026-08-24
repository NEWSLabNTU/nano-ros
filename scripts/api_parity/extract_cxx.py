#!/usr/bin/env python3
"""Extract a C or C++ public API surface from a clang JSON AST.

Phase 379. This is the half of `scripts/api-parity.py` that turns headers into
records; the correlation lives next door in `correlate.py`.

Why a clang AST and not a regex sweep: the question the campaign asks is
"do the ARGUMENTS agree", and arguments are exactly what a regex over headers
gets wrong -- default values, template parameters, `const &` vs value, and
macro-expanded visibility attributes (`RCLCPP_PUBLIC`) all defeat it. clang
already resolved all of that.

Both sides parse with ZERO errors, which is a precondition rather than a nicety:
a partial AST silently drops declarations, and a dropped declaration reads as a
gap in our surface that is not really there.

  ours   -- `-DNROS_PLATFORM_NUTTX` selects the COMMITTED size header
            (`nros_cpp_config_generated_nuttx.h`), so the parse needs no build.
            Every other platform's sizes come from `build.rs` and would make
            this tool depend on a fixture being fresh.
  theirs -- `/opt/ros/<distro>/include`, minus four directories that shadow libc
            headers (`idl/string.h` shadows `<string.h>`; `include/`, `dds/`,
            `ddsc/` likewise). Including all 202 package dirs produces 20 errors
            in libstdc++ that look like a toolchain problem and are not.

Scope is decided by NAMESPACE, not by source file. clang emits `loc.file` only
when it CHANGES from the previously printed location, so recovering a decl's
file means carrying state across a strict pre-order walk of every node in a
400 MB AST -- and getting that subtly wrong silently attributes `std::shared_mutex`
and `builtin_interfaces::msg::Time_` to rclcpp while dropping `rclcpp::Node`,
which is what the first version of this did. The namespace is already on the
path down; it needs no state and cannot drift.
"""

import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

# Include dirs under /opt/ros/*/include whose contents shadow libc/libstdc++
# headers when placed on the include path. See the module docstring.
SHADOWING_DIRS = {"idl", "include", "dds", "ddsc"}


def ros_include_args(prefix):
    """-I flags for every ROS package include dir except the shadowing ones."""
    inc = os.path.join(prefix, "include")
    args = []
    for entry in sorted(os.listdir(inc)):
        if entry in SHADOWING_DIRS:
            continue
        path = os.path.join(inc, entry)
        if os.path.isdir(path):
            args.append("-I" + path)
    return args


def nros_cpp_include_args():
    return [
        "-DNROS_PLATFORM_NUTTX",
        "-I" + os.path.join(ROOT, "packages/api/nros-cpp/include"),
        "-I" + os.path.join(ROOT, "packages/api/nros-c/include"),
    ]


def nros_c_include_args():
    return [
        "-DNROS_PLATFORM_NUTTX",
        "-I" + os.path.join(ROOT, "packages/api/nros-c/include"),
    ]


def dump_ast(source_text, lang, extra_args, tmpdir):
    """Run clang and return the parsed JSON AST.

    Raises on ANY diagnostic: a surface extracted from a partial AST is worse
    than no surface, because the missing declarations read as real gaps.
    """
    ext = ".cpp" if lang == "c++" else ".c"
    src = os.path.join(tmpdir, "api_parity_probe" + ext)
    with open(src, "w") as fh:
        fh.write(source_text)

    cc = "clang++" if lang == "c++" else "clang"
    std = "-std=c++17" if lang == "c++" else "-std=c11"
    cmd = [cc, std, "-fsyntax-only", "-ferror-limit=0", "-Xclang", "-ast-dump=json"]
    cmd += extra_args + [src]

    proc = subprocess.run(cmd, capture_output=True, text=True)
    errors = [ln for ln in proc.stderr.splitlines() if "error:" in ln]
    if errors:
        raise RuntimeError(
            "clang reported %d error(s) parsing the surface; refusing to extract "
            "from a partial AST:\n  %s" % (len(errors), "\n  ".join(errors[:10]))
        )
    return json.loads(proc.stdout)


def annotate_files(node, state=None):
    """Stamp every node with the source file it came from.

    clang prints `loc.file` only when it CHANGES from the previously printed
    location, so the file of any given node is "the last one printed at or
    before it in the dump". Recovering it therefore needs a STRICT PRE-ORDER
    walk that visits every node -- including ones the surface walk skips, and
    including the insides of class bodies. Updating the state only where the
    surface walk happens to look is how an earlier version of this attributed
    `std::shared_mutex` to rclcpp: the state was whatever the last inspected
    node had set, not the last printed one.

    Runs once over the AST before extraction; the AST for all of rclcpp is
    ~400 MB, which is a second or two and a few GB.
    """
    # Iterative: an rclcpp AST nests far deeper than Python's recursion limit,
    # and raising the limit to survive one input is not a fix.
    current = (state or {}).get("file")
    stack = [node]
    while stack:
        n = stack.pop()
        if n is None:
            continue
        loc = n.get("loc")
        if isinstance(loc, dict) and loc.get("file"):
            current = loc["file"]
        rng = n.get("range")
        if isinstance(rng, dict):
            begin = rng.get("begin")
            if isinstance(begin, dict) and begin.get("file"):
                current = begin["file"]
        if current:
            n["_file"] = current
        inner = n.get("inner")
        if inner:
            # Reversed, so popping yields the children in source order -- the
            # order clang printed them, which is what the omission is relative
            # to.
            stack.extend(reversed(inner))
    return node


def _type(node):
    t = node.get("type") or {}
    return t.get("qualType", "")


def _param(node):
    """A parameter as (name, type, has_default).

    clang marks a defaulted parameter with `"init": "c"` and attaches the
    default expression as the ParmVarDecl's only child -- and that child is
    whatever literal was written, so `IntegerLiteral` for `= 10`. Testing for a
    child whose kind ends in "Expr" therefore misses every integer default,
    which made `spin(int32_t poll_ms = 10)` report as diverging from
    `rclcpp::Executor::spin()`.
    """
    return {
        "name": node.get("name", ""),
        "type": _type(node),
        "default": "init" in node or bool(node.get("inner")),
    }


def _signature(node):
    """Params + return type of a Function/CXXMethod/FunctionTemplate node."""
    params = [_param(c) for c in node.get("inner", []) if c.get("kind") == "ParmVarDecl"]
    qual = _type(node)
    ret = qual.split("(", 1)[0].strip() if "(" in qual else qual
    return params, ret


def _template_params(node):
    out = []
    for c in node.get("inner", []):
        if c.get("kind") in ("TemplateTypeParmDecl", "NonTypeTemplateParmDecl"):
            out.append(c.get("name", "_"))
    return out


# `rclcpp::detail`, `rclcpp::experimental`, `rclcpp::node_interfaces` and friends
# are rclcpp's own internals -- a user writes none of them, so a nano-ros gap
# against one is not a gap. Only the top-level namespace of each package counts.
def in_scope(ns, roots):
    """True when `ns` is exactly one of the surface namespaces."""
    return ns.rstrip(":") in roots


def _named(name, prefixes):
    """C has no namespaces, so the NAME PREFIX is the namespace.

    A C header's file scope is not its public surface -- it also drags in every
    libc declaration it includes. `rclc_*` / `nros_*` is the convention both
    sides already follow, so it is the same filter one spelling down.
    """
    if prefixes is None:
        return True
    return any(name.startswith(p) for p in prefixes)


def walk(node, roots, ns="", out=None, prefixes=None):
    """Collect public declarations in the `roots` namespaces into records."""
    if out is None:
        out = []
    for child in node.get("inner", []):
        kind = child.get("kind")
        name = child.get("name", "")

        if kind == "NamespaceDecl":
            # An anonymous namespace is an implementation detail by definition.
            if not name:
                continue
            walk(child, roots, ns + name + "::", out, prefixes)
            continue

        if not in_scope(ns, roots) or not _named(name, prefixes):
            continue

        if kind in ("CXXRecordDecl", "ClassTemplateDecl"):
            record = child
            tparams = []
            if kind == "ClassTemplateDecl":
                tparams = _template_params(child)
                record = next(
                    (c for c in child.get("inner", []) if c.get("kind") == "CXXRecordDecl"),
                    None,
                )
                if record is None:
                    continue
            if not name or not record.get("completeDefinition"):
                continue
            out.append(
                {
                    "kind": "type",
                    "qual": ns + name,
                    "name": name,
                    "template": tparams,
                    "header": child.get("_file", ""),
                    "members": _members(record),
                }
            )
            continue

        if kind in ("FunctionDecl", "FunctionTemplateDecl") and name:
            fn = child
            tparams = []
            if kind == "FunctionTemplateDecl":
                tparams = _template_params(child)
                fn = next(
                    (c for c in child.get("inner", []) if c.get("kind") == "FunctionDecl"),
                    None,
                )
                if fn is None:
                    continue
            params, ret = _signature(fn)
            out.append(
                {
                    "kind": "function",
                    "qual": ns + name,
                    "name": name,
                    "template": tparams,
                    "header": child.get("_file", ""),
                    "params": params,
                    "ret": ret,
                }
            )
            continue

        if kind == "EnumDecl" and name:
            out.append(
                {
                    "kind": "enum",
                    "qual": ns + name,
                    "name": name,
                    "header": child.get("_file", ""),
                    "values": [
                        c.get("name", "")
                        for c in child.get("inner", [])
                        if c.get("kind") == "EnumConstantDecl"
                    ],
                }
            )
            continue

        if kind in ("TypedefDecl", "TypeAliasDecl", "TypeAliasTemplateDecl") and name:
            out.append(
                {
                    "kind": "alias",
                    "qual": ns + name,
                    "name": name,
                    "header": child.get("_file", ""),
                    "type": _type(child),
                }
            )
            continue

    return out


def _members(record):
    """Public methods of a class/struct, with their parameters.

    C++ defaults to private for `class` and public for `struct`; clang marks the
    switch with an AccessSpecDecl, so the current access is tracked as it walks.
    """
    access = "public" if record.get("tagUsed") == "struct" else "private"
    members = []
    for c in record.get("inner", []):
        kind = c.get("kind")
        if kind == "AccessSpecDecl":
            access = c.get("access", access)
            continue
        if access != "public":
            continue
        if kind in ("CXXMethodDecl", "CXXConstructorDecl", "FunctionTemplateDecl"):
            fn = c
            tparams = []
            if kind == "FunctionTemplateDecl":
                tparams = _template_params(c)
                fn = next(
                    (
                        g
                        for g in c.get("inner", [])
                        if g.get("kind") in ("CXXMethodDecl", "CXXConstructorDecl")
                    ),
                    None,
                )
                if fn is None:
                    continue
            name = fn.get("name", "")
            if not name or name.startswith("operator") or fn.get("isImplicit"):
                continue
            params, ret = _signature(fn)
            members.append(
                {"name": name, "template": tparams, "params": params, "ret": ret}
            )
        elif kind == "FieldDecl" and c.get("name"):
            members.append({"name": c["name"], "field": True, "type": _type(c)})
    return members


def extract(source_text, lang, extra_args, roots, tmpdir, prefixes=None):
    """`roots` is a set of top-level namespace names, e.g. {"rclcpp"}.

    For C pass `roots={""}` (file scope) plus `prefixes={"rclc_", ...}` -- see
    `_named` for why the prefix is not optional there.
    """
    ast = annotate_files(dump_ast(source_text, lang, extra_args, tmpdir))
    return walk(ast, set(roots), prefixes=prefixes)
