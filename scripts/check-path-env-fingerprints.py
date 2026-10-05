#!/usr/bin/env python3
"""issue 0491 — a PATH-valued env var must never be fingerprinted as a STRING.

# Why this is worth a gate

`cargo:rerun-if-env-changed=NAME` makes cargo compare that variable's value as
TEXT. One directory has many spellings, and this repo routinely produces three
for the SAME first-party source dir:

    just/sdk-env.just     absolute, rooted at justfile_directory()
    a leaf .cargo/config  { value = "../../../../packages/…", relative = true },
                          which cargo resolves against THAT LEAF —
                          …/rust/talker/../../../../packages/… vs
                          …/rust/listener/../../../../packages/…
    a bare cargo build    unset

While every example leaf had its own `target/` those spellings never met. The
phase-340 shared cargo groups put them in ONE fingerprint namespace, and cargo
then reported

    dirty: EnvVarChanged { name: "NROS_PLATFORM_FREERTOS_SRC",
      old_value: Some(".../listener/../../../../packages/platform/…/src"),
      new_value: Some(".../talker/../../../../packages/platform/…/src") }

for every sibling — the board + zpico build scripts re-ran and
`UnitDependencyInfoChanged` cascaded to each leaf bin, so no two rows in a group
could both be fresh. Nothing fails; the group simply never converges.

Canonicalising in the build script cannot fix it: the string cargo compares is
the one the CONFIG produced, not the one the script resolved. So the rule is
about the DIRECTIVE, not the value — what a build script actually depends on is
the CONTENT of that directory, which `cargo:rerun-if-changed=<dir>` states, and
states identically from every leaf.

# Scope

TWO producers, because the rule has two spellings and checking only the first
one is how this bug survived its own fix for an afternoon: the FreeRTOS rows
went to 0 units while every ThreadX row still rebuilt 6, from
`config/threadx/nros-platform.toml`'s `rerun_if_env_changed` list, which
`runner.rs` replays through `println!("cargo:rerun-if-env-changed={var}")`.

  1. static `cargo:rerun-if-env-changed=NAME` literals in tracked Rust sources;
  2. `rerun_if_env_changed = [...]` entries in the platform manifests
     (`config/*/nros-platform.toml`), which the zpico build script emits.

Both are classified by the same predicate.

issue 1708 — producer 1 used to read only `cargo:rerun-if-env-changed=NAME`
LITERALS and skipped every interpolated name as "a knob table". That made it
blind to the `cargo::` spelling (even as a literal) and to any name built from
a CONSTANT — issue 1623's deliberate `format!("cargo::…={DESCRIPTOR_ENV}")`
included — so a second, undeliberate path watch spelled that way passed
silently. Now every directive STRING (outside comments and `#[cfg(test)]`
items) is read: a `{}` / `{N}` / `{name}` / `{CONST}` placeholder is resolved
through the macro's arguments to a `const NAME: &str = "…"` (this crate first,
then tree-wide if unambiguous) and classified like a literal. A name the scan
cannot read — a parameter, a loop variable, a call — FAILS CLOSED unless an
`UNRESOLVED_EXEMPTIONS` row keyed on (file, fn, expression) says where its
callers' names come from. Producer 2 is the data behind the biggest such loop.

Run: python3 scripts/check-path-env-fingerprints.py
"""

import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DIRECTIVE = re.compile(r"cargo:rerun-if-env-changed=([A-Za-z_][A-Za-z0-9_]*)")

# A name ending in one of these names a filesystem location, not a value.
PATH_SUFFIXES = (
    "_DIR",
    "_DIRS",
    "_SRC",
    "_PATH",
    "_INCLUDE",
    "_INCLUDES",
    "_ROOT",
    "_SYSROOT",
    "_TOML",
    "_FILE",
)

