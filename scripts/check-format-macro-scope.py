#!/usr/bin/env python3
"""RFC-0088 D5 — a bridge-linked image must not reference the format macros.

`NROS_SERIALIZATION_FORMAT_ID` / `NROS_SERIALIZATION_FORMAT` (and their
`NROS_CPP_`-prefixed C++ FFI siblings, and the `nros::linked_format()` /
`nros::linked_format_name()` accessors that lift them) answer ONE question:
*what encoding does the backend this image links speak?* That question has an
answer exactly when the image links one backend — which is the ordinary case,
and the reason the whole check is compile-time rather than a `dlopen`-resolved
string the way ROS 2 does it.

A **bridge image is the exception, structurally**. `Executor::open_multi` opens
two sessions on two backends in one process (RFC-0088 D3); the format stops
being a property of the image and becomes a property of the session. There the
macro is not merely useless, it is *actively wrong*: it names one of the two
backends, silently, with no diagnostic — a subtle wrong answer where the whole
point of the design was to make a mismatch loud. A bridge asks per session
instead:

    C    nros_node_get_serialization_format(node)
    C++  node.serialization_format()
    Rust session.serialization_format()  /  RawSubscription::format()

## What this gate can and cannot see

"Image" is a LINK-time notion and this is a SOURCE-time gate, so the predicate
is a documented approximation, deliberately conservative in the direction that
matters:

  * a translation unit is **bridge-linked** when it names the bridge API
    (`<nros/bridge.h>`, `<nros/bridge.hpp>`, `nros_pubsub_bridge_create`,
    `nros::bridge::`, `nros::MultiExecutor`), or when any sibling source under
    the same *entry root* does — an entry root being the nearest ancestor
    directory holding a `CMakeLists.txt`, `package.xml` or `Cargo.toml`, i.e.
    the unit that becomes one image;
  * it therefore over-approximates (every TU of a bridge entry is treated as
    bridge-linked, even one that only ever touches a single backend) and cannot
    catch an image assembled entirely outside the tree.

Over-approximating is the right error to make: the cost is a diagnostic on a
line that could have stayed, and the fix — ask the session — is correct in both
cases anyway.

## Not vacuous

Two guards, because a gate whose subject set has silently emptied reports the
same green as a gate that passed:

  * the macros must still EXIST at their defining sites. A rename that this
    file did not follow fails here rather than making every scan trivially
    clean;
  * at least one bridge-linked TU must be found. If the bridge API is renamed
    and the patterns above stop matching, the gate says so.

Usage:
    python3 scripts/check-format-macro-scope.py [--list]

`--list` prints the classification (bridge-linked TUs, macro references) and
exits 0 — for working out why a file was picked up.
"""
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

C_LIKE = {".c", ".h", ".cc", ".cpp", ".cxx", ".hpp", ".hh", ".hxx", ".inc"}

# ── The macros, and every spelling of the same fact ──────────────────────────
#
# The RFC names the two C macros. The C++ FFI header carries its own
# `NROS_CPP_`-prefixed copy (emitted there so no C++ header has to include a C
# API header — issue 0160's one-way include order), and `serialization_format.hpp`
# lifts both into `nros::linked_format()`. All four answer "what does THIS image
# speak", so all four are wrong in a bridge; gating only the two the RFC spells
# would be the narrower-than-the-rule coverage issue 0196 is about.
MACRO_REFS = re.compile(
    r"\b(?:"
    r"NROS_SERIALIZATION_FORMAT_ID"
    r"|NROS_SERIALIZATION_FORMAT"
    r"|NROS_CPP_SERIALIZATION_FORMAT_ID"
    r"|NROS_CPP_SERIALIZATION_FORMAT"
    r"|linked_format_name"
    r"|linked_format"
    r")\b"
)

# `NROS_SERIALIZATION_FORMAT_ID_CDR` / `_UORB` are the reserved-discriminant
# TABLE, not the image's answer — naming a format is fine anywhere, including a
# bridge, which is the one place two of them legitimately meet.
TABLE_REF = re.compile(r"\bNROS_(?:CPP_)?SERIALIZATION_FORMAT_ID_[A-Z]+\b")

BRIDGE_MARKERS = re.compile(
    r"(?:"
    r"#\s*include\s*[<\"]nros/bridge\.h(?:pp)?[>\"]"
    r"|\bnros_pubsub_bridge_create\b"
    r"|\bnros_bridge_endpoint_t\b"
    r"|\bnros::bridge::"
    r"|\bnros::MultiExecutor\b"
    r")"
)

