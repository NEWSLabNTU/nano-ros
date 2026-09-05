#!/usr/bin/env python3
"""Issue 1050 defect (3) — `NROS_ENTRY_RMW` must name a backend that registers.

The RMW selector became a BAKED rung: `nano_ros_entry()` and
`nros_px4_add_module()` put `NROS_ENTRY_RMW="<name>"` on the target, the C/C++
headers pass it to `nros_cpp_init_rmw` / `nros_support_init_rmw`, and the core
resolves it through `nros_rmw_cffi::resolve_backend`.

That resolution is a LOOKUP, and a miss is `BackendResolution::Unknown` — a hard
error, not a fallback to the registry default. So the two vocabularies have to
agree exactly:

  * what cmake can bake — `NROS_RMW_KNOWN` in `cmake/NanoRosRmwDispatch.cmake`,
    which is also the set `NANO_ROS_RMW` may take;
  * what the registry answers to — the name each backend passes to
    `nros_rmw_cffi_register_named`.

They agree today by convention and nothing held them there. A rename on either
side used to cost nothing; it now turns every image built for that backend into
one that cannot open a session, at runtime, with a message about a selector the
user never wrote.

Buildless: both sides are read out of the sources.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

DISPATCH = ROOT / "cmake" / "NanoRosRmwDispatch.cmake"
BACKEND_ROOT = ROOT / "packages" / "rmw"

# `nros_rmw_cffi_register_named("uorb", …)` in C/C++, `c"zenoh".as_ptr()` in
# Rust, and — cyclone — a named constant, which is why a bare literal scan is
# not enough on its own. `(?<!fn )` drops the Rust/bindgen DECLARATIONS, whose
# first parameter is spelled `name`.
CALL_RE = re.compile(
    r"(?<!fn )nros_rmw_cffi_register_named\s*\(\s*"
    r"(?:c?\"(?P<lit>[A-Za-z0-9_\-]+)\"|(?P<ident>[A-Za-z_][A-Za-z0-9_]*))"
)
CONST_RE = re.compile(
    r"(?:constexpr\s+)?(?:const\s+)?char\s*\*\s*(?:const\s+)?"
    r"(?P<name>[A-Za-z_][A-Za-z0-9_]*)\s*=\s*\"(?P<val>[^\"]*)\""
)

# Names that are registry entries but never a `NANO_ROS_RMW` value: the
# deprecated unnamed shim and the host-probe backend. Listed so the check can be
# an EQUALITY in both directions without pretending these are absent.
NOT_A_CMAKE_RMW = {"default", "metadata"}


class UnresolvedName(Exception):
    """A registration whose name this script cannot read.

    Deliberately fatal rather than skipped: an unreadable name is exactly the
    case where the two vocabularies drift unobserved, which is the whole reason
    for the gate.
    """


def names_in(text: str) -> set[str]:
    """Every backend name registered by one source file."""
    consts = {m.group("name"): m.group("val") for m in CONST_RE.finditer(text)}
    found: set[str] = set()
    for m in CALL_RE.finditer(text):
        name = m.group("lit")
        if name is None:
            ident = m.group("ident")
            if ident not in consts:
                raise UnresolvedName(ident)
            name = consts[ident]
        found.add(name)
    return found


def compare(known: set[str], registered: dict[str, set[str]]) -> list[str]:
    """The rule. Returns one message per disagreement; empty means OK."""
    errors = []
    for name in sorted(known - set(registered)):
        errors.append(
            f'cmake can bake NROS_ENTRY_RMW="{name}" (it is in NROS_RMW_KNOWN), '
            f"but no backend under packages/rmw/ registers under that name.\n"
            f"      A baked selector that names no registered backend resolves "
            f"to `Unknown`, which FAILS the session open — it does not fall "
            f"back to the registry default. Every image built for that RMW "
            f"would die at `nros::init()`."
        )
    for name in sorted(set(registered) - known - NOT_A_CMAKE_RMW):
        where = ", ".join(sorted(registered[name]))
        errors.append(
            f"'{name}' is registered ({where}) but is not in NROS_RMW_KNOWN, "
            f"so no entry can select it.\n"
            f"      Add it to the descriptor set (and regenerate "
            f"cmake/NanoRosRmwDispatch.cmake), or add it to NOT_A_CMAKE_RMW in "
            f"this script if it is a stub that ships in no image."
        )
    return errors


def self_test() -> None:
    """Negative controls — a gate whose rule never fires proves nothing.

    Both halves are exercised: the NAME READER (each spelling a backend uses,
    and the unreadable case), and the RULE (each direction of disagreement,
    plus the escape that silences it).
    """
    # -- the reader --
    assert names_in('nros_rmw_cffi_register_named("uorb", &kVtable);') == {"uorb"}
    assert names_in('nros_rmw_cffi_register_named(c"zenoh".as_ptr(), &V)') == {"zenoh"}
    assert names_in(
        'constexpr const char *kId = "cyclonedds";\n'
        "return nros_rmw_cffi_register_named(kId, &kVtable);"
    ) == {"cyclonedds"}, "a name behind a same-file constant must resolve"
    # A DECLARATION is not a registration — bindgen emits one whose first
    # parameter is `name`, and reading it as a call is what a naive scan does.
    assert names_in(
        "pub fn nros_rmw_cffi_register_named(name: *const c_char) -> NrosRmwRet;"
    ) == set()
    try:
        names_in("nros_rmw_cffi_register_named(kSomeOtherHeadersConstant, &V);")
    except UnresolvedName as e:
        assert str(e) == "kSomeOtherHeadersConstant"
    else:
        raise AssertionError("an unreadable name must be fatal, not skipped")

    # -- the rule --
    assert compare({"zenoh"}, {"zenoh": {"a.rs"}}) == []
    fired = compare({"zenoh", "uorb"}, {"zenoh": {"a.rs"}})
    assert len(fired) == 1 and "uorb" in fired[0], "a bakeable-but-unregistered name must fire"
    fired = compare({"zenoh"}, {"zenoh": {"a.rs"}, "quux": {"b.cpp"}})
    assert len(fired) == 1 and "quux" in fired[0], "a registered-but-unbakeable name must fire"
    # The escape for the second direction is the exemption list, not a rename.
    assert compare({"zenoh"}, {"zenoh": {"a.rs"}, "default": {"b.rs"}}) == []


def cmake_known() -> set[str]:
    text = DISPATCH.read_text()
    m = re.search(r'set\(NROS_RMW_KNOWN\s+"([^"]*)"', text)
    if not m:
        sys.exit(f"{DISPATCH}: no NROS_RMW_KNOWN — has the dispatch file moved?")
    return {n for n in m.group(1).split(";") if n}


def registered_names() -> dict[str, set[str]]:
    """Backend name -> the files that register it."""
    found: dict[str, set[str]] = {}
    # `git ls-files`, not `rglob` — issue 0844's rule, and `check-no-tracked-file-find`
    # enforces it: an index lookup instead of a walk, measured at 7m36s -> 0.8s
    # for the same 232 paths, and pruning does not help because `find` still
    # stats every directory it considers pruning. `packages/rmw` also carries
    # vendored submodule trees and build output that a walk would read and the
    # index has never seen.
    r = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "--", BACKEND_ROOT.relative_to(ROOT).as_posix()],
        capture_output=True, text=True, check=False,
    )
    if r.returncode != 0:
        sys.exit(f"check-entry-rmw-vocabulary: `git ls-files` failed:\n  {r.stderr.strip()}")
    for rel in sorted(x for x in r.stdout.splitlines() if x.strip()):
        path = ROOT / rel
        if path.suffix not in {".rs", ".c", ".cpp", ".cc", ".h", ".hpp"}:
            continue
        # `packages/rmw/cffi` is the REGISTRY, not a backend: it declares and
        # defines `nros_rmw_cffi_register_named`, and its only call is the
        # deprecated unnamed `"default"` shim.
        if rel.startswith("packages/rmw/cffi/"):
            continue
        # A test registers stub backends under invented names; that is the
        # point of a stub, and none of them is a shipped vocabulary entry.
        if "/tests/" in rel:
            continue
        try:
            names = names_in(path.read_text(errors="replace"))
        except UnresolvedName as e:
            sys.exit(
                f"{rel}: nros_rmw_cffi_register_named() is called with `{e}`, "
                f"which is not a string literal and not a `const char*` defined "
                f"in the same file.\n"
                f"Either spell the name at the call, or define it as a one-line "
                f"constant there — this gate has to be able to read it "
                f"(issue 1050)."
            )
        for name in names:
            found.setdefault(name, set()).add(rel)
    return found


def main() -> int:
    # Always, not only behind a flag: a negative control nobody runs decays
    # into a comment, and this rule's whole job is to fire.
    self_test()
    if "--self-test" in sys.argv:
        print("check-entry-rmw-vocabulary self-test: OK")
        return 0

    known = cmake_known()
    registered = registered_names()
    errors = compare(known, registered)
    if errors:
        print("check-entry-rmw-vocabulary: FAIL", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1

    print(
        f"check-entry-rmw-vocabulary: OK "
        f"({len(known)} bakeable name(s), each registered: {', '.join(sorted(known))})"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
