#!/usr/bin/env python3
"""Issue 1523 — `NROS_COMPONENT_LANG` has ONE vocabulary, and it is lowercase.

CLAUDE.md's pitfall index has carried "case-normalize enum-ish cmake args" since
the ament verbs started passing lowercase `cpp` into a `STREQUAL "CPP"`. It had
no gate, and it recurred on a target PROPERTY rather than an argument:

  * `nano_ros_auto_add_library()` wrote `NROS_COMPONENT_LANG` LOWERCASE — the
    canonical `nros_lang::Language::as_str` spelling, which is also the on-disk
    serde contract and what `nros codegen source-language` answers;
  * `nano_ros_node_register()` wrote the same property UPPERCASE, from its own
    ~30-comparison internal vocabulary;
  * all three readers tested `STREQUAL "C"`.

`STREQUAL` is case-sensitive, so for every `nano_ros_auto_add_library` target
all three readers were permanently FALSE: `LINKER_LANGUAGE C` was never set, the
`NOT … STREQUAL "C"` umbrella branch was always taken, and
`nros_components_register_node`'s reader was dead code for one of its two
producers. Nothing visibly broke, because the two wrong branches cancelled into
the link line issue 0425 wants — which is the whole reason a case mismatch on an
enum-ish cmake string needs a gate rather than a reader.

WHAT IS ENFORCED

  1. THE WRITE HAS ONE SPELLING. No site outside
     `cmake/NanoRosComponentLang.cmake` may `set_property` /
     `set_target_properties` the `NROS_COMPONENT_LANG` property; it goes through
     `_nros_set_component_lang()`, which REFUSES a non-canonical value. A gate
     can only read the variable NAME a write hands over, never its value, so the
     value check belongs at configure time and this rule is what keeps every
     write reaching it.

  2. EVERY READER COMPARES THE CANONICAL SPELLING. Two producers of a canonical
     lowercase language are tracked — `get_target_property(<v> <t>
     NROS_COMPONENT_LANG)`, and the shared inference `_nros_infer_lang(<v> …)` /
     `nros_language_of_sources(<v> …)` (phase-469 S3, the one place the
     extension→language answer comes from). A `STREQUAL` against any variable
     bound by one of those must name a value from the canonical set, spelled
     exactly. `"C"` is the reported defect; `"CXX"` and `"Cpp"` are the same
     mistake with a different shift key.

WHAT IS OUT OF SCOPE, and why

  * A `STREQUAL` against a variable this file cannot trace to a canonical
    producer. `nano_ros_node_register()` keeps a deliberate UPPERCASE internal
    vocabulary and converts at both boundaries; policing its ~30 comparisons
    would mean declaring which variables are which, which is an authored map
    and drifts. What matters is that nothing uppercase LEAVES that file, and
    rule 1 is what says so.
  * `examples/**` and fixtures — the property is internal to the verbs.
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

PROPERTY = "NROS_COMPONENT_LANG"
OWNER = "cmake/NanoRosComponentLang.cmake"
SETTER = "_nros_set_component_lang"

# The canonical vocabulary, mirroring `_nros_set_component_lang`'s own list and
# `nros_lang::Language::as_str`.
CANONICAL = ("c", "cpp", "rust")

# Commands whose FIRST argument is a variable holding a canonical lowercase
# language. `get_target_property(<var> <target> NROS_COMPONENT_LANG)` is handled
# separately because its third argument is what identifies it.
LANG_PRODUCERS = ("_nros_infer_lang", "nros_language_of_sources")

# `<cmd>(<args>)`. cmake arguments here never contain a literal paren — `${VAR}`,
# `$<GENEX:…>` and target names are all paren-free — so a non-greedy no-paren
# body matches the call exactly.
PROP_WRITE = re.compile(
    r"(?:set_property|set_target_properties)\s*\(([^()]*)\)", re.DOTALL
)
GET_PROP = re.compile(r"get_target_property\s*\(\s*(\w+)\s+\S+\s+(\w+)\s*\)"
                      r"|get_property\s*\(\s*(\w+)\s+TARGET\s+\S+\s+PROPERTY\s+(\w+)\s*\)")
PRODUCER = re.compile(
    r"\b(" + "|".join(LANG_PRODUCERS) + r")\s*\(\s*(\w+)\b"
)
# `if(… <var> STREQUAL "<literal>" …)` — also matches `NOT <var> STREQUAL` and
# the dereferenced spelling `"${var}" STREQUAL`. cmake's `if()` takes the bare
# variable NAME, which is the form every site in this tree uses.
STREQUAL = re.compile(r"\"?(?:\$\{)?(\w+)\}?\"?\s+STREQUAL\s+\"([^\"]*)\"")


def _strip_comments(text: str) -> str:
    """Blank out `#` comments, keeping line structure for accurate numbers."""
    # phase-472 W3 — the shared stripper (scripts/lib/comments.py).
    return comments.strip_comments(text, "cmake")


def _line_of(text: str, offset: int) -> int:
    return text.count("\n", 0, offset) + 1


def scan_text(rel: str, raw: str) -> tuple[list[str], int, int]:
    """Return (violations, property-writes, checked-comparisons) for one file."""
    text = _strip_comments(raw)
    violations: list[str] = []
    writes = 0
    compared = 0

    # Rule 1 — the write has one spelling.
    for m in PROP_WRITE.finditer(text):
        body = m.group(1)
        if PROPERTY not in body:
            continue
        writes += 1
        if rel != OWNER:
            violations.append(
                f"{rel}:{_line_of(text, m.start())}: writes {PROPERTY} "
                f"directly. Call {SETTER}(<target> <lang>) "
                f"({OWNER}) — it refuses a value outside the canonical "
                f"lowercase vocabulary {list(CANONICAL)}, which is what made "
                f"three case-sensitive readers disagree (issue 1523)."
            )

    # Rule 2 — every reader of a canonical-lowercase variable compares a
    # canonical lowercase literal.
    # Two strengths, because the two producers answer different questions.
    # `strict` = the property, whose whole vocabulary is CANONICAL: it is
    # written by one function that refuses anything else, so a reader naming
    # something outside the set is comparing against a value that cannot occur.
    # `lowercase` = the shared inference, where a reader may legitimately name
    # an ACCEPTED ALIAS the producer never returns — `nano_ros_add_node` maps
    # `cxx` onto `cpp` on the caller's own input, in the same variable. The rule
    # that binds both, and the one the pitfall is about, is the CASE.
    strict_vars: dict[str, str] = {}
    lower_vars: dict[str, str] = {}
    # issue 1615 (W6): `get_property(<v> TARGET <t> PROPERTY <p>)` reads the
    # property exactly as `get_target_property` does.
    for m in GET_PROP.finditer(text):
        var, prop = (m.group(1), m.group(2)) if m.group(1) else (m.group(3), m.group(4))
        if prop == PROPERTY:
            strict_vars[var] = f"get_target_property(... {PROPERTY})"
    for m in PRODUCER.finditer(text):
        lower_vars[m.group(2)] = f"{m.group(1)}()"

    for m in STREQUAL.finditer(text):
        var, literal = m.group(1), m.group(2)
        origin = strict_vars.get(var)
        strict = origin is not None
        if origin is None:
            origin = lower_vars.get(var)
        if origin is None:
            continue
        compared += 1
        if strict:
            if literal in CANONICAL:
                continue
            expectation = (
                f"which carries the canonical vocabulary {list(CANONICAL)} — "
                f"one function writes that property and refuses anything else"
            )
        else:
            if literal == literal.lower():
                continue
            expectation = (
                f"which answers in the canonical LOWERCASE spelling "
                f"(`Language::as_str`)"
            )
        violations.append(
            f"{rel}:{_line_of(text, m.start())}: `{var} STREQUAL \"{literal}\"`"
            f" — `{var}` is bound by {origin}, {expectation}. STREQUAL is "
            f"case-sensitive, so this is a branch that never runs "
            f"(issue 1523)."
        )

    return violations, writes, compared


# Negative controls. A gate that cannot fail prints the same OK line as one that
# works, so each detector is exercised against a snippet that must trip it AND
# against the shape it must not trip on.
SELF_TEST_CASES: list[tuple[str, str, str, int]] = [
    # (name, path-as-scanned, cmake text, expected violations)
    (
        "the reported defect — uppercase literal against the property",
        "cmake/Probe.cmake",
        'get_target_property(_l ${t} NROS_COMPONENT_LANG)\n'
        'if(_l STREQUAL "C")\n'
        'endif()\n',
        1,
    ),
    (
        "the same defect against the shared inference's answer",
        "cmake/Probe.cmake",
        "_nros_infer_lang(_lang ${_srcs})\n"
        'if(NOT _lang STREQUAL "C")\n'
        "endif()\n",
        1,
    ),
    (
        "a near-miss spelling is not a pass",
        "cmake/Probe.cmake",
        "nros_language_of_sources(_lang SOURCES a.c)\n"
        'if(_lang STREQUAL "CXX")\n'
        "endif()\n",
        1,
    ),
    (
        "an ACCEPTED ALIAS against the inference is fine — it is lowercase "
        "(nano_ros_add_node maps `cxx` onto `cpp` in the same variable)",
        "cmake/Probe.cmake",
        "_nros_infer_lang(_lang ${_srcs})\n"
        'if(_lang STREQUAL "cxx")\n'
        "endif()\n",
        0,
    ),
    (
        "the same alias against the PROPERTY is not — nothing writes it",
        "cmake/Probe.cmake",
        "get_target_property(_l ${t} NROS_COMPONENT_LANG)\n"
        'if(_l STREQUAL "cxx")\n'
        "endif()\n",
        1,
    ),
    (
        "the fixed shape — canonical lowercase",
        "cmake/Probe.cmake",
        "_nros_infer_lang(_lang ${_srcs})\n"
        'get_target_property(_l ${t} NROS_COMPONENT_LANG)\n'
        'if(_lang STREQUAL "c" AND NOT _l STREQUAL "cpp")\n'
        "endif()\n",
        0,
    ),
    (
        "an unrelated STREQUAL on an untracked variable is not this rule",
        "cmake/Probe.cmake",
        'if(_NRC_SHAPE STREQUAL "rclcpp")\n'
        "endif()\n",
        0,
    ),
    (
        "a direct property write outside the owner",
        "cmake/Probe.cmake",
        "set_property(TARGET x PROPERTY NROS_COMPONENT_LANG \"${_l}\")\n",
        1,
    ),
    (
        "the same write inside the owner is the one legal site",
        OWNER,
        "set_property(TARGET ${target} PROPERTY NROS_COMPONENT_LANG \"${lang}\")\n",
        0,
    ),
    (
        "a comment naming the wrong spelling is prose, not code",
        "cmake/Probe.cmake",
        "_nros_infer_lang(_lang ${_srcs})\n"
        '# this used to read `_lang STREQUAL "C"`, which never fired\n',
        0,
    ),
]


def self_test(quiet: bool = False) -> int:
    failures = []
    for name, rel, text, expected in SELF_TEST_CASES:
        got, _, _ = scan_text(rel, text)
        if len(got) != expected:
            failures.append(
                f"{name}: expected {expected} violation(s), got {len(got)}"
                + ("" if not got else " — " + "; ".join(got))
            )
    if failures:
        print("check-component-lang-vocabulary --self-test: FAIL", file=sys.stderr)
        for f in failures:
            print(f"  {f}", file=sys.stderr)
        return 1
    if not quiet:
        print(
            f"check-component-lang-vocabulary --self-test: OK "
            f"({len(SELF_TEST_CASES)} controls)"
        )
    return 0


def main() -> int:
    if "--self-test" in sys.argv[1:]:
        return self_test()
    # ALWAYS, not only behind the flag: a negative control nobody runs decays
    # into a comment (`check-gate-selftests` enforces exactly this).
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
        p for p in listing if p.endswith(".cmake") or p.endswith("CMakeLists.txt")
    ]

    violations: list[str] = []
    writes = 0
    compared = 0
    for rel in files:
        v, w, c = scan_text(rel, (ROOT / rel).read_text(encoding="utf-8", errors="replace"))
        violations.extend(v)
        writes += w
        compared += c

    # Vacuity guard. "0 violations" is also what a scan that matched nothing
    # prints, and both detectors depend on regexes over a file set that could
    # silently narrow.
    if writes < 1:
        violations.append(
            f"harvested {writes} {PROPERTY} write(s) — the writer detector "
            f"matched nothing, so rule 1 could not have fired."
        )
    if compared < 3:
        violations.append(
            f"harvested only {compared} comparison(s) against a canonical "
            f"language variable — the reader detector matched (almost) "
            f"nothing, so rule 2 could not have fired."
        )

    if violations:
        print("check-component-lang-vocabulary: FAIL", file=sys.stderr)
        for v in violations:
            print(f"  {v}", file=sys.stderr)
        return 1

    print(
        f"check-component-lang-vocabulary: OK ({len(files)} cmake file(s); "
        f"{writes} {PROPERTY} write(s), all in {OWNER}; {compared} canonical "
        f"language comparison(s), all lowercase)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
