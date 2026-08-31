#!/usr/bin/env bash
# phase-314 W5 — the feature-set SSoT gate.
#
# Every failure this phase fixed was SILENT: a consumer hook that applied on one
# path and not another (issue 0304), an edition hardcoded so a non-humble build
# compiled as humble and failed on the wire, capabilities that reached a pure
# C/C++ workspace but not a mixed one. None of them produced an error at the
# time; they produced a link failure or a wire mismatch a build away from the
# cause.
#
# So the gate is not "does it build" — it is "is there still exactly one source
# of truth". Drift here is invisible by construction, which is why it needs a
# check rather than a convention.
set -uo pipefail
cd "$(dirname "$0")/.."

fail=0
note() { printf '  %s\n' "$1"; }

# The edition pattern, named once so the selftest below exercises the SAME
# regex the gate runs. A selftest against a re-typed copy of a regex proves
# nothing about the regex that ships.
NROS_EDITION_RE='\b(humble|iron|jazzy)\b'

# The ONE shape allowed to spell an edition outside cmake/NanoRosRosEdition.cmake:
# a Kconfig derived-string mapping, `default "<ed>" if NROS_ROS_<ED>`. Kconfig
# cannot call a cmake function, so each integration needs its own two-line
# mapping from the menu choice to the string both of its lanes read. Anything
# else in integrations/ — a bare fallback, a hand-written cargo feature — is the
# defect this gate exists for.
NROS_EDITION_KCONFIG_MAP='default[[:space:]]+"(humble|iron|jazzy)"[[:space:]]+if[[:space:]]+NROS_ROS_'

# Exercise the failure path on EVERY run (phase-395). This gate in particular
# earned it: it spent its whole life reporting OK against a pattern that
# matched none of the six sites it existed to catch, and a negative control
# nobody runs would not have caught that either — only one that runs here.
nros_edition_selftest() {
    local caught
    # A planted literal must be caught...
    caught=$(printf '%s\n' 'cmake/Fake.cmake:12:    set(_e humble)' \
        | grep -E "$NROS_EDITION_RE" | grep -v ':[0-9]*:[[:space:]]*#' || true)
    if [ -z "$caught" ]; then
        echo "FAIL: check-feature-set-ssot selftest — the edition regex misses a bare literal" >&2
        exit 1
    fi
    # ...including the cargo-feature spelling, which the OLD pattern matched
    # and the new one must not regress.
    caught=$(printf '%s\n' 'cmake/Fake.cmake:3:  set(_f ros-humble)' \
        | grep -E "$NROS_EDITION_RE" || true)
    if [ -z "$caught" ]; then
        echo "FAIL: check-feature-set-ssot selftest — the regex no longer matches ros-humble" >&2
        exit 1
    fi
    # ...a Kconfig derived-string mapping must NOT be, or the one legitimate
    # spelling in each integration becomes an unfixable red (issue 0947).
    caught=$(printf '%s\n' 'integrations/nuttx/Kconfig:90:    default "jazzy"  if NROS_ROS_JAZZY' \
        | grep -E "$NROS_EDITION_RE" | grep -vE "$NROS_EDITION_KCONFIG_MAP" || true)
    if [ -n "$caught" ]; then
        echo "FAIL: check-feature-set-ssot selftest — a Kconfig mapping was treated as a literal" >&2
        exit 1
    fi
    # ...but a bare fallback in the same file must still be.
    caught=$(printf '%s\n' 'integrations/nuttx/Makefile:88:NROS_ROS_EDITION := humble' \
        | grep -E "$NROS_EDITION_RE" | grep -vE "$NROS_EDITION_KCONFIG_MAP" || true)
    if [ -z "$caught" ]; then
        echo "FAIL: check-feature-set-ssot selftest — a bare integrations/ fallback slipped through" >&2
        exit 1
    fi
    # ...and a comment must NOT be, or every explanatory note becomes a red.
    caught=$(printf '%s\n' 'cmake/Fake.cmake:9:    # we used to hardcode humble here' \
        | grep -E "$NROS_EDITION_RE" | grep -v ':[0-9]*:[[:space:]]*#' || true)
    if [ -n "$caught" ]; then
        echo "FAIL: check-feature-set-ssot selftest — a comment was treated as a literal" >&2
        exit 1
    fi
}
nros_edition_selftest

