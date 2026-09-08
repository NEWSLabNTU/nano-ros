#!/usr/bin/env python3
"""The subject list for `check-cpp-capability-layout`, DERIVED from the headers.

Issue 1225. The gate MEASURES its rule -- "a capability probe may gate a METHOD,
it may never change `sizeof`" -- which is right, and it measured it over three
authored names:

    TYPES=("rclcpp::Node" "::nros::Node" "::nros::QoS")

Three of the API's types, chosen while chasing the `timers_` defect the gate was
built for. Every other public type was unmeasured, and a sweep during phase-427
found three that follow a probe: `nros::Timer` 24 -> 32, `nros::GuardCondition`
32 -> 40, `nros::ComponentNode` 55784 -> 55848. That is the authored-list-drifts
class `CLAUDE.md` already records for the RMW parity map, whose authored table
read `("gap", "no vtable slot")` for 28 slots that had moved.

So the list stops being authored. This script asks clang what the umbrella
header actually exports in OUR namespaces and prints one measurable spelling per
line; the gate compiles all of them in one probe TU per configuration.

WHY CLANG AND NOT A TEXT SCAN
-----------------------------
Same reason `scripts/api_parity/extract_cxx.py` uses one, and this reuses its
plumbing (`dump_ast`, `annotate_files`, `nros_cpp_include_args`) rather than
starting a second one: a regex over headers cannot tell a class from a forward
declaration, a nested type from a top-level one, a template parameter with a
default from one without, or an alias template (not instantiable bare) from a
concrete typedef (measurable). All four distinctions decide whether a name can
appear in a `sizeof`, and getting any of them wrong turns into either a spurious
red or -- much worse -- a silently dropped subject, which is the defect this
script exists to remove.

`dump_ast` RAISES on any clang diagnostic, which is load-bearing here. The old
gate compiled against `-Itarget/nros-{c,cpp}-generated`, dirs a pristine tree
does not have, so its probe TU had 149 errors and printed a `sizeof` anyway --
GCC keeps going after `#error`. Numbers measured off a TU that does not compile
are not measurements. Parsing through this script makes that impossible: no
clean parse, no subject list, no gate run.

WHY `-DNROS_PLATFORM_NUTTX` AND NOT A BUILD
-------------------------------------------
`nros_cpp_config_generated.h` is emitted per-build, and the committed
`_nuttx` variant is the one the stub selects under that define -- the same
choice `extract_cxx` already makes, for the same reason: a gate that needs a
fixture to be fresh runs somewhere and rots everywhere else. The gate needs a
sizes header that is CONSISTENT between its arms, not one that is host-shaped,
so a committed one is strictly better. It is also what moves the gate off the
build lane.

WHAT COUNTS AS A SUBJECT
------------------------
Every top-level declaration in `nros`, `rclcpp`, `rclcpp_action` and
`rclcpp_lifecycle` (the roots `scripts/api-parity.py` established -- imported
from it rather than re-spelled) that can legally appear inside `sizeof`:

  class / struct   measured bare
  enum             measured bare; an enum's size follows its underlying type,
                   and an underlying type inside an `#if` is the same defect
  typedef / alias  measured bare -- `nros::Result` IS `ResultOf<void>`, and
                   `rclcpp::Timer` / `rclcpp::TimerBase` are what a ported file
                   writes for `nros::Timer`. The alias spelling is the one whose
                   layout a porting user actually depends on.
  class template   measured at a DERIVED instantiation (below)

WHAT A TEMPLATE IS MEASURED AT
------------------------------
A template has no layout until it is instantiated, so "skip templates" would
leave 27 of 90 subjects unmeasured -- a hole exactly the size of the one this
change closes. The instantiation is derived from the parameter list rather than
authored: every parameter is filled, a type parameter with `int` and a non-type
parameter with `4`. Measured 2026-09-09: all 27 instantiate, message- and
service-parameterised ones included (`Publisher<int>` is 824 bytes), because
these templates constrain their arguments through traits with fallbacks rather
than through hard requirements. See `_instantiation` for why a DEFAULTED
parameter is filled too rather than left to its default.

A parameter kind that cannot be filled this way -- a template template
parameter -- is reported as UNMEASURABLE rather than dropped. There are none
today; if one appears, the gate says so by name instead of quietly shrinking.

Usage::

    cpp-capability-subjects.py               # one subject spelling per line
    cpp-capability-subjects.py --include-args # the compile flags, one per line
    cpp-capability-subjects.py --json        # subjects + diagnostics
    cpp-capability-subjects.py --self-test
"""

import json
import os
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
sys.path.insert(0, os.path.join(ROOT, "scripts", "api_parity"))
sys.path.insert(0, os.path.join(ROOT, "scripts"))

import extract_cxx  # noqa: E402