# Exempt names, each with the reason its spelling cannot vary WITHIN one cargo
# target dir. An exemption is a claim about the fingerprint namespace, not a
# preference — if two builds sharing one `--target-dir` can disagree about the
# string, it belongs in the fix, not here.
ALLOWED = {
    # `CARGO_TARGET_DIR` names the target dir itself, so two spellings are two
    # fingerprint namespaces by construction and cannot meet.
    "CARGO_TARGET_DIR": "names the target dir itself",
    #
    # `CORROSION_BUILD_DIR` USED TO BE EXEMPT HERE on the premise that every
    # cmake build dir owns its own cargo target dir. Issue 0805 made leaves
    # SHARE a target dir, so ~70 different spellings now land in one
    # `.fingerprint/` and the premise is false. Removing the exemption was not
    # bookkeeping: while it stood, every leaf invalidated the previous leaf's
    # build script and recompiled nros-c + nros-cpp — 459 s of cargo time on one
    # platform's warm rebuild, against 6.7 s once fixed.
    #
    # This is the failure mode this ALLOWED table is built to have: an exemption
    # is a claim about a fact OUTSIDE this file, and a change elsewhere can
    # falsify it silently. If you add one, say which invariant it rests on — as
    # these do — so the next person can check whether it still holds.
    # Emitted by cargo from the `links` crate's own OUT_DIR, which lives inside
    # the target dir being fingerprinted.
    "DEP_DDSC_INCLUDE": "cargo `links` metadata, rooted in this target dir",
    "DEP_DDSC_IDLC": "cargo `links` metadata, rooted in this target dir",
    # phase-400 W5 — REASON REPLACED, conclusion unchanged.
    #
    # This used to read "per-zephyr-build-dir; zephyr leaves share no cargo
    # group". The second clause is what W5 sets out to make false, and an
    # exemption whose premise a planned change removes is the CORROSION_BUILD_DIR
    # story two entries up, queued to repeat.
    #
    # The invariant it actually rests on is narrower and survives W5, and it was
    # MEASURED rather than reasoned (654 records across 41 C/C++ build trees,
    # 526 across 18 Rust ones):
    #
    #   * on the C/C++ lane — the one W5 collapses onto a shared cargo dir —
    #     `DOTCONFIG` is UNSET in every build-script environment. Since issue
    #     0460 that lane bakes all 26 knobs into its `cmake -E env` command, so
    #     `knob_usize` returns at the env check and never reaches the
    #     `$DOTCONFIG` fallback that emits this directive's companion read. A
    #     constant (unset) value cannot split a fingerprint namespace.
    #   * on the Rust lane it IS set, because zephyr-lang-rust builds its own
    #     cargo command and forwards no knobs — and that lane shares nothing:
    #     each Rust leaf is its own cargo workspace root (issue 0616).
    #
    # So this is a TRIPWIRE, not a blanket pass. Forwarding `DOTCONFIG` on the
    # C/C++ lane — a tempting way to close a knob gap — would make its value a
    # per-build-dir path inside one shared namespace, which is exactly what this
    # gate exists to prevent. Re-measure with `just shared-dir-churn` before
    # relying on this entry again.
    "DOTCONFIG": "unset on the C/C++ lane that shares; set only on the Rust lane, which does not",
    # A deliberate expert override naming a DIFFERENT SystemModel — a change of
    # value is a change of input, which is exactly what should re-run the script.
    "NROS_MODEL_DIR": "deprecated expert override; a new value IS a new input",
    # Name-shaped despite the suffix: the value is a NuttX-RELATIVE subpath
    # (`arch/arm/src/chip`, `arch/risc-v/src/board`), i.e. a knob, not a
    # filesystem location. One spelling everywhere by construction.
    "NUTTX_ARCH_INCLUDES": "NuttX-relative subpath list, not a location",
    "NUTTX_BOARD_LIB_DIR": "NuttX-relative subpath, not a location",
    # issue 1588 — REASON REPLACED, conclusion unchanged (the DOTCONFIG story
    # above, a second time). These read "cmake-set per build dir, which owns
    # its target dir", and issue 0805 made that false: the NuttX leaves now
    # SHARE one cargo target dir per (triple, profile, ffi crate, kernel,
    # knobs) key — `nros-nuttx.cmake`'s `nros_shared_cargo_dir`.
    #
    # The invariant the directive actually rests on survives the sharing,
    # because it is the opposite of 0491's premise. 0491 is about ONE input
    # with several SPELLINGS, where a respelling re-runs a script whose output
    # would not change. Here the value SELECTS the inputs: `nros-nuttx-ffi`'s
    # build script compiles each leaf's OWN sources into that leaf's image, so
    # two leaves in the shared dir never share its output (three leaves, three
    # distinct images, measured in 0805). Leaf B after leaf A MUST re-run it,
    # and nothing else can say so — the content watches name A's files, which
    # did not change. Dropping the directive would link A's app into B's image.
    # The content watches (`rerun-if-changed` on the RESOLVED paths, since
    # 1588) are what catch an edit within one leaf.
    "APP_INCLUDE_DIRS": "selects the leaf's inputs; leaves never share this script's output",
    "APP_INCLUDE_DIRS_FILE": "selects the leaf's inputs; leaves never share this script's output",
    "APP_FFI_LIBS_FILE": "selects the leaf's inputs; leaves never share this script's output",
}


import sys as _sys  # noqa: E402
from typing import NamedTuple  # noqa: E402

_sys.path.insert(0, os.path.join(ROOT, "scripts", "lib"))
import comments  # noqa: E402  phase-472 W3 — the one comment stripper
import per_item  # noqa: E402  phase-472 W6 — fn bodies, `#[cfg(test)]` items
from exemptions import Exemptions  # noqa: E402  phase-472 W8 — narrow, reasoned rows

# Names that are PATH-valued although no suffix says so. Each says what the
# value names, so the next reader can check the claim against the code.
PATH_NAMES = {
    # RFC-0100 D4 — the absolute path of `<build>/nros/sizing/<entry>.toml`,
    # written `relative = true` into a leaf's generated cargo config.
    "NROS_SIZING_DESCRIPTOR": "names the sizing-descriptor FILE (RFC-0100 D4)",
}

