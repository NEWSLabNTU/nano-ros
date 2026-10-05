#!/usr/bin/env python3
"""The C++ API's hosted-only surface is an ENUMERATED family, not a count.

phase-476 W5. phase-456 W6 asked for "zero `NROS_CPP_HAS_*`, zero
`NROS_CPP_STD`" in `packages/api/nros-cpp/include` and measured it
unreachable: every remaining use gated surface that ported code calls. phase-476
W1-W4 gave each of those a freestanding spelling first, so what is left behind a
capability macro is no longer "the only way to call something". It is hosted
by design, and a count cannot tell that apart from "not yet ported". So the
acceptance became "zero outside the documented hosted-only family, with that
family enumerated rather than counted", and this gate is that sentence.

THE RULE. Every preprocessor conditional in the C++ headers that names a
capability macro (`NROS_CPP_HAS_*`, or `NROS_CPP_STD`) carries a trailing tag

    #ifdef NROS_CPP_HAS_STD_STRING // hosted-family: string-interop

whose id is a member of `FAMILY` below. A member states WHY that surface is
hosted-only and WHICH macros it may name. The gate fails in four ways:

  * an untagged capability conditional — new hosted-only surface nobody
    enumerated, which is how the count grew unnoticed before;
  * a tag whose id is not a family member;
  * a conditional naming a macro its member does not allow (a
    `string-interop` region that starts testing `NROS_CPP_HAS_SHARED_PTR`
    has changed what it is);
  * a member no conditional uses — a stale entry is a place to put the next
    violation, the argument phase-456 W6 made for its baselines.

What it does NOT check, and where that lives: whether a gated region changes a
LAYOUT (`check-cpp-capability-layout`), and whether a hosted STL include sits
outside an `NROS_CPP_STD` region (`check-cpp-freestanding-includes`). This one
answers only "is every hosted-only region one we decided to have?".
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
INCLUDE = "packages/api/nros-cpp/include"

# id -> (allowed macros, reason). The reason is what a reader of a tagged
# `#if` comes here to find.
FAMILY = {
    "capability-definition": (
        {"NROS_CPP_STD"},
        "the ONE definition site of the capability macros (`std_detect.hpp`): "
        "each `NROS_CPP_HAS_*` is defined iff the consumer asked for the porting "
        "surface with `NROS_CPP_STD` (phase-438 W2)",
    ),
    "hosted-console": (
        {"NROS_CPP_STD"},
        "a HOSTED SERVICE rather than API: `stderr` diagnostics, `getenv`, "
        "`fopen`. A freestanding target has no console, environment or file "
        "system to call, and the API surface is identical without them",
    ),
    "string-interop": (
        {"NROS_CPP_HAS_STD_STRING"},
        "`std::string` overloads that forward to a freestanding `const char*` "
        "form (phase-476 W1/W3), and `FixedString`/`HeapString` conversions",
    ),
    "shared-ptr-interop": (
        {"NROS_CPP_HAS_SHARED_PTR"},
        "`Node::SharedPtr` = `std::shared_ptr<Node>` (`std::make_shared<"
        "rclcpp::Node>` is how a ported `main` builds its node) and the "
        "`SharedPtr`-taking spin verbs over the `Node&` ones",
    ),
    "chrono-interop": (
        {"NROS_CPP_HAS_STD_CHRONO"},
        "`std::chrono::duration` overloads that convert to `nros::Duration` and "
        "delegate (`create_wall_timer`, `rclcpp::create_timer`, `Rate`)",
    ),
    "stream-interop": (
        {"NROS_CPP_HAS_STD_SSTREAM"},
        "`RCLCPP_*_STREAM` formatting of a type with its own "
        "`operator<<(std::ostream&)`; every builtin goes through the "
        "freestanding `LogStream` on all targets (phase-476 W4)",
    ),
    "container-interop": (
        {"NROS_CPP_STD"},
        "`std::vector` / `std::string` parameter values and the `<map>` behind "
        "`declare_parameters`, over the freestanding `nros::Seq<T, N>` and "
        "`const char*` forms",
    ),
    "bridge-hosted": (
        {"NROS_CPP_STD"},
        "`nros::MultiExecutor` (`bridge.hpp`) is constructed from a "
        "`std::vector<SessionSpec>` and has no freestanding form: a bridge "
        "between RMW sessions runs on a hosted target. Hosted-only BY DESIGN, "
        "not interop",
    ),
}

CONDITIONAL = re.compile(r"^\s*#\s*(if|ifdef|ifndef|elif)\b(.*)$")
MACRO = re.compile(r"\b(NROS_CPP_HAS_[A-Z_]+|NROS_CPP_STD)\b")
TAG = re.compile(r"//\s*hosted-family:\s*([a-z0-9-]+)\s*$")


def problems(files: dict) -> tuple:
    """(problems, {id: [file:line, ...]}) over `{path: text}`."""
    out = []
    used = {k: [] for k in FAMILY}
    for path, text in sorted(files.items()):
        for n, line in enumerate(text.splitlines(), 1):
            m = CONDITIONAL.match(line)
            if not m:
                continue
            code = m.group(2).split("//", 1)[0]
            macros = set(MACRO.findall(code))
            if not macros:
                continue
            where = f"{path}:{n}"
            tag = TAG.search(line)
            if tag is None:
                out.append(
                    f"  {where}: a capability conditional with no `// hosted-family: <id>` tag\n"
                    f"      {line.strip()}\n"
                    f"      Give the surface a freestanding form, or tag it with a FAMILY member "
                    f"in {Path(__file__).name} — adding a member is a decision, stated with its reason."
                )
                continue
            fid = tag.group(1)
            if fid not in FAMILY:
                out.append(f"  {where}: `hosted-family: {fid}` is not a family member")
                continue
            allowed = FAMILY[fid][0]
            extra = sorted(macros - allowed)
            if extra:
                out.append(
                    f"  {where}: `{fid}` may name {sorted(allowed)}, and this conditional "
                    f"also names {extra}"
                )
            used[fid].append(where)
    for fid, sites in used.items():
        if not sites:
            out.append(
                f"  family member `{fid}` tags no conditional. Delete it: an unused "
                f"member is a place to put the next hosted-only region unannounced."
            )
    return out, used


def tracked_headers() -> dict:
    names = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "-z", f"{INCLUDE}/*.hpp", f"{INCLUDE}/**/*.hpp"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split("\0")
    return {n: (ROOT / n).read_text(encoding="utf-8") for n in sorted(set(names)) if n}


def self_test() -> None:
    """The four refusals and the passing shape, on every run."""
    full = {
        f"f{i}.hpp": f"#if defined({sorted(allowed)[0]}) // hosted-family: {fid}\n#endif\n"
        for i, (fid, (allowed, _)) in enumerate(FAMILY.items())
    }
    ok, _ = problems(full)
    assert not ok, f"selftest: the compliant shape must pass, got {ok}"

    bad = dict(full)
    bad["new.hpp"] = "#ifdef NROS_CPP_HAS_STD_STRING\n#endif\n"
    assert any("no `// hosted-family" in p for p in problems(bad)[0]), "untagged must fail"

    bad = dict(full)
    bad["new.hpp"] = "#ifdef NROS_CPP_HAS_STD_STRING // hosted-family: made-up\n#endif\n"
    assert any("not a family member" in p for p in problems(bad)[0]), "unknown id must fail"

    bad = dict(full)
    bad["new.hpp"] = (
        "#if defined(NROS_CPP_HAS_STD_STRING) && defined(NROS_CPP_HAS_SHARED_PTR)"
        " // hosted-family: string-interop\n#endif\n"
    )
    assert any("also names" in p for p in problems(bad)[0]), "a foreign macro must fail"

    first = next(iter(full))
    bad = {k: v for k, v in full.items() if k != first}
    assert any("tags no conditional" in p for p in problems(bad)[0]), "an unused member must fail"

    # A macro named only in the trailing comment is not a capability conditional.
    prose = dict(full)
    prose["prose.hpp"] = "#ifndef SOME_GUARD // mentions NROS_CPP_STD in prose\n#endif\n"
    assert not problems(prose)[0], "a macro in a comment must not count"


def main() -> int:
    self_test()
    out, used = problems(tracked_headers())
    if out:
        print("check-cpp-hosted-family: FAIL\n", file=sys.stderr)
        print("\n".join(out), file=sys.stderr)
        return 1
    total = sum(len(v) for v in used.values())
    members = ", ".join(f"{k} {len(v)}" for k, v in used.items())
    print(
        f"check-cpp-hosted-family: OK — {total} capability conditional(s), every one in the "
        f"enumerated hosted-only family ({members}); self-test 6 cases OK"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