# The four namespaces that are OURS. Imported from `api-parity.py` rather than
# re-spelled: PR #773 established the set there, and two copies of a namespace
# list is how one of them ends up a namespace behind.
#
# `api-parity.py` has a hyphen in its name, so it is loaded by path rather than
# by `import`. The alternative -- writing `{"nros", "rclcpp", ...}` here -- is
# the same authored-list defect one level up from the one this file removes.
def _our_roots():
    import importlib.util

    spec = importlib.util.spec_from_file_location(
        "_api_parity_roots", os.path.join(ROOT, "scripts", "api-parity.py")
    )
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    roots = set(mod.RCLCPP_NAMESPACES) | {"nros"}
    if "nros" not in roots or "rclcpp" not in roots:
        raise SystemExit(
            "cpp-capability-subjects: api-parity.py's RCLCPP_NAMESPACES no longer "
            "names the roots this gate needs; do not paper over it with a literal."
        )
    return roots


UMBRELLA = '#include "nros/nros.hpp"\n'

# The gate's compile flags, in ONE place, so the parse that DERIVES the subjects
# and the probe that MEASURES them cannot drift apart. `nros_cpp_include_args`
# is `extract_cxx`'s; the platform-api dir is this gate's addition.
def include_args():
    return extract_cxx.nros_cpp_include_args() + [
        "-I" + os.path.join(ROOT, "packages/platform/nros-platform-api/include")
    ]


def _instantiation(name, node):
    """(spelling, problem). Fill EVERY parameter, defaults included.

    Leaving a defaulted parameter to its default is the more faithful thing to
    measure -- it is the shape a user writes -- and it does not work, measured:
    `Future<T, Cap = rx_buffer_capacity<T>::value>` computes its default FROM
    the first argument, and `rx_buffer_capacity<int>::value` is
    `int::SERIALIZED_SIZE_MAX`, which does not exist. A default argument may
    depend on the earlier parameters in ways a synthetic `int` does not satisfy,
    so relying on defaults makes the derivation depend on the argument being a
    REAL message type -- which would put us back to authoring instantiations.

    Supplying every parameter makes each instantiation self-contained. It costs
    nothing the gate cares about: the question is whether a layout follows a
    capability macro, and any legal instantiation answers it.
    """
    args = []
    for c in node.get("inner", []):
        kind = c.get("kind")
        if kind == "TemplateTypeParmDecl":
            args.append("int")
        elif kind == "NonTypeTemplateParmDecl":
            args.append("4")
        elif kind == "TemplateTemplateParmDecl":
            return None, (
                f"{name}: takes a template template parameter, which this "
                f"derivation cannot fill. Give it an instantiation by hand or "
                f"say why it has no layout to measure -- do NOT drop it."
            )
    if not args:
        return None, (
            f"{name}: is a template clang reports no parameters for. Check the "
            f"derivation rather than dropping the subject."
        )
    return f"::{name}<{', '.join(args)}>", None


def _record_of(node):
    """The CXXRecordDecl inside a ClassTemplateDecl, if it has a definition."""
    for c in node.get("inner", []):
        if c.get("kind") == "CXXRecordDecl" and c.get("completeDefinition"):
            return c
    return None


def collect(ast, roots):
    """(subjects, templates, problems) from a clang AST.

    Walks namespaces only. A nested type is deliberately out of scope: its
    layout moves exactly when its enclosing type's does, and the enclosing type
    IS a subject, so measuring both would double the report without widening it.
    """
    subjects = {}
    templates = {}
    problems = []
    stack = [(ast, "")]
    while stack:
        node, ns = stack.pop()
        for child in node.get("inner", []):
            kind = child.get("kind")
            name = child.get("name", "")
            if kind == "NamespaceDecl":
                if name:
                    stack.append((child, ns + name + "::"))
                continue
            if ns.rstrip(":") not in roots or not name:
                continue
            qual = ns + name
            if kind == "CXXRecordDecl":
                if child.get("completeDefinition"):
                    subjects[qual] = "::" + qual
            elif kind == "EnumDecl":
                if child.get("inner"):
                    subjects[qual] = "::" + qual
            elif kind in ("TypedefDecl", "TypeAliasDecl"):
                subjects[qual] = "::" + qual
            elif kind in ("ClassTemplateDecl", "TypeAliasTemplateDecl"):
                if kind == "ClassTemplateDecl" and _record_of(child) is None:
                    continue  # a forward declaration has no layout
                spelling, problem = _instantiation(qual, child)
                if problem:
                    if problem not in problems:
                        problems.append(problem)
                elif qual not in templates:
                    templates[qual] = spelling
    return subjects, templates, problems


# The derivation's OWN negative control, and the reason it is a floor rather
# than a list: these five names were the gate's authored `TYPES` before this
# change, so a derivation that returns fewer than these has silently narrowed --
# the exact failure the change is meant to end. It can only make the gate
# stricter, never weaker, and it costs nothing to keep true.
FLOOR = (
    "::rclcpp::Node",
    "::nros::Node",
    "::nros::QoS",
    "::nros::Result",
    "::nros::ResultOf<int>",
)