# issue 1708 — a path-shaped watch that is DELIBERATE, keyed on the (crate,
# variable) pair its reason is about, never on the name alone: `ALLOWED` above
# is name-keyed tree-wide, and a second crate watching the same name has to
# make its own case.
WATCH_EXEMPTIONS = Exemptions(
    {
        ("packages/tooling/nros-sizing-descriptor", "NROS_SIZING_DESCRIPTOR"):
            "issue 1623 — the unset<->set transition needs a cargo edge (a script "
            "that first ran with it unset kept its undeclared defaults), and the "
            "CONTENT is also watched: `load_for_build_script` emits "
            "`rerun-if-changed` on the file it names",
    },
    what="path-shaped watch",
)
WATCH_NEIGHBOURS = [
    # the same crate watching a DIFFERENT path variable…
    ("packages/tooling/nros-sizing-descriptor", "NROS_BOARD_TOML"),
    # …and a different crate watching the same one.
    ("packages/core/nros-node", "NROS_SIZING_DESCRIPTOR"),
]

# issue 1708 — a watch whose variable name this scanner cannot read: a function
# PARAMETER, a loop variable, a field, a call. Every one fails CLOSED unless a
# row here, keyed on (file, enclosing fn, argument expression), says why the
# name it carries is not a path. The reason is a claim about every CALLER, so
# it names where the names come from.
UNRESOLVED_EXEMPTIONS = Exemptions(
    {
        ("packages/core/nros-node/build.rs", "in_place_dispatch_trusted", "key"):
            "closure called with two `DEP_NROS_RMW_*_DISPATCH` literals — cargo "
            "`links` metadata flags (\"1\"), not locations",
        ("packages/core/nros-node/build.rs", "env_opt_string", "name"):
            "count-knob reader; callers pass `NROS_DECLARED_*` / `NROS_MAX_*` count names",
        ("packages/core/nros-node/build.rs", "env_usize_declared_or", "declared"):
            "count-knob reader; `declared` is an `NROS_DECLARED_*` count carrier",
        ("packages/core/nros-node/build.rs", "env_usize_declared", "declared"):
            "count-knob reader; `declared` is an `NROS_DECLARED_*` count carrier",
        ("packages/drivers/net/nros-smoltcp/build.rs", "env_usize_compat", "name"):
            "usize-knob reader; callers pass `NROS_SMOLTCP_*` sizes/timeouts",
        ("packages/drivers/net/nros-smoltcp/build.rs", "env_usize_compat", "fallback_name"):
            "usize-knob reader; callers pass the legacy `ZPICO_SMOLTCP_*` size names",
        ("packages/rmw/cyclonedds/nros-rmw-cyclonedds-sys/build.rs", "forward_derived_knobs", "knob"):
            "loop over the const `KNOBS` table beside it — five `NROS_CYCLONEDDS_MAX_*` "
            "/ `*_BYTES` integers",
        ("packages/rmw/zenoh/nros-zpico-build/src/runner.rs", "run",
         "platform_config::wire_env_key(key)"):
            "`ZPICO_<KNOB>` over `platform_config::WIRE_KNOBS` — buffer sizes and "
            "intervals",
        ("packages/rmw/zenoh/nros-zpico-build/src/runner.rs", "build_zenoh_pico_unified", "env_var"):
            "`if_env` of a platform manifest's `[[extra_sources]]` — a presence flag "
            "selecting sources, not a location",
        ("packages/rmw/zenoh/nros-zpico-build/src/runner.rs", "build_zenoh_pico_unified",
         "env_def.env"):
            "`defines_env` of a platform manifest — a C `#define`'s VALUE",
        ("packages/rmw/zenoh/nros-zpico-build/src/runner.rs", "build_zenoh_pico_unified", "var"):
            "a platform manifest's `rerun_if_env_changed` list — producer 2 of this "
            "gate, which classifies every entry of it",
        ("packages/tooling/nros-build-helpers/src/shared.rs", "env_usize", "name"):
            "usize-knob reader shared by the C/C++ build helpers; callers pass counts",
        ("packages/tooling/nros-platform-config/src/platform_config.rs", "executor_rung_opt",
         "key"):
            "`executor_env_key(knob)` — the `NROS_EXECUTOR_*` sizing knobs",
        ("packages/tooling/nros-zephyr-build/src/lib.rs", "stated", "self.env_name"):
            "`Knob::env_name` — a Kconfig-paired integer knob (`KCONFIG_PAIRS`)",
    },
    what="unresolvable watch",
)
UNRESOLVED_NEIGHBOURS = [
    # the same expression in a different fn of the same file…
    ("packages/tooling/nros-build-helpers/src/shared.rs", "env_string", "name"),
    # …and the same fn + expression in a different file.
    ("packages/core/nros-node/build.rs", "env_usize", "name"),
]