# 1. The ROS edition is chosen in exactly one place. A literal anywhere else is
#    the defect that made a non-humble build a wire mismatch rather than an
#    error (RFC-0056: the edition drives the keyexpr format that must match the
#    codegen-baked type_hash).
#
#    phase-405 W3 — THIS CHECK USED TO MATCH NOTHING. It grepped for
#    `ros-humble`, the CARGO FEATURE spelling, while every defaulting site in
#    cmake writes the bare word `humble`. Six sites existed; the gate matched
#    zero of them and printed OK, for as long as the check had existed. Worse,
#    the six did not behave alike — two consulted NANO_ROS_ROS_EDITION first and
#    four went straight to the literal, so `nros_find_interfaces` discarded a
#    workspace's declared edition outright (measured: a `-DNANO_ROS_ROS_EDITION=
#    jazzy` configure of examples/templates/cpp-port-minimal-publisher emitted
#    `"ros_edition": "humble"` for std_msgs and builtin_interfaces).
#
#    So the pattern is now the bare word, which also covers `ros-humble`
#    (the hyphen is a word boundary), and the allowlist is ONE FILE:
#    cmake/NanoRosRosEdition.cmake, which holds the default and the valid list
#    and nothing else. An allowlist of one is checkable in a way that "keep
#    these in sync" is not.
#
#    SCOPE: cmake/ + the root CMakeLists + the two api CMakeLists + integrations/.
#    `integrations/` was OUT until issue 0947 closed: NuttX carried `ros-humble`
#    literals in a Makefile lane whose edition vocabulary could not express
#    jazzy at all, so widening the glob first would have landed a red nobody
#    could turn green — which is how a gate gets switched off. Both integrations
#    now derive one `CONFIG_NROS_ROS_EDITION` string from a Kconfig choice and
#    both lanes read it, so the glob covers them.
# Comment lines are excluded: the conversion sites legitimately REFERENCE the
# old hardcode when explaining why they no longer do it.
edition_hits=$(git grep -nE "$NROS_EDITION_RE" -- \
    'cmake/*.cmake' 'cmake/*/*.cmake' 'CMakeLists.txt' \
    'packages/api/nros-c/CMakeLists.txt' \
    'packages/api/nros-cpp/CMakeLists.txt' \
    'integrations/' 2>/dev/null \
    | grep -v '^cmake/NanoRosRosEdition.cmake:' \
    | grep -vE "$NROS_EDITION_KCONFIG_MAP" \
    | grep -v ':[0-9]*:[[:space:]]*#' | cut -d: -f1 | sort -u || true)
if [ -n "$edition_hits" ]; then
    echo "FAIL: a ROS edition literal outside cmake/NanoRosRosEdition.cmake:"
    echo "$edition_hits" | while read -r f; do note "$f"; done
    note "Call _nros_resolve_ros_edition() instead; the only file allowed to"
    note "spell an edition is cmake/NanoRosRosEdition.cmake."
    fail=1
fi

# 2. Exactly one platform→cargo-feature mapping.
#
#    Matched on the ASSIGNMENT, not on a feature name: `platform-freertos` also
#    appears as a directory (`packages/platform/nros-platform-freertos`) in the
#    board/platform wiring files, and `NROS_PLATFORM_LINK_FEATURES` is a
#    transport axis (tcp/udp) with nothing to do with cargo. An earlier version
#    of this check matched the bare substring and reported six files that were
#    not duplication at all — which is how a gate loses its credibility.
plat_hits=$(git grep -n 'set(_platform_features\|set(_rmw_features' -- 'cmake/*.cmake' \
    'cmake/*/*.cmake' 'packages/api/nros-c/CMakeLists.txt' \
    'packages/api/nros-cpp/CMakeLists.txt' 2>/dev/null \
    | grep -v ':[0-9]*:[[:space:]]*#' | cut -d: -f1 | sort -u || true)
if [ -n "$plat_hits" ]; then
    echo "FAIL: a platform/rmw→cargo-feature assembly outside cmake/NanoRosFeatureSet.cmake:"
    echo "$plat_hits" | while read -r f; do note "$f"; done
    note "Call nros_feature_set() instead of re-deriving the list."
    fail=1
fi

# 3. No Rust NODE package names a ROS edition. Cargo features are additive and
#    ros-{humble,iron,jazzy} are compile_error!-exclusive, so a leaf naming one
#    is not an overridable default — it makes every other edition unbuildable in
#    that workspace. Entries own the choice.
leaf_hits=""
while read -r f; do
    [ -n "$f" ] || continue
    # An IMAGE owns the edition. That is an entry package, and also a
    # self-contained standalone example — which has no `[nros.entry]` table but
    # IS the binary (src/main.rs / [[bin]]). Only a lib-only node package,
    # which is linked into someone else\'s image, must stay silent.
    grep -q '\[package.metadata.nros.entry\]' "$f" && continue
    grep -q '^\[\[bin\]\]' "$f" && continue
    [ -f "$(dirname "$f")/src/main.rs" ] && continue
    leaf_hits="${leaf_hits}${f}"$'\n'
