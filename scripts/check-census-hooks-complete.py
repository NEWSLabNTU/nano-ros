#!/usr/bin/env python3
"""Every entity a C++ node creates crosses one ABI entry point, and every one
of those entry points calls its census hook -- phase-463 W1 (issue 1419).

THE CLASS
---------
The phase-308 recorder is the seam a host census is built on: a C++ component
cannot create a publisher, subscription, service, client, timer, guard
condition or parameter without crossing `nros_cpp_*_create` /
`nros_cpp_node_declare_param_*`, and the RMW-bound kinds reach a backend
selected by name (`NROS_RMW=metadata`). So the census is complete by
CONSTRUCTION -- as long as every entry point still calls its hook and every
hook still records. Nothing held either. Measured on the safety island
(2026-09-20): the backend took `_qos` and dropped it, so two sidecars said
`depth: 10` for a `QoS(1)` subscription; no hook observed
`declare_parameter`, so `parameters: []` stood for 21 declared parameters;
and a guard condition was a timer with period 0.

A hook that quietly stops being called is the same defect with a later date,
and it is invisible: the entry point still returns OK, the sidecar still
parses, and the number it carries is merely smaller than the truth -- the
under-count that puts `ExecutorFull` on a board with no console.

WHAT THIS REFUSES
-----------------
1. An executor-side entry point whose body does not call its hook:
   `nros_cpp_timer_create*` -> `on_timer_create`,
   `nros_cpp_guard_condition_create` -> `on_guard_condition_create`,
   `nros_cpp_node_declare_param_*` -> `on_param_declare`,
   `nros_cpp_node_create` / `_ex` -> `on_node_create` (the cursor every other
   record attributes to).
2. An RMW seam in `nros-rmw-metadata` that does not record, or records without
   the QoS it was handed (a `_qos` parameter is the 2026-09 defect verbatim).
3. A hook whose `metadata-mode` body no longer reaches the recorder.
4. The phase-308 layer rule: the hooks and the backend are ADAPTERS. No JSON
   spelled by hand, no schema struct, no slot arithmetic -- those live once in
   `nros::node_metadata`. Two definitions of "what is a slot" is how the count
   this mechanism exists to produce stops meaning anything.

Comments and string literals are stripped before any of this is read, so a
commented-out call is a missing call.

THE NEGATIVE CONTROL
--------------------
Runs on the normal path (`check-gate-selftests` requires it): the shipped
sources are mutated IN MEMORY -- one hook call deleted, one QoS dropped, one
hook body emptied, one JSON literal introduced -- and each mutation must go
red naming the site. A control that nobody runs decays into a comment; a
control that rebuilt `nros-cpp` once per hook would cost minutes per gate.
The fixture census itself (one of everything through the real ABI) is the
`census_fixture_tests` module in `metadata_hooks.rs`, run by the same recipe.

Run: python3 scripts/check-census-hooks-complete.py
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

HOOKS = "packages/api/nros-cpp/src/metadata_hooks.rs"
BACKEND = "packages/rmw/metadata/src/lib.rs"
TIMER = "packages/api/nros-cpp/src/timer.rs"
GUARD = "packages/api/nros-cpp/src/guard_condition.rs"
PARAMS = "packages/api/nros-cpp/src/params_shim.rs"
NODE = "packages/api/nros-cpp/src/lib.rs"

SUBJECTS = (HOOKS, BACKEND, TIMER, GUARD, PARAMS, NODE)

# Executor-side entry points and the hook each must call. A PREFIX matches
# every `pub unsafe extern "C" fn` whose name starts with it, so a fifth timer
# entry or an eighth declare variant is held to the rule the day it is added.
ENTRY_HOOKS: tuple[tuple[str, str, str], ...] = (
    (TIMER, "nros_cpp_timer_create", "metadata_hooks::on_timer_create("),
    (GUARD, "nros_cpp_guard_condition_create", "metadata_hooks::on_guard_condition_create("),
    (PARAMS, "nros_cpp_node_declare_param_", "metadata_hooks::on_param_declare("),
    (NODE, "nros_cpp_node_create", "metadata_hooks::on_node_create("),
)

# The RMW seams: each must record, and must pass the profile it received.
RMW_SEAMS = ("create_publisher", "create_subscription", "create_service", "create_client")

# Hook name -> the recorder call its `metadata-mode` body must reach. The two
# executor-entity hooks go through the module's own `record` helper, which is
# held to `metadata_mode::record(` itself.
HOOK_BODIES = {
    "on_node_create": "metadata_mode::begin_node(",
    "on_timer_create": "record(",
    "on_guard_condition_create": "record(",
    "on_param_declare": "metadata_mode::record_parameter(",
    "record": "metadata_mode::record(",
}

# phase-308 layer rule: tokens an ADAPTER must not contain (code only).
LAYER_FORBIDDEN = (
    "serde",
    "json!",
    "MetadataRecorder",
    "EntityMetadata",
    "push_entity",
    "push_node",
    "EntitySlot",
    "CallbackSlot",
    "NodeSlot",
    "to_source_metadata_json",
    "write_source_metadata_json",
)
# A string literal that spells JSON by hand.
LAYER_JSON_LITERAL = re.compile(r'\{\\"|\\":|":')


class Fail(Exception):
    pass


def load_subjects() -> dict[str, str]:
    files: dict[str, str] = {}
    for rel in SUBJECTS:
        p = ROOT / rel
        if not p.is_file():
            raise Fail(f"{rel} is missing; the census seam moved and this gate did not follow")
        files[rel] = p.read_text(encoding="utf-8")
    return files


# --- source stripping ------------------------------------------------------


def strip_comments_and_strings(src: str) -> str:
    """Rust source with comments removed and string literals emptied to `""`.

    Newlines are kept so a diagnostic can still name a line. Good enough for
    this file set: no raw strings with quotes inside, no `/*` nesting deeper
    than one level in what it reads.
    """
    out: list[str] = []
    i = 0
    n = len(src)
    while i < n:
        c = src[i]
        nxt = src[i + 1] if i + 1 < n else ""
        if c == "/" and nxt == "/":
            j = src.find("\n", i)
            i = n if j < 0 else j
            continue
        if c == "/" and nxt == "*":
            depth = 1
            i += 2
            while i < n and depth:
                if src.startswith("/*", i):
                    depth += 1
                    i += 2
                elif src.startswith("*/", i):
                    depth -= 1
                    i += 2
                else:
                    if src[i] == "\n":
                        out.append("\n")
                    i += 1
            continue
        if c == '"':
            out.append('""')
            i += 1
            while i < n and src[i] != '"':
                if src[i] == "\\":
                    i += 1
                if i < n and src[i] == "\n":
                    out.append("\n")
                i += 1
            i += 1
            continue
        if c == "'" and i + 2 < n and src[i + 2] == "'" and src[i + 1] != "\\":
            # A char literal like '"' or '{'; a lifetime has no closing quote.
            i += 3
            continue
        out.append(c)
        i += 1
    return "".join(out)


def string_literals(src: str) -> list[str]:
    """Every string literal in the CODE (comments excluded)."""
    lits: list[str] = []
    i = 0
    n = len(src)
    while i < n:
        c = src[i]
        nxt = src[i + 1] if i + 1 < n else ""
        if c == "/" and nxt == "/":
            j = src.find("\n", i)
            i = n if j < 0 else j
            continue
        if c == "/" and nxt == "*":
            j = src.find("*/", i + 2)
            i = n if j < 0 else j + 2
            continue
        if c == '"':
            j = i + 1
            while j < n and src[j] != '"':
                if src[j] == "\\":
                    j += 1
                j += 1
            lits.append(src[i + 1 : j])
            i = j + 1
            continue
        if c == "'" and i + 2 < n and src[i + 2] == "'" and src[i + 1] != "\\":
            i += 3
            continue
        i += 1
    return lits


def without_tests(src: str) -> str:
    """The non-test part of a file: everything before its first `cfg(test)`."""
    m = re.search(r"#\[cfg\(\s*(all\(\s*)?test\b", src)
    return src if m is None else src[: m.start()]


# --- function bodies -------------------------------------------------------

FN_HEAD = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)\s*[<(]")


def function_bodies(code: str) -> dict[str, str]:
    """`name -> body` for every `fn` in comment- and string-stripped source.

    Brace-matched from the first `{` after the signature. A name defined twice
    (a `cfg`-split pair) keeps the concatenation, so a hook that exists in both
    arms is read in both.
    """
    bodies: dict[str, str] = {}
    for m in FN_HEAD.finditer(code):
        name = m.group(1)
        start = code.find("{", m.end())
        semi = code.find(";", m.end())
        if start < 0 or (0 <= semi < start):
            continue  # a declaration (`fn f();`), not a definition
        depth = 0
        i = start
        while i < len(code):
            if code[i] == "{":
                depth += 1
            elif code[i] == "}":
                depth -= 1
                if depth == 0:
                    break
            i += 1
        bodies[name] = bodies.get(name, "") + code[start : i + 1]
    return bodies


def line_of(code: str, needle: str) -> int:
    idx = code.find(needle)
    return code.count("\n", 0, idx) + 1 if idx >= 0 else 0


# --- the rules -------------------------------------------------------------


def check(files: dict[str, str]) -> tuple[list[str], dict[str, int]]:
    errs: list[str] = []
    stats = {"entry_points": 0, "rmw_seams": 0, "hooks": 0}
    # The two adapters are read without their test modules (a test may spell
    # JSON to assert on it); the entry-point files are read whole, because
    # `nros-cpp/src/lib.rs` has test modules ABOVE its node entry points.
    stripped = {
        rel: strip_comments_and_strings(without_tests(src) if rel in (HOOKS, BACKEND) else src)
        for rel, src in files.items()
    }
    bodies = {rel: function_bodies(code) for rel, code in stripped.items()}

    # 1. Executor-side entry points call their hook.
    for rel, prefix, hook in ENTRY_HOOKS:
        names = [n for n in bodies[rel] if n.startswith(prefix)]
        if not names:
            errs.append(
                f"{rel}: no `fn {prefix}*` found -- the entry point moved and this gate "
                "did not follow, which is the vacuous-gate class"
            )
            continue
        for name in sorted(names):
            stats["entry_points"] += 1
            if hook not in bodies[rel][name]:
                errs.append(
                    f"{rel}:{line_of(stripped[rel], f'fn {name}')}: `{name}` creates an "
                    f"entity and never calls `{hook})` -- the census would not see it"
                )

    # 2. The RMW seams record, with the QoS they were handed.
    for seam in RMW_SEAMS:
        body = bodies[BACKEND].get(seam)
        if body is None:
            errs.append(f"{BACKEND}: `fn {seam}` is missing from the recording backend")
            continue
        stats["rmw_seams"] += 1
        if "record(" not in body:
            errs.append(f"{BACKEND}: `{seam}` never records the entity it is asked to create")
        head_start = stripped[BACKEND].find(f"fn {seam}")
        head = stripped[BACKEND][head_start : stripped[BACKEND].find("{", head_start)]
        if "_qos" in head or not re.search(r"\bqos\s*:", head):
            errs.append(
                f"{BACKEND}:{line_of(stripped[BACKEND], f'fn {seam}')}: `{seam}` drops the "
                "QoS it receives (the parameter is not `qos`) -- a default is not an observation"
            )
        elif not re.search(r"\bqos\b", body):
            errs.append(
                f"{BACKEND}:{line_of(stripped[BACKEND], f'fn {seam}')}: `{seam}` takes `qos` "
                "and never passes it on"
            )
    rec = bodies[BACKEND].get("record", "")
    if "qos: Some(qos)" not in rec.replace(" ", "").replace("qos:Some(qos)", "qos: Some(qos)"):
        errs.append(
            f"{BACKEND}: `record` does not hand the profile to the recorder (`qos: Some(qos)`)"
        )

    # 3. Every hook's `metadata-mode` body reaches the recorder.
    for hook, call in HOOK_BODIES.items():
        body = bodies[HOOKS].get(hook)
        if body is None:
            errs.append(f"{HOOKS}: hook `{hook}` is missing")
            continue
        stats["hooks"] += 1
        if call not in body:
            errs.append(
                f"{HOOKS}:{line_of(stripped[HOOKS], f'fn {hook}')}: `{hook}` no longer calls "
                f"`{call})` -- an entry point that calls it records nothing"
            )
        elif hook != "record" and "#[cfg(feature = \"metadata-mode\")]" not in files[HOOKS]:
            errs.append(f"{HOOKS}: the hooks are no longer gated on `metadata-mode`")

    # 4. The phase-308 layer rule on the two adapters.
    for rel in (HOOKS, BACKEND):
        code = stripped[rel]
        for token in LAYER_FORBIDDEN:
            if re.search(r"\b" + re.escape(token), code):
                errs.append(
                    f"{rel}:{line_of(code, token)}: `{token}` in an adapter -- the schema, the "
                    "recorder and the slot arithmetic live once in `nros::node_metadata`"
                )
        for lit in string_literals(without_tests(files[rel])):
            if LAYER_JSON_LITERAL.search(lit):
                errs.append(
                    f"{rel}: string literal {lit!r} spells JSON by hand -- serialisation is "
                    "`nros::metadata_mode::to_json`, nothing here formats anything"
                )
                break

    return errs, stats


# --- the negative control --------------------------------------------------


def self_test(files: dict[str, str]) -> None:
    """Break the shipped sources in memory and require a red each time."""

    def red(label: str, mutated: dict[str, str], needle: str) -> None:
        errs, _ = check(mutated)
        if not any(needle in e for e in errs):
            raise Fail(
                f"selftest mutation {label!r} produced no error mentioning {needle!r}.\n"
                f"  Got: {errs or '(clean)'}"
            )

    def drop_once(rel: str, mutated: dict[str, str], needle: str, replacement: str = "") -> None:
        if needle not in mutated[rel]:
            raise Fail(f"selftest: {needle!r} not in {rel}; the mutation would test nothing")
        mutated[rel] = mutated[rel].replace(needle, replacement, 1)

    # A pristine tree is green -- otherwise every mutation below is red for
    # the wrong reason.
    errs, _ = check(files)
    if errs:
        raise Fail("selftest: the tree is not green, so the mutations prove nothing:\n  " + "\n  ".join(errs))

    # 1. The guard-condition entry point stops calling its hook.
    m = dict(files)
    drop_once(GUARD, m, "crate::metadata_hooks::on_guard_condition_create();")
    red("guard hook call deleted", m, "nros_cpp_guard_condition_create")

    # 2. One of the seven declare variants stops calling the parameter hook.
    m = dict(files)
    body_start = m[PARAMS].find("fn nros_cpp_node_declare_param_double(")
    call = m[PARAMS].find("crate::metadata_hooks::on_param_declare(", body_start)
    call_end = m[PARAMS].find(";", call) + 1
    m[PARAMS] = m[PARAMS][:call] + m[PARAMS][call_end:]
    red("one declare variant unhooked", m, "nros_cpp_node_declare_param_double")

    # 3. The one-shot timer entry stops calling its hook -- commented out, to
    #    prove a comment is not a call.
    m = dict(files)
    drop_once(
        TIMER,
        m,
        "crate::metadata_hooks::on_timer_create(\n                nros::node_metadata::TimerKind::Oneshot,",
        "// crate::metadata_hooks::on_timer_create(\n                // nros::node_metadata::TimerKind::Oneshot,",
    )
    red("oneshot hook commented out", m, "nros_cpp_timer_create_oneshot")

    # 4. The backend drops the QoS again -- the 2026-09 defect verbatim.
    m = dict(files)
    drop_once(
        BACKEND,
        m,
        "        qos: nros_rmw::QoSProfile,\n    ) -> Result<Self::SubscriptionHandle, Self::Error> {\n        record(EntityKind::Subscription, topic.name, topic.type_name, qos)?;",
        "        _qos: nros_rmw::QoSProfile,\n    ) -> Result<Self::SubscriptionHandle, Self::Error> {\n        record(EntityKind::Subscription, topic.name, topic.type_name, Default::default())?;",
    )
    red("subscription QoS dropped", m, "drops the QoS")

    # 5. A hook body that no longer records.
    m = dict(files)
    drop_once(
        HOOKS,
        m,
        "if !nros::metadata_mode::record_parameter(_name, _value) {",
        "if false {",
    )
    red("parameter hook emptied", m, "on_param_declare")

    # 6. JSON spelled by hand in an adapter, and a schema struct named there.
    m = dict(files)
    drop_once(
        BACKEND,
        m,
        "fn close(&mut self) -> Result<(), Self::Error> {",
        'fn close(&mut self) -> Result<(), Self::Error> {\n        let _ = "{\\"version\\":1}";',
    )
    red("JSON literal in the backend", m, "spells JSON by hand")
    m = dict(files)
    drop_once(
        HOOKS,
        m,
        "fn record(\n",
        "fn record(\n    _r: Option<nros::node_metadata::MetadataRecorder>,\n",
    )
    red("recorder type named in the hooks", m, "MetadataRecorder")


def main() -> int:
    try:
        files = load_subjects()
        self_test(files)
        errs, stats = check(files)
    except Fail as exc:
        print(f"check-census-hooks-complete: {exc}", file=sys.stderr)
        return 1
    if errs:
        print(f"check-census-hooks-complete: {len(errs)} problem(s):\n", file=sys.stderr)
        for e in errs:
            print(f"  - {e}", file=sys.stderr)
        print(
            "\n  Every `nros_cpp_*_create` / `nros_cpp_node_declare_param_*` entry point calls\n"
            "  its `metadata_hooks::on_*` hook, every RMW seam in nros-rmw-metadata records\n"
            "  the QoS it is handed, and neither adapter spells JSON or a slot itself\n"
            "  (phase-463 W1, issue 1419).",
            file=sys.stderr,
        )
        return 1
    print(
        "check-census-hooks-complete: OK "
        f"({stats['entry_points']} entry points hooked, {stats['rmw_seams']} RMW seams record "
        f"QoS, {stats['hooks']} hooks reach the recorder, layer rule clean; 7 mutations red)"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
