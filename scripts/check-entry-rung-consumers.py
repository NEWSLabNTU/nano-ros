#!/usr/bin/env python3
"""issue 1531 — a baked BOOT-CONFIG rung must reach EVERY language surface.

`nano_ros_entry()` bakes `NROS_ENTRY_*` onto the entry target regardless of the
entry's language, so a rung is available to C and C++ alike. Consuming it is a
separate act, and for `NROS_ENTRY_RMW` only C++ ever did: the C road held its own
answer in its own preprocessor and passed `NULL` to `nros_support_init`.

That asymmetry was invisible for three weeks and cost a high-severity outage. The
same commit that added the rung (issue 1050) made a selector-less open with more
than one registered backend a hard refusal, so C got the new failure mode and not
the new capability; issue 1530 is what it cost, with every native C example
failing `nros_support_init` for eighteen days behind an absorbing stale verdict.

## Scope, and why it is DERIVED rather than authored

Plain symmetry over every `NROS_ENTRY_*` is the wrong rule: `NROS_ENTRY_MAX_NODES`
and `NROS_ENTRY_MAX_ENTITIES` are legitimately C++-only (they size C++ storage
templates), and `NROS_ENTRY_IP_LAST` is a per-member network knob no language
surface reads. A rule that fires on those needs an exemption list, which is only
as complete as whoever wrote it — the problem this gate exists to answer, one
level up.

So the scope is the RFC-0045 precedence ladder: the rungs that are BOTH

  * baked by cmake onto an entry target (read out of the cmake), AND
  * a field of `nros_node::BootConfig` (read out of the Rust),

because those are exactly the facts every language's session open resolves. Both
sides are read from source; nothing here enumerates a rung. A new ladder field
that cmake learns to bake joins the scope automatically, and a rung outside the
ladder is out of scope without anybody exempting it.

The rule: an in-scope rung consumed by one surface must be consumed by the other.
Consumed by NEITHER is fine — that is a rung nothing has adopted yet, not a
surface left behind.
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# Where cmake bakes onto an entry target.
CMAKE_PRODUCERS = [
    Path("cmake/NanoRosEntry.cmake"),
    Path("integrations/px4/NanoRosPx4Module.cmake"),
]

# The ladder's own definition.
BOOT_CONFIG = Path("packages/core/nros-node/src/executor/types.rs")

# The two language surfaces. A rung reaches a surface through its HEADERS —
# that is what an out-of-tree consumer compiles against.
SURFACES = {
    "C": Path("packages/api/nros-c/include"),
    "C++": Path("packages/api/nros-cpp/include"),
}

BAKE_RE = re.compile(r"\bNROS_ENTRY_([A-Z][A-Z0-9_]*)\s*=")
USE_RE = re.compile(r"\bNROS_ENTRY_([A-Z][A-Z0-9_]*)\b")
FIELD_RE = re.compile(r"^\s*pub ([a-z][a-z0-9_]*):", re.MULTILINE)


def baked_rungs(texts: dict[str, str]) -> set[str]:
    """Rung tails cmake bakes onto a target, e.g. `{"RMW", "LOCATOR", ...}`."""
    found: set[str] = set()
    for text in texts.values():
        found |= {m.group(1) for m in BAKE_RE.finditer(text)}
    return found


def ladder_fields(text: str) -> set[str]:
    """`BootConfig`'s field names, upper-cased to the macro spelling."""
    start = text.find("pub struct BootConfig")
    if start < 0:
        sys.exit(
            f"{BOOT_CONFIG}: `pub struct BootConfig` not found — this gate's scope "
            f"is derived from its fields, and an empty scope would pass vacuously."
        )
    end = text.find("\n}", start)
    if end < 0:
        sys.exit(f"{BOOT_CONFIG}: BootConfig's body is unterminated.")
    fields = {m.group(1).upper() for m in FIELD_RE.finditer(text[start:end])}
    if not fields:
        sys.exit(f"{BOOT_CONFIG}: BootConfig has no fields — refusing an empty scope.")
    return fields


def consumers(rungs: set[str], surface_text: dict[str, str]) -> dict[str, set[str]]:
    """surface -> the in-scope rungs it names."""
    return {
        name: {m.group(1) for m in USE_RE.finditer(text)} & rungs
        for name, text in surface_text.items()
    }