# `cargo:` and the newer `cargo::` spelling (issue 1623's own site uses the
# latter, and a literal in it was invisible too).
DIRECTIVE_PREFIX = re.compile(r"cargo::?rerun-if-env-changed=")
_NAME = re.compile(r"[A-Za-z_][A-Za-z0-9_]*\Z")
_CONST = re.compile(
    r"\bconst\s+([A-Z][A-Z0-9_]*)\s*:\s*&\s*(?:'static\s+)?str\s*=\s*\"([^\"\\]*)\"\s*;"
)
_CONST_PATH = re.compile(r"(?:(?:[A-Za-z_][A-Za-z0-9_]*)::)*([A-Z][A-Z0-9_]*)\Z")
_FN = re.compile(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)")


class Watch(NamedTuple):
    rel: str
    line: int
    fn: str
    expr: str  # what the directive's name came from, as written
    name: str | None  # the variable it resolves to; None = unresolvable


def const_defs(text: str) -> dict:
    """{NAME: {value, …}} for every `const NAME: &str = "…";` in `text`."""
    code = comments.strip_comments(text, "rust")
    out = {}
    for m in _CONST.finditer(code):
        out.setdefault(m.group(1), set()).add(m.group(2))
    return out


def _merge(into: dict, more: dict) -> dict:
    for k, vs in more.items():
        into.setdefault(k, set()).update(vs)
    return into


def _resolve(expr: str, local: dict, tree: dict):
    """The env var name `expr` evaluates to, or None.

    Only a `&str` CONSTANT resolves: bare (`NAME`, looked up in this crate) or
    path-qualified (`crate::NAME`, `other_crate::NAME`, looked up in this crate
    then tree-wide). An ambiguous name — two different values — does not."""
    expr = expr.strip().lstrip("&*").strip()
    m = _CONST_PATH.match(expr)
    if not m:
        return None
    key = m.group(1)
    for table in (local, tree):
        vals = table.get(key)
        if vals:
            return next(iter(vals)) if len(vals) == 1 else None
    return None


def _split_args(code: str, code_s: str, i: int) -> list:
    """Top-level comma-separated arguments from offset `i` (just past the
    format string's closing quote) to the macro's closing paren. Structure is
    read from `code_s` (string contents blanked), text from `code`."""
    args, depth, start = [], 0, i
    for k in range(i, len(code_s)):
        ch = code_s[k]
        if ch in "([{":
            depth += 1
        elif ch in ")]}":
            if depth == 0:
                args.append(code[start:k])
                break
            depth -= 1
        elif ch == "," and depth == 0:
            args.append(code[start:k])
            start = k + 1
    parts = [a.strip() for a in args]
    return [a for a in parts[1:] if a] if parts and not parts[0] else [a for a in parts if a]


def _placeholders(fmt: str) -> list:
    """[(offset, inner)] for each `{…}` in a format string, `{{` skipped."""
    out, k = [], 0
    while k < len(fmt):
        if fmt.startswith("{{", k) or fmt.startswith("}}", k):
            k += 2
            continue
        if fmt[k] == "{":
            e = fmt.find("}", k)
            if e < 0:
                break
            out.append((k, fmt[k + 1:e].split(":", 1)[0].strip()))
            k = e + 1
            continue
        k += 1
    return out


def scan_rust(rel: str, text: str, local: dict, tree: dict) -> list:
    """[Watch] for every `rerun-if-env-changed` directive string in `text`,
    outside comments and outside `#[cfg(test)]` items (a test emits nothing to
    cargo)."""
    code = comments.strip_comments(text, "rust")
    code_s = comments.strip_comments(text, "rust", strings=True)
    test_blank = per_item.rust_cfg_test_blank(code_s)
    fns = per_item.blocks(code_s, _FN)
    out = []
    for m in DIRECTIVE_PREFIX.finditer(code):
        if code_s[m.start()] != " ":
            continue  # not inside a string literal (prose survives stripping)
        q = m.start() - 1
        while q >= 0 and code[q] != '"':
            q -= 1
        if q < 0 or code_s[q] != '"' or test_blank[q] != '"':
            continue
        c = q + 1
        while c < len(code) and code[c] != '"':
            c += 2 if code[c] == "\\" else 1
        fmt = code[q + 1:c]
        rest_at = m.end() - (q + 1)
        e = rest_at
        while e < len(fmt) and fmt[e] not in '\\" \t\n':
            e += 1
        rest = fmt[rest_at:e]
        enclosing = [f for f in fns if f[1] < m.start() < f[2]]
        fn = max(enclosing, key=lambda f: f[1])[0].group(1) if enclosing else "<top>"
        line = per_item.line_of(text, m.start())
        if _NAME.match(rest):
            out.append(Watch(rel, line, fn, rest, rest))
            continue
        ph = re.fullmatch(r"\{([^{}:]*)(?::[^{}]*)?\}", rest)
        if not ph:
            # `=` then nothing, a concatenation, a partial name (`NROS_{x}_DIR`).
            out.append(Watch(rel, line, fn, rest or "<empty>", None))
            continue
        inner = ph.group(1).strip()
        args = _split_args(code, code_s, c + 1)
        named = {}
        positional = []
        for a in args:
            nm = re.match(r"([A-Za-z_][A-Za-z0-9_]*)\s*=(?!=)\s*(.*)\Z", a, re.S)
            if nm:
                named[nm.group(1)] = nm.group(2).strip()
            else:
                positional.append(a)
        if inner == "" or inner.isdigit():
            if inner == "":
                idx = sum(1 for off, inn in _placeholders(fmt) if off < rest_at and inn == "")
            else:
                idx = int(inner)
            expr = positional[idx] if idx < len(positional) else "<missing argument>"
        else:
            expr = named.get(inner, inner)  # named argument, else implicit capture
        expr = " ".join(expr.split())
        out.append(Watch(rel, line, fn, expr, _resolve(expr, local, tree)))
    return out


