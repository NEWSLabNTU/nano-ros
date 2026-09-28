#!/usr/bin/env python3
"""Issue 1050 defect (3) — `NROS_ENTRY_RMW` must name a backend that registers.

The RMW selector became a BAKED rung: `nano_ros_entry()` and
`nros_px4_add_module()` put `NROS_ENTRY_RMW="<name>"` on the target, the C/C++
headers pass it to `nros_cpp_init_rmw` / `nros_support_init_rmw`, and the core
resolves it through `nros_rmw_cffi::resolve_backend`.

That resolution is a LOOKUP, and a miss is `BackendResolution::Unknown` — a hard
error, not a fallback to the registry default. So the two vocabularies have to
agree exactly:

  * what cmake can bake — the canonical `<nano_ros_provides kind="rmw"/>`
    announcement of each in-tree backend, which is also the set `NANO_ROS_RMW`
    may take (phase-439 W4: `NROS_RMW_KNOWN` is no longer a generated literal
    in `cmake/NanoRosRmwDispatch.cmake`, it is a query);
  * what the registry answers to — the name each backend passes to
    `nros_rmw_cffi_register_named`.

They agree today by convention and nothing held them there. A rename on either
side used to cost nothing; it now turns every image built for that backend into
one that cannot open a session, at runtime, with a message about a selector the
user never wrote.

And a third rule, issue 1530: a name in `NOT_A_CMAKE_RMW` — a registry entry no
entry can ever bake — must not SELF-REGISTER through `nros_rmw_register_backend!`.
The exemption list says "no image selects this by name"; an `.init_array` ctor
puts it in the registry of every image that links the crate anyway, and since
issue 1050 a second registered name makes a selector-less open `Ambiguous`, i.e.
a hard refusal. That is not hypothetical: the census recorder's ctor did exactly
this to every native C and C++ image, and because the C surface reads no baked
rung (issue 1531) every native C example failed `nros_support_init` with
`NROS_RET_INVALID_ARGUMENT` for eighteen days behind an absorbing stale verdict.

The rule lives HERE rather than in a gate of its own because this script already
owns both vocabularies and the exemption list that distinguishes them. A second
script would be a second answer to "which names can an entry name", which is the
drift this one exists to prevent.

Buildless: all three sides are read out of the sources.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

BACKEND_ROOT = ROOT / "packages" / "rmw"

# The announcement each backend's `package.xml` carries. Same regex shape as
# `check-rmw-descriptors.py`'s, and the FIRST match per package is the canonical
# name (the rule `rmw_resolver::known_rmw_in` applies).
PROVIDES_RE = re.compile(r'<nano_ros_provides\s+kind="rmw"\s+name="([^"]+)"\s*/?>')

# `nros_rmw_cffi_register_named("uorb", …)` in C/C++, `c"zenoh".as_ptr()` in
# Rust, and — cyclone — a named constant, which is why a bare literal scan is
# not enough on its own. `(?<!fn )` drops the Rust/bindgen DECLARATIONS, whose
# first parameter is spelled `name`.
# Two spellings reach the registry, and reading only the first is how this gate
# came to report "0 unbakeable names present" over a tree where `metadata` was
# registered and self-registering (issue 1530, and issue 0196's shape):
#
#   * the C ABI entry point, `nros_rmw_cffi_register_named(...)` — every C/C++
#     backend, and the Rust ones that call it directly;
#   * the Rust adapter's associated function,
#     `RustBackendAdapter::<T>::register_named(c"metadata".as_ptr())`, which is a
#     wrapper over that same entry point.
#
# `(?<!fn )` drops the Rust/bindgen DECLARATIONS, whose first parameter is
# spelled `name`. The turbofish is optional so a plain `Adapter::register_named`
# is read too.
CALL_RE = re.compile(
    # `(?<![A-Za-z0-9_])` is load-bearing beside `(?<!fn )`: without it the bare
    # `register_named` alternative matches the TAIL of
    # `nros_rmw_cffi_register_named` in a declaration and reads its `name`
    # parameter as the backend's name.
    r"(?<!fn )(?<![A-Za-z0-9_])(?:nros_rmw_cffi_register_named|register_named)\s*\(\s*"
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


# issue 1530 — the `.init_array` self-registration macro, at STATEMENT position.
# A doc comment naming the macro is not an invocation (`section.rs` documents it
# in a `///` block, and `nros-board-{nuttx,threadx}` discuss it in `//` prose), so
# a line whose first non-space characters open a comment is not a site. Same
# `strip`-before-match discipline as the other source-reading gates.
SELF_REGISTER_RE = re.compile(r"^\s*(?:[A-Za-z_][A-Za-z0-9_]*::)*nros_rmw_register_backend!")


def self_registers(text: str) -> bool:
    """Does this source file INVOKE the self-registration macro?"""
    for line in text.splitlines():
        stripped = line.lstrip()
        if stripped.startswith(("//", "/*", "*", "#")):
            continue
        if SELF_REGISTER_RE.match(line):
            return True
    return False


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


def compare(
    known: set[str],
    registered: dict[str, set[str]],
    self_registered: dict[str, set[str]] | None = None,
) -> list[str]:
    """The rule. Returns one message per disagreement; empty means OK."""
    errors = []
    for name in sorted(known - set(registered)):
        errors.append(
            f'cmake can bake NROS_ENTRY_RMW="{name}" (a package.xml announces it), '
            f"but no backend under packages/rmw/ registers under that name.\n"
            f"      A baked selector that names no registered backend resolves "
            f"to `Unknown`, which FAILS the session open — it does not fall "
            f"back to the registry default. Every image built for that RMW "
            f"would die at `nros::init()`."
        )
    for name in sorted(set(registered) - known - NOT_A_CMAKE_RMW):
        where = ", ".join(sorted(registered[name]))
        errors.append(
            f"'{name}' is registered ({where}) but no package.xml announces it "
            f"as an rmw provision, so no entry can select it.\n"
            f"      Add `<nano_ros_provides kind=\"rmw\" name=\"{name}\"/>` to the "
            f"backend's package.xml (with an nros-rmw.toml beside it), or add it "
            f"to NOT_A_CMAKE_RMW in this script if it is a stub that ships in no "
            f"image."
        )
    # issue 1530 — the third rule. An exempt name is one no entry can select;
    # self-registering it contradicts the exemption and makes every image that
    # links the crate ambiguous.
    for name in sorted(set(self_registered or {}) & NOT_A_CMAKE_RMW):
        where = ", ".join(sorted((self_registered or {})[name]))
        errors.append(
            f"'{name}' is in NOT_A_CMAKE_RMW — no entry can bake it — yet it "
            f"SELF-REGISTERS via nros_rmw_register_backend! ({where}).\n"
            f"      Since issue 1050 a second registered name makes a "
            f"selector-less open `Ambiguous`, which is a hard refusal, so every "
            f"image linking this crate can no longer open a session without "
            f"naming a backend. Register it from the consumer that SELECTS it "
            f"instead (issue 1530: `census_select_backend` registers the "
            f"recorder and then sets $NROS_RMW), or make the name bakeable."
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
    # issue 1530 — the Rust adapter's wrapper is the OTHER spelling that reaches
    # the registry. Missing it is what made this gate blind to `metadata`.
    assert names_in(
        'nros_rmw_cffi::RustBackendAdapter::<MetadataRmw>::register_named(c"metadata".as_ptr())'
    ) == {"metadata"}, "the adapter spelling must be read too"
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

    # -- issue 1530: the self-registration reader --
    assert self_registers("nros_rmw_cffi::nros_rmw_register_backend! {\n    fn() {}\n}")
    assert self_registers("nros_rmw_register_backend! { fn() {} }")
    assert not self_registers(
        "/// nros_rmw_cffi::nros_rmw_register_backend! {\n/// }\n"
    ), "a doc comment naming the macro is not an invocation"
    assert not self_registers(
        "// The unified-RMW `nros_rmw_register_backend!` macro is a no-op on NuttX"
    ), "prose about the macro is not an invocation"
    assert not self_registers("let _ = nros_rmw_metadata_register();")

    # -- issue 1530: the rule --
    assert compare({"zenoh"}, {"zenoh": {"a.rs"}}, {"zenoh": {"a.rs"}}) == [], (
        "a BAKEABLE name may self-register — that is how every real backend works"
    )
    fired = compare(
        {"zenoh"}, {"zenoh": {"a.rs"}, "metadata": {"m.rs"}}, {"metadata": {"m.rs"}}
    )
    assert len(fired) == 1 and "metadata" in fired[0] and "m.rs" in fired[0], (
        "an exempt name that self-registers must fire, and name its file"
    )
    # Registered-but-not-self-registered is the fixed state, and must be silent.
    assert compare({"zenoh"}, {"zenoh": {"a.rs"}, "metadata": {"m.rs"}}, {}) == []


def cmake_known() -> set[str]:
    """Every rmw name `NANO_ROS_RMW` may take, from the ANNOUNCEMENTS.

    phase-439 W4 — this used to regex `set(NROS_RMW_KNOWN "…")` out of
    `cmake/NanoRosRmwDispatch.cmake`, which was a GENERATED literal listing the
    backends present when the `nros` binary was compiled. That file is now a
    query against the provider scan and holds no list, so there is nothing left
    to grep.

    The same rule the CLI applies (`rmw_resolver::known_rmw_in`): one CANONICAL
    name per provider — its FIRST `<nano_ros_provides kind="rmw"/>` — not one
    per announcement, because `zenoh` / `rmw-zenoh` / `rmw-zenoh-cffi` are three
    spellings of one backend and only the first is a `NANO_ROS_RMW` value.

    Read from source rather than by asking the CLI on purpose: this gate is on
    the buildless, source-free `check-fast` line, where no `nros` binary exists.
    That makes it a SECOND reader of the announcements, which this repo
    normally refuses — the mitigation is that it reads the same files by the
    same rule and that `check-rmw-descriptors` already parses them this way, and
    the cross-check is
    `rmw_resolver::tests::every_announced_backend_resolves_through_the_scan`,
    which fails if the CLI's answer and the announcements ever diverge.

    Out-of-tree providers are deliberately out of scope: this gate is about
    whether THIS tree's bakeable names and THIS tree's registrations agree.
    """
    known: set[str] = set()
    for pkg_xml in sorted(BACKEND_ROOT.glob("*/*/package.xml")):
        if not (pkg_xml.parent / "nros-rmw.toml").is_file():
            continue
        m = PROVIDES_RE.search(pkg_xml.read_text(errors="replace"))
        if not m:
            sys.exit(
                f"{pkg_xml}: an rmw backend with a descriptor announces no "
                f'<nano_ros_provides kind="rmw" name="…"/> — nothing could '
                f"select it."
            )
        known.add(m.group(1))
    if not known:
        sys.exit(
            f"{BACKEND_ROOT}: no rmw provider announcements found — refusing to "
            f"compare an EMPTY bakeable set, which would pass vacuously."
        )
    return known


def registered_names() -> tuple[dict[str, set[str]], dict[str, set[str]]]:
    """(backend name -> files that register it, backend name -> files that SELF-register it)."""
    found: dict[str, set[str]] = {}
    self_reg: dict[str, set[str]] = {}
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
        text = path.read_text(errors="replace")
        for name in names:
            found.setdefault(name, set()).add(rel)
        # issue 1530 — a file that both registers a name and invokes the macro
        # self-registers that name. The two readers run over the same text so a
        # crate cannot hide one from the other.
        if self_registers(text):
            for name in names:
                self_reg.setdefault(name, set()).add(rel)
    return found, self_reg


def main() -> int:
    # Always, not only behind a flag: a negative control nobody runs decays
    # into a comment, and this rule's whole job is to fire.
    self_test()
    if "--self-test" in sys.argv:
        print("check-entry-rmw-vocabulary self-test: OK")
        return 0

    known = cmake_known()
    registered, self_registered = registered_names()
    errors = compare(known, registered, self_registered)
    if errors:
        print("check-entry-rmw-vocabulary: FAIL", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1

    exempt_registered = sorted(set(registered) & NOT_A_CMAKE_RMW)
    print(
        f"check-entry-rmw-vocabulary: OK "
        f"({len(known)} bakeable name(s), each registered: {', '.join(sorted(known))}; "
        f"{len(exempt_registered)} unbakeable name(s) present and none "
        f"self-registering: {', '.join(exempt_registered) or 'none'})"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