done < <(git grep -ln '"ros-\(humble\|iron\|jazzy\)"' -- 'examples/**/Cargo.toml' 2>/dev/null || true)
if [ -n "${leaf_hits//[$'\n']/}" ]; then
    echo "FAIL: node package(s) name a ROS edition — the edition is image-level:"
    echo "$leaf_hits" | while read -r f; do [ -n "$f" ] && note "$f"; done
    note "Drop the ros-* feature; the entry supplies it and unification carries it."
    fail=1
fi

# 4. phase-315 W3 — no manifest anywhere puts a ROS edition in its `default`
#    list. Cargo features are additive and the editions are compile_error!-
#    exclusive, so a default edition means `--features ros-jazzy` yields BOTH
#    and fails to compile: the user must discover `--no-default-features` and
#    then re-name every other default they wanted. The C++ selector
#    (-DNANO_ROS_ROS_EDITION) replaces; this keeps the cargo selector
#    equivalent. Invisible until someone tries a second edition, which is
#    exactly why it needs a gate rather than a convention.
default_hits=$(git grep -n '^default = \[.*ros-\(humble\|iron\|jazzy\)' \
    -- '*/Cargo.toml' 2>/dev/null | cut -d: -f1 | sort -u || true)
if [ -n "$default_hits" ]; then
    echo "FAIL: a ROS edition inside a \`default\` feature list:"
    echo "$default_hits" | while read -r f; do note "$f"; done
    note "Drop it. Naming no edition is legal and resolves to humble"
    note "(consumers gate on cfg(not(any(ros-iron, ros-jazzy))))."
    fail=1
fi

# 5. phase-315 W1/W2 — a WORKSPACE entry declares none of the three axes.
#    A workspace has a system.toml, which is the SSoT; `nros sync` generates a
#    selection facade crate carrying the derived features and the entry depends
#    on that. An entry that also names them by hand is a second copy free to
#    contradict the declaration — and the edition copy fails on the WIRE, not
#    at build time (RFC-0056), so nothing catches it.
#
#    Standalone examples are deliberately NOT covered: they have no system.toml,
#    so `--features` IS their selector (rule 4 keeps it replace-shaped).
entry_hits=$(python3 - <<'PY'
import re, subprocess, sys

# Brace-balanced, because a dep spec spans lines when its features array does.
# A line-wise grep cannot tell "the entry sets nros/ros-humble" from "a node
# package forwards its own safety-e2e" — those are different findings and only
# the first is W1/W2's to fix.
DECLARED = re.compile(
    r'"(ros-(?:humble|iron|jazzy)|rmw-(?:zenoh|xrce|cyclonedds|uorb)'
    r'|param-services|lifecycle-services|safety-e2e)"'
)
OWNED = lambda n: n == "nros" or n.startswith("nros-board-") or n.startswith("nros-rmw-")

files = subprocess.run(
    ["git", "ls-files", "examples/workspaces/*/Cargo.toml"],
    capture_output=True, text=True).stdout.split()
bad = []
for f in files:
    try:
        text = open(f).read()
    except OSError:
        continue
    if "[package.metadata.nros.entry]" not in text:
        continue
    hdr = re.search(r"^\[dependencies\]\s*$", text, re.M)
    if not hdr:
        continue
    body_start = hdr.end()
    nxt = re.search(r"^\[", text[body_start:], re.M)
    body_end = body_start + (nxt.start() if nxt else len(text) - body_start)
    for m in re.finditer(r"^([A-Za-z0-9_-]+)\s*=\s*", text[body_start:body_end], re.M):
        if not OWNED(m.group(1)):
            continue
        i = body_start + m.end()
        if text[i] != "{":
            continue
        depth, j = 0, i
        while j < body_end:
            if text[j] == "{":
                depth += 1
            elif text[j] == "}":
                depth -= 1
                if depth == 0:
                    break
            j += 1
        spec = text[i:j + 1]
        # Strip comments inside the spec before matching — these manifests
        # explain the axes in prose right where they used to set them.
        spec = re.sub(r"#[^\n]*", "", spec)
        hit = DECLARED.search(spec)
        if hit:
            bad.append(f"  {f} — {m.group(1)} names {hit.group(0)}")
            break
print("\n".join(bad))
PY
)
if [ -n "${entry_hits//[$'\n']/}" ]; then
    echo "FAIL: workspace entry package(s) restate a declared axis:"
    echo "$entry_hits" | while read -r f; do [ -n "$f" ] && note "$f"; done
    note "Depend on the generated <entry>_nros_selection facade instead;"
    note "\`nros sync\` derives edition/RMW/capabilities from system.toml."
    fail=1