def is_path_shaped(name: str) -> bool:
    return name in PATH_NAMES or name.endswith(PATH_SUFFIXES)


def crate_of(rel: str, crate_dirs) -> str:
    """The nearest ancestor holding a tracked `Cargo.toml` ('' = repo root)."""
    d = os.path.dirname(rel)
    while d and d not in crate_dirs:
        d = os.path.dirname(d)
    return d


def classify(watches, crate, watch_ex=None, unresolved_ex=None):
    """(path_offenders, unresolved) — each a list of Watch."""
    watch_ex = watch_ex or WATCH_EXEMPTIONS
    unresolved_ex = unresolved_ex or UNRESOLVED_EXEMPTIONS
    bad, unresolved = [], []
    for w in watches:
        if w.name is None:
            if not unresolved_ex.covers((w.rel, w.fn, w.expr)):
                unresolved.append(w)
            continue
        if w.name in ALLOWED or not is_path_shaped(w.name):
            continue
        if watch_ex.covers((crate, w.name)):
            continue
        bad.append(w)
    return bad, unresolved


def tracked_crate_dirs() -> set:
    listed = subprocess.run(
        ["git", "ls-files", "Cargo.toml", "*/Cargo.toml"],
        capture_output=True, text=True, cwd=ROOT,
    ).stdout.split()
    return {os.path.dirname(p) for p in listed}


def scan_tree(sources):
    """(watches_by_crate {crate: [Watch]}, examined_count) over `sources`."""
    crate_dirs = tracked_crate_dirs()
    texts, local, tree = {}, {}, {}
    for rel in sources:
        try:
            text = open(os.path.join(ROOT, rel), encoding="utf-8").read()
        except (OSError, UnicodeDecodeError):
            continue
        texts[rel] = text
        if "const" in text:
            defs = const_defs(text)
            _merge(local.setdefault(crate_of(rel, crate_dirs), {}), defs)
            _merge(tree, defs)
    by_crate = {}
    for rel, text in texts.items():
        if "rerun-if-env-changed=" not in text:
            continue
        crate = crate_of(rel, crate_dirs)
        by_crate.setdefault(crate, []).extend(
            scan_rust(rel, text, local.get(crate, {}), tree)
        )
    return by_crate


def manifest_offenders(manifest_paths):
    """[(relpath, name)] for path-shaped names in a `rerun_if_env_changed` list.

    Parsed with a regex rather than a TOML library: this gate runs in
    `check-fast`, which must not depend on `tomli` being installed, and the key
    is written as one array in every manifest.
    """
    out = []
    for rel in manifest_paths:
        try:
            text = open(os.path.join(ROOT, rel), encoding="utf-8").read()
        except (OSError, UnicodeDecodeError):
            continue
        for m in re.finditer(r"rerun_if_env_changed\s*=\s*\[(.*?)\]", text, re.S):
            for name in re.findall(r'"([A-Za-z_][A-Za-z0-9_]*)"', m.group(1)):
                if name in ALLOWED:
                    continue
                if name.endswith(PATH_SUFFIXES):
                    out.append((rel, name))
    return out


def tracked_platform_manifests():
    """Producer 2's files — every `nros-platform.toml`, in BOTH in-tree roots.

    issue 1220 — this globbed `config/*/nros-platform.toml` alone, which was
    every platform when the rule was written and, by 2026-09-10, none of the
    three that carry a `rerun_if_env_changed` list. phase-400 W1 moved the
    ported platforms' descriptors to `packages/platform/nros-platform-<x>/`;
    `config/` kept `bare-metal` and `generic`, neither of which declares one. So
    the gate scanned 2 manifests, found 0 lists, and printed OK — including for
    `threadx`, the file this module's own Scope section names as the reason
    producer 2 exists. A gate's SCOPE is part of the rule it enforces.

    The vacuity guard is `manifests_declare_something` below: producer 2 having
    NO data is now a failure, not a quiet pass, so the same silence cannot come
    back through a third root.
    """
    listed = subprocess.run(
        [
            "git", "ls-files",
            "config/*/nros-platform.toml",
            "packages/platform/*/nros-platform.toml",
        ],
        capture_output=True,
        text=True,
        cwd=ROOT,
    ).stdout.split()
    return sorted(listed)


