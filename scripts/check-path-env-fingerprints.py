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

Both are classified by the same predicate. Interpolated names (`{var}`) in Rust
belong to knob tables whose values are numbers/strings and cannot be resolved
statically — producer 2 is exactly the data behind the biggest such loop.

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
    # One Zephyr build dir per image; west images are not in a shared cargo
    # group (`NROS_FIXTURE_SHARED_PLATFORMS`), and the value IS the identity of
    # the build being configured.
    "DOTCONFIG": "per-zephyr-build-dir; zephyr leaves share no cargo group",
    # A deliberate expert override naming a DIFFERENT SystemModel — a change of
    # value is a change of input, which is exactly what should re-run the script.
    "NROS_MODEL_DIR": "deprecated expert override; a new value IS a new input",
    # Name-shaped despite the suffix: the value is a NuttX-RELATIVE subpath
    # (`arch/arm/src/chip`, `arch/risc-v/src/board`), i.e. a knob, not a
    # filesystem location. One spelling everywhere by construction.
    "NUTTX_ARCH_INCLUDES": "NuttX-relative subpath list, not a location",
    "NUTTX_BOARD_LIB_DIR": "NuttX-relative subpath, not a location",
    # Set by CMake for the corrosion build it configures; that build dir owns
    # its own cargo target dir, so two spellings cannot meet in one
    # `.fingerprint/`.
    "APP_INCLUDE_DIRS": "cmake-set per build dir, which owns its target dir",
    "APP_INCLUDE_DIRS_FILE": "cmake-set per build dir, which owns its target dir",
    "APP_FFI_LIBS_FILE": "cmake-set per build dir, which owns its target dir",
}


def offenders(paths):
    """[(relpath, name)] for path-shaped names fingerprinted as strings.

    Split out so the self-test can drive it over a synthetic file.
    """
    out = []
    for rel in paths:
        try:
            text = open(os.path.join(ROOT, rel), encoding="utf-8").read()
        except (OSError, UnicodeDecodeError):
            continue
        for m in DIRECTIVE.finditer(text):
            name = m.group(1)
            if name in ALLOWED:
                continue
            if name.endswith(PATH_SUFFIXES):
                out.append((rel, name))
    return out


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
    listed = subprocess.run(
        ["git", "ls-files", "config/*/nros-platform.toml"],
        capture_output=True,
        text=True,
        cwd=ROOT,
    ).stdout.split()
    return listed


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
    """Both directions on a synthetic source — a checker that stopped checking
    passes silently, which is the shape this gate exists for."""
    import tempfile

    with tempfile.TemporaryDirectory(dir=os.path.join(ROOT, "tmp")) as d:
        probe = os.path.join(d, "build.rs")
        rel = os.path.relpath(probe, ROOT)

        def write(body):
            with open(probe, "w") as fh:
                fh.write(body)

        # A path-shaped name IS reported.
        write('fn main(){println!("cargo:rerun-if-env-changed=SOME_PLATFORM_SRC");}\n')
        if not offenders([rel]):
            sys.stderr.write("self-test: a path-shaped name was NOT reported\n")
            sys.exit(2)
        # A value-shaped name is NOT reported (knobs must stay fingerprinted).
        write('fn main(){println!("cargo:rerun-if-env-changed=ZPICO_TX_BATCH");}\n')
        if offenders([rel]):
            sys.stderr.write("self-test: a value-shaped name was reported\n")
            sys.exit(2)
        # An exempted name is not reported…
        write('fn main(){println!("cargo:rerun-if-env-changed=CARGO_TARGET_DIR");}\n')
        if offenders([rel]):
            sys.stderr.write("self-test: an exempted name was reported\n")
            sys.exit(2)
        # …and the exemption is by NAME, not by suffix.
        write('fn main(){println!("cargo:rerun-if-env-changed=OTHER_TARGET_DIR");}\n')
        if not offenders([rel]):
            sys.stderr.write("self-test: exemption leaked to a sibling name\n")
            sys.exit(2)
        # An interpolated name is skipped rather than guessed at.
        write('fn main(){println!("cargo:rerun-if-env-changed={name}");}\n')
        if offenders([rel]):
            sys.stderr.write("self-test: an interpolated name was reported\n")
            sys.exit(2)

        # …and the manifest producer, both directions on the same classifier.
        manifest = os.path.join(d, "nros-platform.toml")
        rel_manifest = os.path.relpath(manifest, ROOT)
        with open(manifest, "w") as fh:
            fh.write('rerun_if_env_changed = [\n  "THREADX_CONFIG_DIR",\n]\n')
        if not manifest_offenders([rel_manifest]):
            sys.stderr.write("self-test: a path-shaped manifest entry was NOT reported\n")
            sys.exit(2)
        with open(manifest, "w") as fh:
            fh.write('rerun_if_env_changed = ["FREERTOS_PORT", "NROS_ZENOH_DEBUG"]\n')
        if manifest_offenders([rel_manifest]):
            sys.stderr.write("self-test: a value-shaped manifest entry was reported\n")
            sys.exit(2)


def main():
    self_test()
    sources = tracked_rust_sources()
    manifests = tracked_platform_manifests()
    bad = offenders(sources) + manifest_offenders(manifests)
    if bad:
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
            "  the name to ALLOWED in this file WITH that reason.\n"
        )
        sys.exit(1)
    print(
        f"check-path-env-fingerprints: OK ({len(sources)} tracked Rust source(s), "
        f"{len(manifests)} platform manifest(s))"
    )


if __name__ == "__main__":
    main()
