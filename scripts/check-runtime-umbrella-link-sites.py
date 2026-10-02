#!/usr/bin/env python3
"""Issue 1467 — a PROPAGATED runtime-umbrella link must go through the resolver.

Two rules about `NanoRos::NanoRosCpp` / `NanoRos::NanoRos`. The first was spelled
by hand at all 21 link sites under `cmake/`; the second was spelled nowhere, and
the nine PROPAGATED sites of the first are where its absence is a bug:

  1. WHICH umbrella (issue 0425 — prefer the C++ one, it bundles nros-c, so a
     mixed workspace keeps exactly ONE Rust staticlib per binary); and
  2. WHETHER the consuming binary wants one at all (issue 1467 — a leaf whose
     app IS a Rust staticlib already defines that whole C ABI, so an umbrella
     reaching its link line is a duplicate-symbol failure: 228 `rust-lld`
     errors across the twelve `threadx_riscv64` leaves).

Rule 2 is invisible from the declaring site: `if(TARGET NanoRos::NanoRosCpp)`
asks what the build tree DEFINES, and every leaf `add_subdirectory()`s the root,
so it is always true. It can only be answered on the CONSUMER, which means a
generator expression on the head target — `cmake/NanoRosRuntimeUmbrella.cmake`'s
`nros_link_runtime_umbrella()`.

WHAT IS ENFORCED

  * A `target_link_libraries(... PUBLIC|INTERFACE ... <umbrella>)` anywhere under
    `cmake/` must instead call `nros_link_runtime_umbrella()`. PUBLIC/INTERFACE
    is precisely the case where the requirement propagates to a binary the
    declaring site cannot see, which is the defect.
  * So must a write of `INTERFACE_LINK_LIBRARIES` naming an umbrella
    (`set_property` / `set_target_properties`) — the `_ffi_lib` ordering hook
    that phase 150.B added is exactly this shape.
  * A PRIVATE link is RULED, not banned, and the ruling is per file with a
    reason. Two independent reasons, both load-bearing:
      - the consumer IS the target, so the umbrella choice is already the
        consumer's own; and
      - `cmake/board/nano-ros-board-{qemu-armv7a,rv-virt}-nuttx.cmake` compare
        that executable's `LINK_LIBRARIES` entries as literal STRINGS
        (`if(_lib STREQUAL "NanoRos::NanoRosCpp")`) to skip the umbrella while
        ferrying its include dirs into the cargo cross-build. A generator
        expression is not a name those comparisons can match, and the failure
        would be a missing per-build mirror include dir surfacing as the Phase
        155.B.5 `nros_config_generated.h` `#error` minutes later.
    The count is pinned so a NEW private site has to come here and read that,
    rather than being copied from a neighbour.

WHAT IS OUT OF SCOPE, and why

  * `examples/**`, `packages/testing/**/fixtures/**`, `integrations/**` — a
    consumer writing `target_link_libraries(<my_app> PRIVATE NanoRos::NanoRos)`
    is using the published API, and that line is what the book tells a user to
    write. `integrations/nuttx/CMakeLists.txt` surfaces the umbrella on NuttX's
    `apps` INTERFACE target; NuttX has no Rust-staticlib app seam
    (`CRATE_TYPES staticlib` appears once in the tree, in the ThreadX RV64
    board), so rule 2 cannot fire there, while the literal name is what the
    NuttX board walkers match.
  * `cmake/NanoRosRuntimeUmbrella.cmake` itself — it IS the resolver.
"""

import re
import subprocess
import sys
from pathlib import Path
import sys as _w3_sys  # noqa: E402
from pathlib import Path as _W3Path  # noqa: E402
_w3_sys.path.insert(0, str(_W3Path(__file__).resolve().parent / "lib"))
import comments  # noqa: E402  phase-472 W3 — the one comment stripper

ROOT = Path(__file__).resolve().parent.parent

RESOLVER = "cmake/NanoRosRuntimeUmbrella.cmake"

# The umbrella target names. `nros_cpp::nros_cpp` / `nros_c::nros_c` are the
# install-time spellings the codegen ladder used to fall back to.
UMBRELLAS = (
    "NanoRos::NanoRosCpp",
    "NanoRos::NanoRos",
    "nros_cpp::nros_cpp",
    "nros_c::nros_c",
)

# RULED private sites: file -> (count, reason). See the module docstring.
RULED_PRIVATE = {
    "cmake/NanoRosEntry.cmake": (
        2,
        "nano_ros_entry()'s own executable — C or C++, never a Rust staticlib "
        "carrier, and the NuttX boards match this name as a string",
    ),
    "cmake/NanoRosNodeRegister.cmake": (
        8,
        "the four RTOS/native typed-entry CARRIER executables (nuttx, threadx, "
        "freertos, native), two lines each",
    ),
    "cmake/compat/NrosRclcppCompat.cmake": (
        2,
        "ament_auto_add_executable + the rclcpp_components_register_node "
        "synthesised main; both are the binary itself",
    ),
}