def manifests_declare_something(manifests):
    """Does producer 2 have any data at all in `manifests`?

    Not a style check: producer 2 is a list inside a TOML file, and a glob that
    stops reaching those files looks exactly like a tree where nobody writes
    them. The two are told apart here and nowhere else.
    """
    for rel in manifests:
        try:
            with open(os.path.join(ROOT, rel), encoding="utf-8") as fh:
                text = fh.read()
        except (OSError, UnicodeDecodeError):
            continue
        if re.search(r"rerun_if_env_changed\s*=\s*\[", text):
            return True
    return False


def tracked_rust_sources():
    listed = subprocess.run(
        ["git", "ls-files", "*.rs"], capture_output=True, text=True, cwd=ROOT
    ).stdout.split()
    # Vendored/generated trees are not ours to fix.
    return [
        p
        for p in listed
        if "/third-party/" not in f"/{p}"
        and "/generated/" not in f"/{p}"
        and not p.startswith("third-party/")
    ]


def self_test():
    """Both directions on synthetic input — a checker that stopped checking
    passes silently, which is the shape this gate exists for."""
    import tempfile

    def watches(body, consts=None, crate="packages/x/probe"):
        local = const_defs(body)
        _merge(local, consts or {})
        return crate, scan_rust(f"{crate}/build.rs", body, local, local)

    def flagged(body, consts=None, crate="packages/x/probe", watch_ex=None,
                unresolved_ex=None):
        crate, ws = watches(body, consts, crate)
        return classify(ws, crate, watch_ex or Exemptions({}),
                        unresolved_ex or Exemptions({}))

    def expect(cond, what):
        if not cond:
            sys.stderr.write(f"self-test: {what}\n")
            sys.exit(2)

    # A LITERAL path-shaped name IS reported, in both directive spellings.
    for pfx in ("cargo:", "cargo::"):
        bad, _u = flagged(f'fn main(){{println!("{pfx}rerun-if-env-changed=SOME_PLATFORM_SRC");}}\n')
        expect(bad, f"a literal path-shaped name was NOT reported ({pfx})")
    # A value-shaped name is NOT reported (knobs must stay fingerprinted).
    bad, unr = flagged('fn main(){println!("cargo:rerun-if-env-changed=ZPICO_TX_BATCH");}\n')
    expect(not bad and not unr, "a value-shaped literal was reported")
    # A name in ALLOWED is not reported, and the exemption is by NAME, not suffix.
    bad, _u = flagged('fn main(){println!("cargo:rerun-if-env-changed=CARGO_TARGET_DIR");}\n')
    expect(not bad, "an ALLOWED name was reported")
    bad, _u = flagged('fn main(){println!("cargo:rerun-if-env-changed=OTHER_TARGET_DIR");}\n')
    expect(bad, "the ALLOWED exemption leaked to a sibling name")

    # issue 1708 — a CONST captured inline (`{NAME}`) resolves and is classified.
    src = 'const BOARD: &str = "NROS_BOARD_TOML";\nfn main(){println!("cargo:rerun-if-env-changed={BOARD}");}\n'
    bad, unr = flagged(src)
    expect([w.name for w in bad] == ["NROS_BOARD_TOML"] and not unr,
           f"a const-captured path watch was not resolved+reported: {bad} {unr}")
    # …a `format!` with a POSITIONAL argument, `cargo::` spelling, path-qualified
    # const from ANOTHER crate (resolved tree-wide).
    src = ('fn f(emit:&mut dyn FnMut(&str)){emit(&format!(\n'
           '    "cargo::rerun-if-env-changed={}",\n    other::SDK_ROOT_ENV\n));}\n')
    bad, unr = flagged(src, consts={"SDK_ROOT_ENV": {"THREADX_SDK_ROOT"}})
    expect([w.name for w in bad] == ["THREADX_SDK_ROOT"] and not unr,
           f"a format!-built positional path watch was not resolved+reported: {bad} {unr}")
    # …a NAMED argument, after an unrelated earlier placeholder.
    src = ('const V: &str = "NROS_PATCH_FILE";\n'
           'fn main(){println!("x={} cargo:rerun-if-env-changed={n}", 1, n = V);}\n')
    bad, unr = flagged(src)
    expect([w.name for w in bad] == ["NROS_PATCH_FILE"] and not unr,
           f"a named-argument path watch was not resolved+reported: {bad} {unr}")
    # …and a resolved VALUE-shaped const is fine.
    src = 'const K: &str = "ZPICO_TX_BATCH";\nfn main(){println!("cargo:rerun-if-env-changed={}", K);}\n'
    bad, unr = flagged(src)
    expect(not bad and not unr, "a value-shaped const was reported")

    # An UNRESOLVABLE argument FAILS CLOSED — a parameter, a call, a partial name.
    for body in (
        'fn knob(name:&str){println!("cargo:rerun-if-env-changed={name}");}\n',
        'fn f(){println!("cargo:rerun-if-env-changed={}", key_for(3));}\n',
        'fn f(x:&str){println!("cargo:rerun-if-env-changed=NROS_{x}_DIR");}\n',
        'fn f(){println!("cargo:rerun-if-env-changed={}");}\n',
    ):
        _b, unr = flagged(body)
        expect(unr, f"an unresolvable watch was silently ignored: {body!r}")
    # …unless a row keyed on (file, fn, expr) covers it — and ONLY that.
    ex = Exemptions({("packages/x/probe/build.rs", "knob", "name"): "counts"})
    _b, unr = flagged('fn knob(name:&str){println!("cargo:rerun-if-env-changed={name}");}\n',
                      unresolved_ex=ex)
    expect(not unr, "an unresolved-exemption row did not cover its own site")
    _b, unr = flagged('fn other(name:&str){println!("cargo:rerun-if-env-changed={name}");}\n',
                      unresolved_ex=Exemptions({("packages/x/probe/build.rs", "knob", "name"): "c"}))
    expect(unr, "an unresolved-exemption row leaked to another fn")

    # A comment and a `#[cfg(test)]` item emit nothing to cargo.
    bad, unr = flagged('// println!("cargo:rerun-if-env-changed=SOME_DIR");\n'
                       '#[cfg(test)]\nmod t { fn f(x:&str){ let _ = format!("cargo:rerun-if-env-changed={x}"); } }\n'
                       'fn main(){}\n')
    expect(not bad and not unr, f"a comment / cfg(test) watch was counted: {bad} {unr}")

    # issue 1623's deliberate watch: exempt in ITS crate, for ITS variable only.
    # The row is restated here rather than read from the live table, so that
    # deleting the live row fails on the TREE scan, naming the real site.
    sd = "packages/tooling/nros-sizing-descriptor"
    row_1623 = {(sd, "NROS_SIZING_DESCRIPTOR"): "issue 1623"}
    src = ('pub const DESCRIPTOR_ENV: &str = "NROS_SIZING_DESCRIPTOR";\n'
           'fn f(emit:&mut dyn FnMut(&str)){emit(&format!("cargo::rerun-if-env-changed={DESCRIPTOR_ENV}"));}\n')
    bad, _u = flagged(src, crate=sd, watch_ex=Exemptions({}))
    expect(bad, "NROS_SIZING_DESCRIPTOR is not classified as a path")
    bad, _u = flagged(src, crate=sd, watch_ex=Exemptions(row_1623))
    expect(not bad, "the issue-1623 row does not cover its own site")
    bad, _u = flagged(src, crate="packages/core/nros-node",
                      watch_ex=Exemptions(row_1623))
    expect(bad, "the issue-1623 row leaked to a NEIGHBOURING crate")
    bad, _u = flagged(src.replace("NROS_SIZING_DESCRIPTOR", "NROS_BOARD_TOML"), crate=sd,
                      watch_ex=Exemptions(row_1623))
    expect(bad, "the issue-1623 row leaked to a NEIGHBOURING variable")
    for table, neighbours in ((WATCH_EXEMPTIONS, WATCH_NEIGHBOURS),
                              (UNRESOLVED_EXEMPTIONS, UNRESOLVED_NEIGHBOURS)):
        probs = Exemptions(table.table, what=table.what).check(neighbours)
        expect(not probs, f"exemption table: {probs}")

    # …and the manifest producer, both directions on the same classifier.
    with tempfile.TemporaryDirectory(dir=os.path.join(ROOT, "tmp")) as d:
        manifest = os.path.join(d, "nros-platform.toml")
        rel_manifest = os.path.relpath(manifest, ROOT)
        with open(manifest, "w") as fh:
            fh.write('rerun_if_env_changed = [\n  "THREADX_CONFIG_DIR",\n]\n')
        expect(manifest_offenders([rel_manifest]),
               "a path-shaped manifest entry was NOT reported")
        with open(manifest, "w") as fh:
            fh.write('rerun_if_env_changed = ["FREERTOS_PORT", "NROS_ZENOH_DEBUG"]\n')
        expect(not manifest_offenders([rel_manifest]),
               "a value-shaped manifest entry was reported")