def compare(scope: set[str], used: dict[str, set[str]]) -> list[str]:
    """The rule. One message per asymmetry; empty means OK."""
    errors = []
    for rung in sorted(scope):
        have = sorted(s for s, r in used.items() if rung in r)
        miss = sorted(s for s, r in used.items() if rung not in r)
        if have and miss:
            errors.append(
                f"NROS_ENTRY_{rung} is baked onto every entry target and is a "
                f"BootConfig field, but only {', '.join(have)} consume(s) it — "
                f"{', '.join(miss)} do(es) not.\n"
                f"      A rung one surface cannot read is a capability that "
                f"language does not have, while the image carries the answer in "
                f"its own preprocessor. That is issue 1531: it cost every native "
                f"C example its session open for eighteen days (issue 1530).\n"
                f"      Give the missing surface a consumer — for C that is a "
                f"header-side spelling reached by <nros/types.h>, AFTER the "
                f"generated declarations (see <nros/baked_rmw.h>), so every "
                f"consumer gets it rather than only those including one module."
            )
    return errors


def self_test() -> None:
    """Negative controls — a gate whose rule never fires proves nothing."""
    # -- the readers --
    assert baked_rungs({"a": 'target_compile_definitions(t PRIVATE "NROS_ENTRY_RMW=\\"z\\"")'}) == {
        "RMW"
    }
    assert baked_rungs({"a": '"NROS_ENTRY_DOMAIN_ID=${X}"'}) == {"DOMAIN_ID"}
    # A mere MENTION is not a bake: the tail needs `=`.
    assert baked_rungs({"a": "# NROS_ENTRY_RMW is baked below"}) == set()
    assert ladder_fields(
        "pub struct BootConfig<'a> {\n    pub locator: Option<&'a str>,\n"
        "    pub rmw: Option<&'a str>,\n}\n"
    ) == {"LOCATOR", "RMW"}

    # -- the rule --
    assert compare({"RMW"}, {"C": {"RMW"}, "C++": {"RMW"}}) == []
    # Consumed by NEITHER is not an asymmetry.
    assert compare({"RMW"}, {"C": set(), "C++": set()}) == []
    # The 1531 shape itself.
    fired = compare({"RMW"}, {"C": set(), "C++": {"RMW"}})
    assert len(fired) == 1 and "RMW" in fired[0] and "C do(es) not" in fired[0], fired
    # ... and the mirror, so the rule is not written in one direction only.
    fired = compare({"LOCATOR"}, {"C": {"LOCATOR"}, "C++": set()})
    assert len(fired) == 1 and "C++ do(es) not" in fired[0], fired


def read_tracked(paths: list[Path]) -> dict[str, str]:
    """`git ls-files`, not a walk — issue 0844, enforced by check-no-tracked-file-find."""
    out: dict[str, str] = {}
    r = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "--", *(p.as_posix() for p in paths)],
        capture_output=True,
        text=True,
        check=False,
    )
    if r.returncode != 0:
        sys.exit(f"check-entry-rung-consumers: `git ls-files` failed:\n  {r.stderr.strip()}")
    for rel in (x.strip() for x in r.stdout.splitlines()):
        if rel:
            out[rel] = (ROOT / rel).read_text(errors="replace")
    return out


def main() -> int:
    self_test()
    if "--self-test" in sys.argv:
        print("check-entry-rung-consumers self-test: OK")
        return 0

    cmake = read_tracked(CMAKE_PRODUCERS)
    if not cmake:
        sys.exit(
            "check-entry-rung-consumers: no cmake producer found — refusing to "
            "compare an EMPTY baked set, which would pass vacuously."
        )
    baked = baked_rungs(cmake)
    ladder = ladder_fields((ROOT / BOOT_CONFIG).read_text(errors="replace"))
    scope = baked & ladder
    if not scope:
        sys.exit(
            "check-entry-rung-consumers: the baked set and BootConfig's fields do "
            "not intersect. One of the two readers has drifted; an empty scope "
            "would pass vacuously."
        )

    surface_text = {}
    for name, root in SURFACES.items():
        files = read_tracked([root])
        if not files:
            sys.exit(f"check-entry-rung-consumers: no tracked headers under {root}.")
        surface_text[name] = "\n".join(files.values())

    used = consumers(scope, surface_text)
    errors = compare(scope, used)
    if errors:
        print("check-entry-rung-consumers: FAIL", file=sys.stderr)
        for e in errors:
            print(f"  - {e}", file=sys.stderr)
        return 1

    shared = sorted(r for r in scope if all(r in u for u in used.values()))
    unadopted = sorted(r for r in scope if not any(r in u for u in used.values()))
    print(
        f"check-entry-rung-consumers: OK — {len(scope)} ladder rung(s) baked: "
        f"{len(shared)} on every surface ({', '.join('NROS_ENTRY_' + r for r in shared)})"
        + (
            f"; {len(unadopted)} adopted by none ({', '.join(unadopted)})"
            if unadopted
            else ""
        )
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