# `target_link_libraries(<args>)`. cmake arguments here never contain a literal
# paren: `${VAR}`, `$<GENEX:...>` and target names are all paren-free, so a
# non-greedy no-paren body is an exact match for the call.
TLL = re.compile(r"target_link_libraries\s*\(([^()]*)\)", re.DOTALL)

# A write of the propagated link list by property name.
PROP_WRITE = re.compile(
    r"(?:set_property|set_target_properties)\s*\(([^()]*)\)", re.DOTALL
)


def _strip_comments(text: str) -> str:
    """Blank out `#` comments, keeping line structure for accurate numbers."""
    # phase-472 W3 — the shared stripper (scripts/lib/comments.py).
    return comments.strip_comments(text, "cmake")


def _names_an_umbrella(body: str) -> bool:
    return any(u in body for u in UMBRELLAS)


def _line_of(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def scan_text(rel: str, raw: str) -> tuple[list[str], int]:
    """Return (violations, ruled-private-count) for one cmake file's text."""
    text = _strip_comments(raw)
    violations: list[str] = []
    private = 0

    # issue 1615 (W6): an umbrella reached through a VARIABLE is the same link.
    # `set(_u NanoRos::NanoRosCpp)` + `target_link_libraries(t INTERFACE ${_u})`
    # named no umbrella in the call, so it was never read. Variables BOUND to an
    # umbrella in this file (`set` / `list(APPEND)`) are expanded in the body.
    bound = {mm.group(1) for mm in re.finditer(
        r"\b(?:set|list\s*\(\s*APPEND)\s*\(?\s*([A-Za-z_][A-Za-z0-9_]*)\b([^()]*)\)",
        text) if _names_an_umbrella(mm.group(2))}
    for m in TLL.finditer(text):
        body = m.group(1)
        if not (_names_an_umbrella(body)
                or any("${" + v + "}" in body for v in bound)):
            continue
        line = _line_of(text, m.start())
        if re.search(r"\b(PUBLIC|INTERFACE)\b", body):
            violations.append(
                f"{rel}:{line}: target_link_libraries(... "
                f"PUBLIC/INTERFACE ... <umbrella>) — a PROPAGATED umbrella "
                f"must go through nros_link_runtime_umbrella() "
                f"({RESOLVER}), which guards it on the consuming binary "
                f"(issue 1467)."
            )
        elif not re.search(r"\bPRIVATE\b", body):
            # The scope is a VARIABLE (`${_link_type}`) or absent. This is how
            # the reported site read on main — a PUBLIC-or-INTERFACE link whose
            # keyword the gate cannot see — so it must not fall into the
            # ruled-PRIVATE bucket by default.
            violations.append(
                f"{rel}:{line}: target_link_libraries(... <umbrella>) with "
                f"no literal PRIVATE/PUBLIC/INTERFACE keyword, so whether "
                f"the umbrella PROPAGATES cannot be read here. Call "
                f"nros_link_runtime_umbrella() and pass the scope to it "
                f"(issue 1467)."
            )
        else:
            private += 1

    for m in PROP_WRITE.finditer(text):
        body = m.group(1)
        if "INTERFACE_LINK_LIBRARIES" not in body:
            continue
        if not _names_an_umbrella(body):
            continue
        line = _line_of(text, m.start())
        violations.append(
            f"{rel}:{line}: a write of INTERFACE_LINK_LIBRARIES naming an "
            f"umbrella — resolve it with nros_runtime_umbrella_expr(... "
            f"GUARDED) so the consuming binary can decline it (issue 1467)."
        )

    return violations, private


# Negative controls. A gate that cannot fail prints the same OK line as one that
# works, so each detector is exercised against a snippet that must trip it and
# against the shape it must NOT trip on.
SELF_TEST_CASES: list[tuple[str, str, int, int]] = [
    # (name, cmake text, expected violations, expected ruled-private)
    (
        "propagated PUBLIC link — the reported class",
        'target_link_libraries(std_msgs__nano_ros_c PUBLIC NanoRos::NanoRosCpp)\n',
        1,
        0,
    ),
    (
        "propagated INTERFACE link",
        'target_link_libraries(iface INTERFACE NanoRos::NanoRos)\n',
        1,
        0,
    ),
    (
        "scope held in a variable — the spelling main actually carried",
        'target_link_libraries(lib ${_link_type} NanoRos::NanoRosCpp)\n',
        1,
        0,
    ),
    (
        "INTERFACE_LINK_LIBRARIES property write (the _ffi_lib ordering hook)",
        "set_property(TARGET x APPEND PROPERTY\n"
        "  INTERFACE_LINK_LIBRARIES NanoRos::NanoRosCpp)\n",
        1,
        0,
    ),
    (
        "multi-line call, umbrella on its own line",
        "target_link_libraries(exe PUBLIC\n    ${other}\n    nros_cpp::nros_cpp)\n",
        1,
        0,
    ),
    (
        "PRIVATE link — allowed, counted, not a violation",
        'target_link_libraries(${_NRA_NAME} PRIVATE NanoRos::NanoRosCpp)\n',
        0,
        1,
    ),
    (
        "the resolved call — no literal umbrella name at all",
        "nros_link_runtime_umbrella(${_lib_target} PUBLIC)\n",
        0,
        0,
    ),
    (
        "a COMMENTED example must not count",
        "#     target_link_libraries(my_app PUBLIC NanoRos::NanoRos)\n",
        0,
        0,
    ),
    (
        "the umbrella's own definition links a non-umbrella target",
        "target_link_libraries(NanoRos INTERFACE nros_c-static)\n",
        0,
        0,
    ),
]


def self_test(quiet: bool = False) -> int:
    failures = []
    for name, text, want_v, want_p in SELF_TEST_CASES:
        got_v, got_p = scan_text("selftest.cmake", text)
        if len(got_v) != want_v or got_p != want_p:
            failures.append(
                f"  {name}: expected {want_v} violation(s)/{want_p} private, "
                f"got {len(got_v)}/{got_p}"
            )
    if failures:
        print("check-runtime-umbrella-link-sites --self-test: FAIL", file=sys.stderr)
        for f in failures:
            print(f, file=sys.stderr)
        return 1
    if not quiet:
        print(
            f"check-runtime-umbrella-link-sites --self-test: OK "
            f"({len(SELF_TEST_CASES)} controls)"
        )
    return 0


def main() -> int:
    if "--self-test" in sys.argv[1:]:
        return self_test()
    # ALWAYS, not only behind the flag: a negative control nobody runs decays
    # into a comment, and this rule's whole job is to fire. (`check-gate-selftests`
    # enforces exactly this, and caught the flag-only shape here.)
    rc = self_test(quiet=True)
    if rc != 0:
        return rc

    listing = subprocess.run(
        ["git", "ls-files", "cmake", "CMakeLists.txt", "nano_rosConfig.cmake"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split()
    files = [
        p
        for p in listing
        if (p.endswith(".cmake") or p.endswith("CMakeLists.txt")) and p != RESOLVER
    ]

    violations: list[str] = []
    private_counts: dict[str, int] = {}

    for rel in files:
        raw = (ROOT / rel).read_text(encoding="utf-8", errors="replace")
        file_violations, private = scan_text(rel, raw)
        violations.extend(file_violations)
        if private:
            private_counts[rel] = private

    # The ruled-private ledger, both directions.
    for rel, count in sorted(private_counts.items()):
        ruled = RULED_PRIVATE.get(rel)
        if ruled is None:
            violations.append(
                f"{rel}: {count} PRIVATE umbrella link(s) at a site this gate "
                f"does not rule. A PRIVATE link is allowed — the consumer IS "
                f"the target — but it must be RULED in "
                f"scripts/check-runtime-umbrella-link-sites.py with a reason, "
                f"so the next one is a decision and not a copy."
            )
        elif count != ruled[0]:
            violations.append(
                f"{rel}: {count} PRIVATE umbrella link(s), ruled for "
                f"{ruled[0]} ({ruled[1]}). Update the ruling and say why the "
                f"new site is the binary's own decision."
            )
    for rel, (count, reason) in sorted(RULED_PRIVATE.items()):
        if rel not in private_counts:
            violations.append(
                f"{rel}: ruled for {count} PRIVATE umbrella link(s) "
                f"({reason}) and has none. Drop the ruling — a stale entry "
                f"tolerates a site nobody checked."
            )

    if violations:
        print("check-runtime-umbrella-link-sites: FAIL", file=sys.stderr)
        for v in violations:
            print(f"  {v}", file=sys.stderr)
        return 1

    total_private = sum(private_counts.values())
    print(
        f"check-runtime-umbrella-link-sites: OK "
        f"({len(files)} cmake file(s); 0 unresolved propagated umbrella links; "
        f"{total_private} ruled PRIVATE link(s) in "
        f"{len(private_counts)} file(s))"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
