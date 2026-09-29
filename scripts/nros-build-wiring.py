#!/usr/bin/env python3
"""Print the build wiring inventory: roads, carriers, producers, readers.

Why this is a SCRIPT and not a table in the reference doc: every count here
moves. `docs/reference/canonical-build-path.md` explains the roads and why
their carriers differ — prose that stays true — and defers every NUMBER to this
command. A hand-authored census is the failure mode this repository has
recorded more than any other (CLAUDE.md's "Gated" note on the package list, the
28-vs-88 RMW parity map, the sizes-header family): it reads as current and
answers about the tree someone measured once.

Everything below is derived from the tree at the moment you run it. Nothing is
an authored list except `ROADS`, which is checked against the `Driver` enum by
`check-build-wiring-roads`.

    python3 scripts/nros-build-wiring.py            # the whole inventory
    python3 scripts/nros-build-wiring.py --roads    # just the road table
    python3 scripts/nros-build-wiring.py --scripts  # build.rs by ROLE
    python3 scripts/nros-build-wiring.py --json

phase-471 replaced the build-script CAPABILITY letters with ROLES, and fixed
the population they were counted over. Both were wrong in the same direction —
they described the tree somebody grepped rather than the tree:

  * the POPULATION was "a file named `build.rs`", which let
    `packages/cli/nros-cli-core/src/cmd/build.rs` — the `nros build` COMMAND,
    4383 lines — in as a build script, with three capability letters read off
    prose in its own doc comment. It is now "a `build.rs` beside a
    `Cargo.toml`".
  * a LETTER was a grep for a TOOL. `C` (`cc::Build`) put three first-party
    test C files in the same class as the FreeRTOS kernel, and left
    `cyclonedds-sys` — ~200k lines of vendored C through `cmake::Config` — as
    "re-roots paths and nothing else".

What replaced them: one ROLE per script (ordered rules, first match wins, and
an UNCLASSIFIED list rather than an "other" bucket), plus two fields that are
the questions the hazards actually turn on — `source_origin` (where a
compiler's sources come from) and the PATH RESOLUTION section (issue 1280's
build-script half, which no gate covers).
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path
import sys as _w3_sys  # noqa: E402
from pathlib import Path as _W3Path  # noqa: E402
_w3_sys.path.insert(0, str(_W3Path(__file__).resolve().parent / "lib"))
import comments  # noqa: E402  phase-472 W3 — the one comment stripper

ROOT = Path(__file__).resolve().parent.parent

# The one authored structure, and the reason it is authored: a road's CARRIER
# is a design fact, not something a grep can read off the tree. The road NAMES
# are checked against `Driver` (plan.rs) by `check-build-wiring-roads`, so this
# cannot silently miss a road someone adds.
ROADS = {
    "Cargo": {
        "exec": "cargo build",
        "carrier": "`[env]` rows in the generated `build/<coord>/nros-cargo.toml`, read via `--config`",
        "emits_root": True,
        "hazard": "issue 0491 — a PATH-valued row has three spellings, so watch the CONTENT, never `rerun-if-env-changed`",
    },
    "CMake": {
        "exec": "cmake --build",
        "carrier": "`corrosion_set_env_vars()` on the target's own cargo command",
        "emits_root": True,
        "hazard": "issue 0460 — `set(ENV{})` touches only the configure process, so it carries NOTHING to a cargo lane",
    },
    "West": {
        "exec": "west build -b <board>",
        "carrier": "`$DOTCONFIG`, read by each build script through `nros_zephyr_build`",
        "emits_root": False,
        "hazard": "issue 0460 — zephyr-lang-rust builds its own cargo command and inherits no environment at all",
    },
}

# ---------------------------------------------------------------------------
# ROLE — what a build script is FOR.
#
# This replaced a set of capability LETTERS (phase-471). Those letters were
# greps for a TOOL, and a tool is not a role: `cc::Build` put
# `nros-platform-cffi` (three first-party C files for its own tests) in the
# same bucket as `nros-board-freertos` (the FreeRTOS kernel and lwIP out of an
# env-resolved SDK root), while `cyclonedds-sys` — which builds ~200k lines of
# vendored C through `cmake::Config` — read as "re-roots paths and nothing
# else". The two have nothing in common to converge and every reason to be
# told apart.
#
# ORDERED: first match wins, so a rule may assume the earlier ones failed. The
# order is from most specific to least, and the reason each rule sits where it
# does is in its `why`. A script matching NO rule is reported by PATH under
# UNCLASSIFIED rather than swept into an "other" bucket — an unnamed row is a
# finding for whoever added it, not a number to carry.
ROLES = [
    (
        "generated",
        "emitted by codegen; never hand-edited",
        # Only the header: `nros-node` says GENERATED about a file it WRITES.
        lambda t, ctx: re.search(r"GENERATED by", "\n".join(t.splitlines()[:6])),
        "first, because a generated file is not evidence about anybody's style",
    ),
    (
        "delegating-shim",
        "every effect is a call into a shared build crate",
        lambda t, ctx: (
            re.search(r"nros_board_common::\w+_build::|nros_board_common::\w+_link::"
                      r"|nros_zpico_build::|zephyr_build::export_kconfig", t)
            and not re.search(r"cc::Build|cmake::Config", t)
        ),
        "the CONVERGED shape: the script names the board, the crate holds the recipe",
    ),
    (
        "linker-script",
        "places a linker script in OUT_DIR and compiles nothing",
        lambda t, ctx: (
            re.search(r"rustc-link-search", t)
            and re.search(r"\.x\"|\.x\b", t)
            and not re.search(r"cc::Build|cmake::Config", t)
        ),
        "a self-contained job with no knob, no path and no SDK — correct as it is",
    ),
    (
        "abi-bindings",
        "generates Rust from a C header (RFC-0054)",
        lambda t, ctx: re.search(r"bindgen", t),
        None,
    ),
    (
        "c-compiler",
        "drives a C/C++ compile — see SOURCE ORIGIN for where the sources come from",
        lambda t, ctx: re.search(r"cc::Build|cmake::Config", t),
        "ONE role, deliberately. Which tree it compiles is a second, derived "
        "field (`source_origin`) rather than a second role, because 'where do "
        "the sources come from' is the question the hazards turn on and a role "
        "name would hide it",
    ),
    (
        "codegen-driver",
        "runs an in-repo generator at build time",
        lambda t, ctx: re.search(
            r"generate_run_plan|rosidl_codegen|generate-px4|nros_codegen"
            r"|pack\.toml|nros-rmw\.toml", t),
        None,
    ),
    (
        "provenance-stamp",
        "embeds a source stamp so a stale binary can say so",
        lambda t, ctx: re.search(r"source_stamp|SOURCE_STAMP|play_launch", t),
        "before knob-resolver: both emit `rustc-env`, and the stamp is the "
        "narrower claim",
    ),
    (
        "knob-resolver",
        "resolves RFC-0049 knobs into a generated config module",
        lambda t, ctx: re.search(r"_config\.rs|nros_zephyr_build::|rustc-env=", t),
        None,
    ),
    (
        "marker",
        "only rerun directives — an input cargo cannot see by itself",
        lambda t, ctx: re.search(r"rerun-if-", t),
        "last: every script above also emits these, so this is what is LEFT",
    ),
]

# Where a `c-compiler`'s sources come from. Three answers exist in the tree and
# only one of them is exposed to issue 1280 — which is the entire reason this
# is reported beside the role instead of folded into it.
SOURCE_ORIGINS = [
    # `DEP_<NAME>` is the `links=` hand-off from the crate that already
    # resolved the tree — it reads like a path variable and is not one, so the
    # negative lookahead is load-bearing rather than tidy.
    # `OUT_DIR` / `CARGO_*_DIR` are cargo's own and name nothing inherited.
    ("sdk-path-variable", r"env::var(?:_os)?\(\s*\"(?!DEP_|CARGO_|OUT_DIR)"
                          r"[A-Z_0-9]*(?:_DIR|_SRC|_SOURCE_DIR|_INCLUDE|_ROOT)\""
                          r"|nros_build_paths::(?!repo_root|try_repo_root|canonical"
                          r"|watch_path|checkout_root_of|reroot_foreign)\w+\("),
    ("links-channel", r"env::var(?:_os)?\(\s*\"DEP_"),
    ("workspace-relative", r"."),
]

# ---------------------------------------------------------------------------
# HAZARD SURFACE — orthogonal to the role, and the axis the gates care about.
#
# Two measured defect classes live here, so the census answers them directly
# rather than leaving it to a reader to cross-reference roles against issues:
#
#   issue 1280 — an inherited absolute path outranks the checkout being built.
#                `nros_build_paths` is the ONE resolver that applies the
#                three-valued rule; a raw `env::var` of the same name does not.
#   issue 0491 — a `rerun-if-env-changed` on a PATH variable fingerprints a
#                spelling, so it rebuilds forever. Watch the CONTENT.
#
# The variable NAMES are read off `just/sdk-env.just`, never authored here —
# and the test is not "is it exported" but "does its export carry the re-root
# wrapper". That is the same discriminator `check-inherited-checkout-paths`
# enforces on the shell side, read from the same line, so the two halves of
# issue 1280 cannot come to have different subjects. (An authored
# "these are not paths" table would be a SECOND spelling of the gate's
# `NOT_A_PATH`, which is how the tree gets two answers to one question.)
SDK_ENV_EXPORT = re.compile(
    r"^export\s+([A-Za-z_][A-Za-z_0-9]*)\s*:=\s*(.*)$", re.M)
REROOT_WRAPPER = "_NROS_REROOT"
# `just`'s name for "the checkout this justfile is in".
HERE_VAR = "_NROS_HERE"

# A read whose VALUE is used. `env::var("X").is_err()` asks whether the variable
# is set and never touches the path, so re-rooting it would change nothing —
# counting it would report a crate that has no defect and hide the ones that do.
RAW_ENV_READ = re.compile(
    r'env::var(?:_os)?\(\s*"([A-Z_0-9]+)"\s*\)(?!\s*\.is_(?:err|ok)\(\))')

# phase-471 W6 — the SECOND producer of a path-valued build-script variable.
# `just/sdk-env.just` is what a `just` road exports; a BOARD DESCRIPTOR's
# `cargo_config` `[env]` block is what the cargo and cmake roads export, and
# `THREADX_EXTRA_INCLUDES` lives only there. The test is `${workspace}`-rooted,
# which is precisely the class issue 1280 is about — a path INSIDE a checkout,
# so a value carried in from another one is re-rootable and exact. A value
# outside every checkout is KEPT by the rule anyway, so not deriving those costs
# nothing. Derived, not authored, for `sdk_path_vars`'s reason.
BOARD_ENV_PATH_ROW = re.compile(
    r'^\s*([A-Z_][A-Z_0-9]*)\s*=\s*\{\s*value\s*=\s*"\$\{workspace\}', re.M)

# A `path =` dependency inside a manifest section. Used to walk from a build
# script to the in-repo crates that RUN as part of it.
MANIFEST_SECTION = re.compile(r"^\[([^\]]+)\]\s*$", re.M)
PATH_DEP = re.compile(r'^\s*([\w-]+)\s*=\s*\{[^}]*?\bpath\s*=\s*"([^"]+)"', re.M | re.S)


def tracked(*patterns: str) -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", *patterns],
        cwd=ROOT, capture_output=True, text=True, check=True,
    ).stdout.split()
    return sorted(out)


def read(rel: str) -> str:
    try:
        return (ROOT / rel).read_text(errors="replace")
    except OSError:
        return ""


def sdk_path_vars() -> list[str]:
    """Path-valued variables, READ from `just/sdk-env.just`.

    An export is path-valued exactly when its definition carries the re-root
    wrapper — the same test `check-inherited-checkout-paths` applies, off the
    same line. Deriving it makes this census and that gate subjects of one
    fact; an authored copy is the shape issue 1280's own census had, and it was
    19 names and already short by five when it was written.
    """
    return sorted(
        name for name, rhs in SDK_ENV_EXPORT.findall(read("just/sdk-env.just"))
        # The wrapper, or the checkout root itself: `just` states the latter
        # outright rather than re-rooting it, and it is still a path a bare
        # `cargo` would take from the inherited environment.
        if REROOT_WRAPPER in rhs or rhs.strip() == HERE_VAR
    )


def board_env_path_vars() -> list[str]:
    """Path-valued variables a BOARD DESCRIPTOR exports, READ from the descriptors.

    A second SUBJECT, deliberately not folded into `sdk_path_vars`: that one is
    paired with `check-inherited-checkout-paths` off one line of
    `just/sdk-env.just`, and widening it would break the pairing rather than
    extend it. This one has its own SSoT — the `cargo_config` `[env]` rows in
    `nros-board.toml` — and its own reason to exist: `THREADX_EXTRA_INCLUDES`
    is exported by a descriptor and by a cmake board file, never by `just`, so
    no amount of reading `sdk-env.just` will ever find it (phase-471 W6).
    """
    names: set[str] = set()
    for p in tracked("*nros-board.toml"):
        names.update(BOARD_ENV_PATH_ROW.findall(read(p)))
    return sorted(names)


def path_vars() -> list[str]:
    """Every path-valued build-script variable, from BOTH producers."""
    return sorted(set(sdk_path_vars()) | set(board_env_path_vars()))


def strip_line_comments(text: str) -> str:
    """Path questions are asked of the CODE.

    The best-written script in the tree
    (`examples/mps2-an385-baremetal/c/talker`) explains in its doc comment why
    it does NOT use `$NROS_REPO_DIR`, and a census that reads comments counted
    that as a use.
    """
    # phase-472 W3 — the shared stripper (scripts/lib/comments.py).
    return comments.strip_comments(text, "rust")


def raw_reads_of(code: str, vars_: list[str] | set[str]) -> list[str]:
    """The path variables this code resolves with a bare `env::var`."""
    return sorted({m.group(1) for m in RAW_ENV_READ.finditer(code)
                   if m.group(1) in vars_})


def names_among(code: str, vars_: list[str] | set[str]) -> list[str]:
    """The path variables this code NAMES at all.

    `DEP_<NAME>` is the `links=` channel, not a read of `<NAME>`.
    """
    return sorted(v for v in vars_
                  if re.search(rf"(?<!DEP_)\b{re.escape(v)}\b", code))


def build_scripts() -> list[dict]:
    """One row per CARGO BUILD SCRIPT.

    The population is "a `build.rs` beside a `Cargo.toml`", not "a file named
    `build.rs`". Until phase-471 it was the latter, and the difference was not
    academic: `packages/cli/nros-cli-core/src/cmd/build.rs` — the `nros build`
    COMMAND, 4383 lines, a third of everything the census weighed — was counted
    as a build script and given three capability letters off prose in its own
    doc comment. A census whose population is wrong answers every question
    wrong, quietly.
    """
    vars_ = path_vars()
    rows = []
    for p in tracked("*/build.rs"):
        if not (ROOT / p).parent.joinpath("Cargo.toml").is_file():
            continue
        text = read(p)
        code = strip_line_comments(text)
        parts = p.split("/")
        category = parts[1] if parts[0] == "packages" else parts[0]

        manifest = read(str(Path(p).parent / "Cargo.toml"))
        ctx = {
            "board_deps": re.findall(
                r"^(nros-board-[\w-]+)\s*=\s*\{[^}]*path\s*=", manifest, re.M),
            "sdk_path_vars": names_among(code, vars_),
        }

        role = next((name for name, _, rule, _ in ROLES if rule(text, ctx)), None)
        # A PRIVATE `env_path`-style helper takes the variable NAME as an
        # ARGUMENT, so no literal-matching probe can see which variables it
        # resolves — which is exactly how two board crates dropped the
        # re-rooting rule without any grep noticing (issue 1527). One that
        # DELEGATES to the shared resolver is the fix, not the hazard, so what
        # is reported is a helper that resolves the value itself.
        private_helper = any(
            "nros_build_paths::env_path(" not in body
            for body in re.findall(r"^fn env_path\w*\([^\n]*\n(?:.*\n)*?^\}", code, re.M)
        )
        origin = None
        if role == "c-compiler":
            origin = ("sdk-path-variable" if private_helper
                      else next(n for n, rx in SOURCE_ORIGINS if re.search(rx, code)))

        # Hazard surface. `nros_build_paths` is the only resolver that applies
        # issue 1280's three-valued rule, so a variable a build script resolves
        # WITHOUT it takes whatever another checkout's shell exported.
        rows.append({
            "path": p,
            "category": category,
            "role": role,
            "source_origin": origin,
            "crate": Path(p).parent.name,
            "board_deps": ctx["board_deps"],
            "sdk_path_vars": ctx["sdk_path_vars"],
            "routes_paths": bool(re.search(r"nros_build_paths::", code)),
            "raw_path_reads": raw_reads_of(code, vars_),
            "private_path_helper": private_helper,
        })

    # Second pass — base vs overlay. A board crate is an OVERLAY when it
    # path-depends on a board crate that COMPILES the RTOS for it; the one it
    # depends on is the BASE. Derived from the set just computed rather than
    # from "depends on anything called nros-board-*", which counted
    # `nros-board-common` (the shared build crate, not a board) and made every
    # base read as an overlay.
    bases = {
        r["crate"] for r in rows
        if r["path"].startswith("packages/boards/") and r["role"] == "c-compiler"
    }
    for r in rows:
        if not r["path"].startswith("packages/boards/"):
            r["board_role"] = None
        elif set(r["board_deps"]) & bases:
            r["board_role"] = "overlay"
        else:
            r["board_role"] = "base"
    return sorted(rows, key=lambda r: r["path"])


BUILD_DEP_SECTIONS = ("build-dependencies",)
LIB_DEP_SECTIONS = ("dependencies", "build-dependencies")


def _manifest_path_deps(manifest_rel: str, sections: tuple[str, ...]) -> list[str]:
    """In-repo `path =` deps of a manifest, as repo-relative crate dirs.

    `sections` is what makes the walk mean something. From the crate cargo
    compiles a `build.rs` for, only `[build-dependencies]` runs at build time —
    its `[dependencies]` are the RUNTIME crate and its `[dev-dependencies]` are
    tests, and walking either reaches most of the tree. From a build-script
    LIBRARY, ordinary `[dependencies]` DO run at build time, because the library
    itself is already running there.
    """
    text = read(manifest_rel)
    base = Path(manifest_rel).parent
    out: list[str] = []
    # Split on section headers so a `[patch.*]` or `[package]` row cannot be
    # mistaken for a dependency.
    marks = [(m.start(), m.end(), m.group(1)) for m in MANIFEST_SECTION.finditer(text)]
    for i, (_s, e, name) in enumerate(marks):
        if name.rsplit(".", 1)[-1] not in sections:
            continue
        stop = marks[i + 1][0] if i + 1 < len(marks) else len(text)
        for _dep, rel in PATH_DEP.findall(text[e:stop]):
            resolved = (ROOT / base / rel).resolve()
            try:
                out.append(str(resolved.relative_to(ROOT)))
            except ValueError:
                continue  # outside the checkout — not ours to scan
    return out


def build_script_libs() -> list[dict]:
    """One row per in-repo crate that RUNS as part of a build script.

    phase-471 W6. `build_scripts()`'s population is deliberately narrow — W0
    fixed it by narrowing — but the RULE it feeds ("a build script resolves a
    path-valued SDK variable through `nros_build_paths`") is about code that
    runs at build time, and `nros-board-common` is exactly that: a build-script
    LIBRARY, reached from four board `build.rs` files through
    `[build-dependencies]`. A raw read there is strictly worse than one in a
    single board's own script, because it reaches every board that calls it —
    and that is where issue 1527's remaining three `NUTTX_DIR` reads were,
    invisible to a gate whose population was `build.rs`.

    Derived by walking `[build-dependencies]` path deps out of the census's own
    build scripts, then transitively through in-repo path deps. Nothing is
    authored.

    The ONE crate excluded is the one that DEFINES the rule (`pub fn
    reroot_foreign`): its own `env_path` reads `env::var` of a `&str` parameter
    and cannot be spelled `nros_build_paths::`, so including it would report the
    rule as a violation of itself. Identified by what it defines, not by name.
    """
    vars_ = path_vars()
    seen: set[str] = set()
    frontier: list[str] = []
    for r in build_scripts():
        frontier.extend(_manifest_path_deps(
            str(Path(r["path"]).parent / "Cargo.toml"), BUILD_DEP_SECTIONS))

    while frontier:
        crate_dir = frontier.pop()
        if crate_dir in seen:
            continue
        manifest = f"{crate_dir}/Cargo.toml"
        if not (ROOT / manifest).is_file():
            continue
        seen.add(crate_dir)
        frontier.extend(_manifest_path_deps(manifest, LIB_DEP_SECTIONS))

    rows = []
    for crate_dir in sorted(seen):
        files = tracked(f"{crate_dir}/src/*.rs")
        if not files:
            continue
        crate_code = strip_line_comments("\n".join(read(f) for f in files))
        if "pub fn reroot_foreign" in crate_code:
            continue  # the crate that IS the rule
        # One row per FILE: a failure has to name the file to edit, and an
        # exemption is written at the site it excuses.
        for f in files:
            code = strip_line_comments(read(f))
            named = names_among(code, vars_)
            if not named:
                continue
            rows.append({
                "path": f,
                "crate": Path(crate_dir).name,
                "sdk_path_vars": named,
                "routes_paths": "nros_build_paths::" in code,
                "raw_path_reads": raw_reads_of(code, vars_),
            })
    return sorted(rows, key=lambda r: r["path"])


def knob_sources() -> dict[str, int]:
    kconfig = 0
    for f in ("zephyr/Kconfig", "packages/rmw/zenoh/zpico-zephyr/Kconfig"):
        kconfig += len(re.findall(r"^config [A-Z0-9_]+", read(f), re.M))
    cargo_build = read("zephyr/cmake/nros_cargo_build.cmake")
    return {
        "nros-platform.toml (RFC-0049 platform rung)": len(tracked("*nros-platform.toml")),
        "nros-board.toml (RFC-0064 board rung)": len(tracked("*nros-board.toml")),
        "nros-rmw.toml (RFC-0071 backend descriptor)": len(tracked("*nros-rmw.toml")),
        "Kconfig symbols (the Zephyr rung)": kconfig,
        "cmake _nros_resolve_knob() forwards": len(re.findall(r"_nros_resolve_knob\(", cargo_build)),
        "system.toml (the leaf's declaration)": len(tracked("*system.toml")),
        "system.contract.yaml (declared QoS)": len(tracked("*system.contract.yaml")),
    }


def readers() -> dict[str, int]:
    """Crates reaching each shared resolution surface."""
    surfaces = [
        "nros_platform_config",
        "nros_board_common",
        "nros_zephyr_build",
        "nros_sizing_descriptor",
        "nros_build_paths",
    ]
    out = {}
    for s in surfaces:
        r = subprocess.run(
            ["git", "grep", "-l", s, "--", "*.rs"],
            cwd=ROOT, capture_output=True, text=True,
        )
        out[s] = len(r.stdout.split()) if r.returncode == 0 else 0
    return out


def inventory() -> dict:
    scripts = build_scripts()
    by_category: dict[str, int] = {}
    by_role: dict[str, list[str]] = {name: [] for name, _, _, _ in ROLES}
    for r in scripts:
        by_category[r["category"]] = by_category.get(r["category"], 0) + 1
        if r["role"]:
            by_role[r["role"]].append(r["path"])

    libs = build_script_libs()
    unprotected = [
        r for r in scripts
        if (r["raw_path_reads"] or r["private_path_helper"]) and r["sdk_path_vars"]
    ] + [r for r in libs if r["raw_path_reads"]]
    return {
        "roads": ROADS,
        "build_scripts": {
            "total": len(scripts),
            "by_category": dict(sorted(by_category.items(), key=lambda kv: -kv[1])),
            "by_role": {
                name: {"label": label, "count": len(by_role[name]),
                       "exemplar": by_role[name][0] if by_role[name] else None,
                       # Why this rule sits where it does in the order. Carried
                       # into the JSON rather than left as a source comment: the
                       # ORDER is what decides a contested row, so a reader
                       # auditing a classification needs it beside the answer.
                       "why": why, "members": by_role[name]}
                for name, label, _, why in ROLES
            },
            "unclassified": [r["path"] for r in scripts if r["role"] is None],
            "by_source_origin": {
                name: [r["path"] for r in scripts if r["source_origin"] == name]
                for name, _ in SOURCE_ORIGINS
            },
            "board_roles": {
                name: [r["path"] for r in scripts if r["board_role"] == name]
                for name in ("base", "overlay")
            },
            "rows": scripts,
        },
        "path_resolution": {
            "sdk_path_vars": sdk_path_vars(),
            "board_env_path_vars": board_env_path_vars(),
            "path_vars": path_vars(),
            "scripts_naming_one": sum(1 for r in scripts if r["sdk_path_vars"]),
            "routed_through_nros_build_paths": sum(
                1 for r in scripts if r["sdk_path_vars"] and r["routes_paths"]
            ),
            # phase-471 W6 — the build-script LIBRARIES, a second population for
            # the same rule. `nros-board-common` is reached from four board
            # `build.rs` files, so a raw read there is worse than one in a single
            # script; it was invisible while the population was `build.rs` alone.
            "libs_naming_one": len(libs),
            "libs_routed": sum(1 for r in libs if r["routes_paths"]),
            "libs": [r["path"] for r in libs],
            "unprotected": [
                {"path": r["path"],
                 "raw_reads": r["raw_path_reads"],
                 "private_helper": r.get("private_path_helper", False)}
                for r in unprotected
            ],
        },
        "knob_sources": knob_sources(),
        "readers": readers(),
    }


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--roads", action="store_true", help="only the road table")
    ap.add_argument("--scripts", action="store_true", help="only the build.rs census")
    ap.add_argument("--json", action="store_true", help="machine-readable")
    args = ap.parse_args()

    inv = inventory()
    if args.json:
        print(json.dumps(inv, indent=2, sort_keys=True))
        return 0

    want_all = not (args.roads or args.scripts)

    if args.roads or want_all:
        print("BUILD ROADS — how a knob reaches the compiler on each")
        print(f"  {len(ROADS)} roads, {len(ROADS)} DIFFERENT carriers. That is the finding,")
        print("  not an accident: each carrier has produced its own delivery defect.")
        for name, r in ROADS.items():
            print(f"\n  {name}  ({r['exec']})")
            print(f"    stage 4 emits a root: {'yes' if r['emits_root'] else 'no'}")
            print(f"    carrier: {r['carrier']}")
            print(f"    hazard:  {r['hazard']}")
        print()

    if args.scripts or want_all:
        bs = inv["build_scripts"]
        print(f"BUILD SCRIPTS — {bs['total']} tracked `build.rs`")
        print("  by category:")
        for k, v in bs["by_category"].items():
            print(f"    {v:>4}  {k}")
        print("  by ROLE — what the script is FOR (ordered; first match wins):")
        for name, meta in bs["by_role"].items():
            if not meta["count"]:
                continue
            print(f"    {meta['count']:>4}  {name:<17} {meta['label']}")
            print(f"          e.g. {meta['exemplar']}")
        if bs["unclassified"]:
            print(f"    {len(bs['unclassified']):>4}  UNCLASSIFIED — rule these, "
                  "do not add an 'other' bucket:")
            for p in bs["unclassified"]:
                print(f"          {p}")
        print()

        print("  SOURCE ORIGIN of each `c-compiler` — where its sources come from.")
        print("  Only the first is exposed to issue 1280; the other two cannot be.")
        for name, paths in bs["by_source_origin"].items():
            if not paths:
                continue
            print(f"    {len(paths):>4}  {name}")
            for q in paths:
                print(f"          {q}")
        print()

        print("  BOARD CRATES — base vs overlay, read from the manifests.")
        for name, paths in bs["board_roles"].items():
            print(f"    {len(paths):>4}  {name}")
            for q in paths:
                print(f"          {q}")
        print()

        pr = inv["path_resolution"]
        print("PATH RESOLUTION — issue 1280's build-script half")
        print(f"    {len(pr['sdk_path_vars']):>4}  path-valued variables "
              "(read from just/sdk-env.just)")
        only_board = sorted(set(pr["board_env_path_vars"]) - set(pr["sdk_path_vars"]))
        print(f"    {len(only_board):>4}  MORE from board descriptors' cargo_config "
              f"[env] — a second producer{', ' + ', '.join(only_board) if only_board else ''}")
        print(f"    {pr['scripts_naming_one']:>4}  build scripts naming at least one")
        print(f"    {pr['routed_through_nros_build_paths']:>4}  of those reaching "
              "nros_build_paths (the ONE three-valued rule)")
        print(f"    {pr['libs_naming_one']:>4}  build-script LIBRARY files naming one "
              f"({pr['libs_routed']} reaching nros_build_paths)")
        for q in pr["libs"]:
            print(f"          {q}")
        print(f"    {len(pr['unprotected']):>4}  resolving one WITHOUT it — "
              "an inherited value from another checkout would win")
        for u in pr["unprotected"]:
            how = ", ".join(u["raw_reads"]) or "private env_path helper"
            if u["raw_reads"] and u["private_helper"]:
                how += " + private env_path helper"
            print(f"          {u['path']}  ({how})")
        if not pr["unprotected"]:
            # A row nobody ever sees is a row nobody trusts. This line read 5
            # on 2026-09-28 (issue 1527) over build scripts alone, and 10 more
            # in the library population phase-471 W6 added, so the zero is a
            # measurement. Gate: `check-build-script-path-resolution`.
            print("          (it read 5 before issue 1527 and 10 more before "
                  "phase-471 W6 widened the population)")
        print()

    if want_all:
        print("KNOB SOURCES — what can state a value")
        for k, v in inv["knob_sources"].items():
            print(f"    {v:>4}  {k}")
        print()
        print("SHARED RESOLUTION SURFACES — files reaching each")
        for k, v in inv["readers"].items():
            print(f"    {v:>4}  {k}")
        print()
        print("  Ladder order is RFC-0049's: builtin < platform < board < env.")
        print("  The map and the rationale: docs/reference/canonical-build-path.md")

    return 0


if __name__ == "__main__":
    sys.exit(main())