# ── Where the macros are DEFINED / derived. These are the API, not an image ──
#
# Each entry is (path, the token that must appear in it). The token half is the
# vacuity guard: if a rename moves the macro and nobody updates this file, the
# gate fails loudly instead of scanning for a string that no longer exists.
DEFINITION_SITES = [
    ("packages/api/nros-c/include/nros/serialization_format.h", "NROS_SERIALIZATION_FORMAT_ID"),
    ("packages/api/nros-c/include/nros/nros_generated.h", "NROS_SERIALIZATION_FORMAT_ID"),
    ("packages/api/nros-cpp/include/nros/serialization_format.hpp", "linked_format"),
    ("packages/api/nros-cpp/include/nros/nros_cpp_ffi.h", "NROS_CPP_SERIALIZATION_FORMAT_ID"),
]

# Files exempt from the "must not reference" rule: the API that defines the
# macros, the compile probes that exist to prove the assertion has teeth, and
# `bridge.{h,hpp}` itself (its prose explains why a bridge does not use them).
EXEMPT_PREFIXES = (
    "packages/api/nros-c/include/nros/",
    "packages/api/nros-cpp/include/nros/",
    "packages/api/nros-c/tests/compile/",
    "packages/api/nros-cpp/tests/compile/",
)

ENTRY_ROOT_MARKERS = ("CMakeLists.txt", "package.xml", "Cargo.toml")


def tracked_files():
    out = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "-z"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout
    return [p for p in out.split("\0") if p]


def entry_root(rel: str) -> str:
    """Nearest ancestor directory that becomes one image, as a repo-relative path."""
    d = (ROOT / rel).parent
    while True:
        if any((d / m).exists() for m in ENTRY_ROOT_MARKERS):
            return str(d.relative_to(ROOT))
        if d == ROOT:
            return "."
        d = d.parent


def is_exempt(rel: str) -> bool:
    return rel.startswith(EXEMPT_PREFIXES)


def read(rel: str) -> str:
    try:
        return (ROOT / rel).read_text(encoding="utf-8", errors="replace")
    except OSError:
        return ""



def self_test(quiet: bool = False) -> None:
    """Negative controls — a gate whose rule never fires proves nothing.

    Each case asserts the rule FIRES on the shape it exists for, then that the
    intended escape silences it. These run on the NORMAL path (see the call in
    `main`), because a control nobody runs decays into a comment.
    """
    cases = []

    def case(name: str, ok: bool) -> None:
        cases.append((name, ok))

    # 1. The image-wide macros are caught, in both language spellings.
    for macro in (
        "NROS_SERIALIZATION_FORMAT_ID",
        "NROS_SERIALIZATION_FORMAT",
        "NROS_CPP_SERIALIZATION_FORMAT_ID",
        "NROS_CPP_SERIALIZATION_FORMAT",
    ):
        line = f"    uint8_t id = {macro};"
        case(f"{macro} is caught", bool(MACRO_REFS.search(TABLE_REF.sub("", line))))

    # 2. The C++ accessors are caught too — `linked_format()` answers the same
    #    question the macro does, so gating only the macros would be the
    #    narrower-than-the-rule coverage issue 0196 is about.
    for expr in ("nros::linked_format()", "nros::linked_format_name()"):
        case(f"{expr} is caught", bool(MACRO_REFS.search(TABLE_REF.sub("", expr))))

    # 3. The reserved-discriminant TABLE is NOT a violation. Naming a format is
    #    fine anywhere, including a bridge — the one place two legitimately
    #    meet. This is the escape, and it must silence the rule. The stripping
    #    is what makes it work, so the case is written against a line where the
    #    table entry is the ONLY token: without TABLE_REF this must go red.
    for line in (
        "NROS_SERIALIZATION_FORMAT_ID_CDR",
        "NROS_CPP_SERIALIZATION_FORMAT_ID_UORB",
    ):
        stripped = TABLE_REF.sub("", line)
        case(f"table ref is stripped: {line[:34]}", stripped.strip() == "")

    # 4. A prefix of a macro name is not a match — the word boundary matters,
    #    or `NROS_SERIALIZATION_FORMATTER` would read as the macro.
    case(
        "a longer identifier is not a match",
        not MACRO_REFS.search(TABLE_REF.sub("", "int NROS_SERIALIZATION_FORMATTER = 0;")),
    )

    # 5. Bridge detection fires on each marker the API actually presents.
    for marker in (
        "#include <nros/bridge.h>",
        '#include "nros/bridge.hpp"',
        "    nros_pubsub_bridge_create(exec, &src, &dst);",
        "    nros_bridge_endpoint_t src;",
        "    nros::MultiExecutor exec;",
    ):
        case(f"bridge marker: {marker.strip()[:34]}", bool(BRIDGE_MARKERS.search(marker)))

    # 6. And does NOT fire on a plain nano-ros TU, or every image would be
    #    treated as a bridge and the gate would fail the whole tree.
    case(
        "a plain TU is not a bridge",
        not BRIDGE_MARKERS.search("#include <nros/nros.h>\n    nros_init();"),
    )

    failed = [n for n, ok in cases if not ok]
    if failed:
        print("check-format-macro-scope SELFTEST FAILED:")
        for n in failed:
            print(f"  - {n}")
        sys.exit(2)
    if not quiet:
        print(f"check-format-macro-scope self-test: OK ({len(cases)} case(s))")