fi

# 6. phase-323 W4 — no branch may APPEND `param_services` / `lifecycle` to the
#    capability list.
#
#    posix carried `if(PLATFORM STREQUAL "posix") list(APPEND _caps
#    param_services lifecycle)`, which until phase-323 W1 was the ONLY route
#    those axes took on hosted (issue 0353): `NANO_ROS_FEATURES` was never
#    populated on the workspace path. The effect was that hosted could not fail
#    when a declaration was missing, so "forgot to declare" and "declared" were
#    indistinguishable on the platform most people develop on.
#
#    `safety` is deliberately NOT matched here: `if(NANO_ROS_SAFETY_E2E)
#    list(APPEND _caps safety)` is the option round-trip, which is how a
#    STANDALONE project selects the axis with `-DNANO_ROS_SAFETY_E2E=ON`. That
#    is a selector, not a platform default.
cap_branch=$(git grep -nE 'list\(APPEND _caps [^)]*(param_services|lifecycle)' \
    -- 'cmake/*.cmake' 'packages/api/nros-c/CMakeLists.txt' \
    'packages/api/nros-cpp/CMakeLists.txt' 2>/dev/null \
    | grep -v ':[0-9]*:[[:space:]]*#' || true)
if [ -n "$cap_branch" ]; then
    echo "FAIL: a branch appends a capability instead of reading the declaration:"
    printf '%s\n' "$cap_branch" | while IFS= read -r h; do note "$h"; done
    note "Capabilities come from system.toml via NANO_ROS_FEATURES (phase-323 W1)."
    fail=1
fi

# 7. phase-323 W4 — no fixture row may force a capability's cmake knob.
#
#    Four safety rows carried `cmake_defs = { NANO_ROS_SAFETY_E2E = "ON" }`,
#    citing issue #118 for the missing wiring. 0118 was RESOLVED (phase-269) and
#    was about the integrity readback API, not cmake lowering — so a temporary
#    `-D` ended up defended by a closed ticket and outlived the bug it hid. A
#    fixture that forces the knob tests something the user's declaration does
#    not produce.
fixture_forced=$(git grep -nE 'cmake_defs.*NANO_ROS_(SAFETY_E2E|FEATURES)' \
    -- examples/fixtures.toml 2>/dev/null \
    | grep -v ':[0-9]*:[[:space:]]*#' || true)
if [ -n "$fixture_forced" ]; then
    echo "FAIL: a fixture row forces a capability knob instead of declaring it:"
    printf '%s\n' "$fixture_forced" | while IFS= read -r h; do note "$h"; done
    note "Declare it in the bringup's system.toml; the workspace derives the rest."
    fail=1
fi

# 8. issue 0358 — "is this package deploy-bound?" is asked in ONE place.
#
#    `[package.metadata.nros.entry]` and `[package.metadata.nros.deploy.<target>]`
#    both mean it. A consumer that reaches for either field alone is already
#    wrong: the source-metadata probe checked only `entry`, so 27 packages fell
#    through to a host build they cannot survive and surfaced as `DOTCONFIG must
#    be set by wrapper` on a Zephyr leaf (issue 0318) — several layers from the
#    predicate that forgot half its input.
#
#    `PackageMetadataNros::deploy_bound()` is that one place. This forbids
#    re-deriving it at a call site.
bound_hits=$(git grep -nE 'nros\.(entry\.is_some\(\)|deploy\.is_empty\(\))' \
    -- 'packages/cli/**/*.rs' 'packages/core/**/*.rs' 2>/dev/null \
    | grep -v 'fn deploy_bound' \
    | grep -v ':[0-9]*:[[:space:]]*//' || true)
if [ -n "$bound_hits" ]; then
    echo "FAIL: deploy-bound re-derived at a call site instead of via deploy_bound():"
    printf '%s\n' "$bound_hits" | while IFS= read -r h; do note "$h"; done
    note "Call PackageMetadataNros::deploy_bound() — it knows both spellings."
    fail=1
fi

if [ "$fail" -eq 0 ]; then
    echo "feature-set SSoT OK — one edition source, one platform mapping, no node-level"
    echo "editions, no default-list editions, no entry-level axis restatements."
fi
exit "$fail"