# The derivation asks for the PORTING SURFACE, always (phase-438 W4).
#
# Since W2 the `NROS_CPP_HAS_*` macros are a consumer REQUEST, not something a
# hosted compiler is handed, so a parse without this flag sees a SMALLER API
# than the gate's baseline arm measures -- `::rclcpp::NodeOptions` is the one
# such subject today (90 with, 89 without, measured). Deriving without it would
# narrow the subject list exactly the way issue 1225 exists to prevent, and
# would silently orphan that subject's baseline line.
#
# It is appended rather than left to the caller so the negative control's
# mutated-tree derivation cannot drift from the real one.
DERIVE_FLAGS = ["-DNROS_CPP_STD=1"]


def derive(args=None):
    """(subjects, template instantiations, problems).

    `args` exists so the gate's own negative control can derive against a
    MUTATED copy of the include tree with the same code path the real run uses.
    A control that re-implements the derivation tests the re-implementation.
    """
    roots = _our_roots()
    try:
        with tempfile.TemporaryDirectory() as td:
            ast = extract_cxx.dump_ast(
                UMBRELLA,
                "c++",
                (list(args) if args else include_args()) + DERIVE_FLAGS,
                td,
            )
    except RuntimeError as exc:
        return [], [], [str(exc)]
    subjects, templates, problems = collect(ast, roots)
    out = sorted(set(subjects.values()) | set(templates.values()))
    missing = [f for f in FLOOR if f not in out]
    if missing:
        problems.append(
            "the derivation lost subject(s) the AUTHORED list already had: "
            + ", ".join(missing)
            + "\n      A derived list that is smaller than the list it replaced is"
            "\n      the narrowing this gate exists to prevent."
        )
    return out, sorted(templates.values()), problems


def self_test():
    """Prove the collector can fail, on the NORMAL path.

    Three shapes, all of which a text scanner gets wrong and all of which
    silently drop a subject if the AST walk gets them wrong: a forward
    declaration (no layout), a template parameter with a default (a different
    instantiation), and an alias template (not instantiable bare, so it must be
    filled like any other template).
    """
    ast = {
        "inner": [
            {
                "kind": "NamespaceDecl",
                "name": "nros",
                "inner": [
                    {"kind": "CXXRecordDecl", "name": "Real", "completeDefinition": True},
                    {"kind": "CXXRecordDecl", "name": "FwdOnly"},
                    {
                        "kind": "ClassTemplateDecl",
                        "name": "Defaulted",
                        "inner": [
                            {"kind": "TemplateTypeParmDecl", "name": "T"},
                            {
                                "kind": "NonTypeTemplateParmDecl",
                                "name": "Cap",
                                "inner": [{"kind": "TemplateArgument"}],
                            },
                            {"kind": "CXXRecordDecl", "completeDefinition": True},
                        ],
                    },
                    {
                        "kind": "ClassTemplateDecl",
                        "name": "Higher",
                        "inner": [
                            {"kind": "TemplateTemplateParmDecl", "name": "C"},
                            {"kind": "CXXRecordDecl", "completeDefinition": True},
                        ],
                    },
                ],
            },
            {
                "kind": "NamespaceDecl",
                "name": "std",
                "inner": [
                    {"kind": "CXXRecordDecl", "name": "NotOurs", "completeDefinition": True}
                ],
            },
        ]
    }
    subjects, templates, problems = collect(ast, {"nros", "rclcpp"})
    fail = []
    if "::nros::Real" not in subjects.values():
        fail.append("a defined class was not collected")
    if "nros::FwdOnly" in subjects:
        fail.append("a FORWARD DECLARATION was collected; it has no layout")
    if "std::NotOurs" in subjects:
        fail.append("a type outside our roots was collected")
    if templates.get("nros::Defaulted") != "::nros::Defaulted<int, 4>":
        fail.append(
            "a defaulted template parameter was left to its default: got "
            f"{templates.get('nros::Defaulted')!r}, want '::nros::Defaulted<int, 4>'. "
            "A default that is computed from an earlier parameter does not "
            "survive a synthetic `int` (nros::Future is the measured case)."
        )
    if not any("template template parameter" in p for p in problems):
        fail.append("a template template parameter was dropped instead of reported")
    if fail:
        print(
            "cpp-capability-subjects --self-test FAILED:\n  - " + "\n  - ".join(fail),
            file=sys.stderr,
        )
        raise SystemExit(1)


def main():
    self_test()
    if "--include-args" in sys.argv:
        for a in include_args():
            print(a)
        return 0
    subjects, templates, problems = derive()
    if problems:
        print("cpp-capability-subjects: cannot derive the subject list:", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        return 1
    if "--json" in sys.argv:
        print(json.dumps({"subjects": subjects, "templates": templates}, indent=2))
        return 0
    for s in subjects:
        print(s)
    return 0


if __name__ == "__main__":
    sys.exit(main())