def main() -> int:
    want_list = "--list" in sys.argv[1:]

    # The negative controls run on the NORMAL path. `check-gate-selftests`
    # requires that: a control reachable only behind a flag is a comment, and
    # this gate's whole value is that its two regexes fire on the right shapes
    # and stay silent on the reserved-discriminant table.
    verbose_selftest = "--self" in " ".join(sys.argv[1:])
    self_test(quiet=not verbose_selftest)
    if verbose_selftest:
        return 0

    # Vacuity guard 1 — the macros still exist where this gate thinks they do.
    missing = []
    for path, token in DEFINITION_SITES:
        body = read(path)
        if not body:
            missing.append(f"{path} (file not found)")
        elif token not in body:
            missing.append(f"{path} (no `{token}`)")
    if missing:
        print("check-format-macro-scope: FAIL — the macros this gate polices are not")
        print("       where it expects them, so every scan below would be vacuous:")
        for m in missing:
            print(f"         {m}")
        print()
        print("       Either the definition moved (update DEFINITION_SITES and")
        print("       MACRO_REFS together) or the cbindgen headers are unregenerated")
        print("       (`cargo run -p nros-cbindgen-headers`).")
        return 1

    sources = [p for p in tracked_files() if Path(p).suffix in C_LIKE]

    # Pass 1 — which entry roots contain bridge code, and which TUs name it.
    bridge_tus = []
    bridge_roots = set()
    for rel in sources:
        if BRIDGE_MARKERS.search(read(rel)):
            bridge_tus.append(rel)
            if not is_exempt(rel):
                bridge_roots.add(entry_root(rel))

    # Vacuity guard 2 — the bridge API is still recognisable.
    if not bridge_tus:
        print("check-format-macro-scope: FAIL — no bridge-linked source found at all.")
        print("       The bridge API was renamed and BRIDGE_MARKERS no longer matches,")
        print("       so this gate has no subject and would pass on anything.")
        return 1

    # Pass 2 — a bridge-linked TU must not reference the image-wide answer.
    violations = []
    for rel in sources:
        if is_exempt(rel):
            continue
        root = entry_root(rel)
        direct = rel in bridge_tus
        if not direct and root not in bridge_roots:
            continue
        body = read(rel)
        for n, line in enumerate(body.splitlines(), 1):
            # Strip the reserved-discriminant table before looking for the
            # image's answer — `..._ID_CDR` contains `..._ID` as a prefix.
            probe = TABLE_REF.sub("", line)
            if MACRO_REFS.search(probe):
                violations.append((rel, n, line.strip(), root, direct))

    if want_list:
        print("bridge-linked translation units:")
        for rel in sorted(bridge_tus):
            print(f"  {rel}")
        print()
        print("entry roots treated as bridge images:")
        for r in sorted(bridge_roots):
            print(f"  {r}")
        print()
        print(f"macro references inside them: {len(violations)}")
        for rel, n, line, _root, _direct in violations:
            print(f"  {rel}:{n}: {line}")
        return 0

    if violations:
        print("check-format-macro-scope: FAIL — a bridge-linked image references the")
        print("       image-wide serialization format (RFC-0088 D5). A bridge links two")
        print("       backends, so there is no single answer and the macro silently")
        print("       names one of them:")
        print()
        for rel, n, line, root, direct in violations:
            why = "names the bridge API" if direct else f"shares entry root `{root}` with bridge code"
            print(f"         {rel}:{n}  ({why})")
            print(f"           {line}")
        print()
        print("       Ask the SESSION instead — that is the one place the answer is")
        print("       per-backend and therefore true:")
        print("         C    nros_node_get_serialization_format(node)")
        print("         C++  node.serialization_format()")
        print("         Rust session.serialization_format() / RawSubscription::format()")
        return 1

    print(
        f"check-format-macro-scope: OK ({len(sources)} C/C++ source(s); "
        f"{len(bridge_tus)} bridge-linked, {len(bridge_roots)} bridge entry root(s); "
        "none reaches the image-wide format macro)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