def declared_env_names(sources, manifests):
    """Every env var name either producer declares as a build input.

    The gate above classifies these; this returns them ALL, path-shaped or not.
    Split out for phase-395 W10's shadow fixture cache, which has to witness the
    values of exactly this set: the enumeration of "which env vars are build
    inputs" already lives here, in the one place that knows about BOTH
    producers, and issue 0491 is the record of what consulting only one of them
    costs. A second enumerator would be that bug with a new spelling.

    issue 1708 — includes names a CONST-built directive resolves to, which the
    literal-only scan this replaced could not see. Unresolvable ones are not
    names and are omitted.
    """
    names = set()
    for ws in scan_tree(sources).values():
        names.update(w.name for w in ws if w.name)
    for rel in manifests:
        try:
            text = open(os.path.join(ROOT, rel), encoding="utf-8").read()
        except (OSError, UnicodeDecodeError):
            continue
        for m in re.finditer(r"rerun_if_env_changed\s*=\s*\[(.*?)\]", text, re.S):
            names.update(re.findall(r'"([A-Za-z_][A-Za-z0-9_]*)"', m.group(1)))
    return sorted(names)


def main():
    if "--list-env-names" in sys.argv[1:]:
        for name in declared_env_names(
            tracked_rust_sources(), tracked_platform_manifests()
        ):
            print(name)
        return
    self_test()
    sources = tracked_rust_sources()
    manifests = tracked_platform_manifests()
    if not manifests_declare_something(manifests):
        sys.stderr.write(
            "check-path-env-fingerprints: producer 2 is VACUOUS — none of the "
            f"{len(manifests)} platform manifest(s) reached declares a "
            "`rerun_if_env_changed` list.\n\n"
            "  This gate's whole reason for having a second producer is that "
            "list;\n"
            "  three descriptors carry one. If they moved again, widen\n"
            "  `tracked_platform_manifests`. If the key was genuinely retired "
            "tree-wide,\n"
            "  delete producer 2 rather than leaving it scanning nothing "
            "(issue 1220).\n"
        )
        sys.exit(1)
    by_crate = scan_tree(sources)
    examined = sum(len(ws) for ws in by_crate.values())
    bad_rust, unresolved = [], []
    for crate, ws in sorted(by_crate.items()):
        b, u = classify(ws, crate)
        bad_rust += b
        unresolved += u
    if examined == 0:
        sys.stderr.write(
            "check-path-env-fingerprints: producer 1 is VACUOUS — no "
            "`rerun-if-env-changed` directive found in any tracked Rust source.\n"
        )
        sys.exit(1)
    stale = WATCH_EXEMPTIONS.stale() + UNRESOLVED_EXEMPTIONS.stale()
    rc = 0
    if unresolved:
        rc = 1
        sys.stderr.write(
            "check-path-env-fingerprints: `rerun-if-env-changed` whose variable "
            "NAME cannot be read statically (issue 1708 — fails CLOSED).\n\n"
        )
        for w in unresolved:
            sys.stderr.write(f"  {w.rel}:{w.line} (fn {w.fn}): `{w.expr}`\n")
        sys.stderr.write(
            "\n  Only a `const NAME: &str = \"…\"` resolves. Either name the variable\n"
            "  through such a const, or add an UNRESOLVED_EXEMPTIONS row keyed on\n"
            "  (file, fn, expression) whose reason says where every caller's name\n"
            "  comes from and why none is a path.\n\n"
        )
    if stale:
        rc = 1
        sys.stderr.write("check-path-env-fingerprints: STALE exemption row(s) — "
                         "they match no site; delete them:\n")
        for k in stale:
            sys.stderr.write(f"  {k}\n")
        sys.stderr.write("\n")
    bad = [(f"{w.rel}:{w.line}", w.name if w.expr == w.name else f"{w.name} (via `{w.expr}`)")
           for w in bad_rust] + manifest_offenders(manifests)
    if bad:
        rc = 1
        sys.stderr.write(
            "check-path-env-fingerprints: `cargo:rerun-if-env-changed` on a "
            "PATH-valued variable.\n\n"
        )
        for rel, name in bad:
            sys.stderr.write(f"  {rel}: {name}\n")
        sys.stderr.write(
            "\n  Cargo compares that variable as TEXT, and one directory has\n"
            "  several spellings here (just exports it absolute; a leaf\n"
            "  .cargo/config.toml writes it `relative = true`, i.e. once per\n"
            "  leaf; a bare build leaves it unset). Rows sharing one\n"
            "  --target-dir then invalidate each other forever. (issue 0491)\n\n"
            "  Watch the CONTENT instead:\n"
            "      let dir = nros_build_paths::env_or_repo_path(NAME, rel);\n"
            "      nros_build_paths::watch_path(&dir);   // rerun-if-changed\n"
            "  If the spelling genuinely cannot vary within one target dir, add\n"
            "  the name to ALLOWED in this file WITH that reason; if the watch is\n"
            "  deliberate for ONE crate, a WATCH_EXEMPTIONS row keyed on\n"
            "  (crate, name) (issue 1708).\n"
        )
    if rc:
        sys.exit(rc)
    print(
        f"check-path-env-fingerprints: OK ({examined} rerun-if-env-changed "
        f"site(s) examined in {len(sources)} tracked Rust source(s), "
        f"{len(manifests)} platform manifest(s))"
    )


if __name__ == "__main__":
    main()
