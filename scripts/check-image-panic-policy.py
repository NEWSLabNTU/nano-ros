#!/usr/bin/env python3
"""phase-366 M6 / RFC-0077 — every image declares exactly one ending.

`check-archive-lang-items.sh` counts per LINK LINE, which catches DUPLICATION
and is structurally blind to ABSENCE: an image with no provider has no archive
to count. Issue 0617's `#[panic_handler] function required` was exactly that,
and it is caught today only by a link failing four crates from the cause.

This gate is the other half, and it is buildless: it reads what each image
DECLARES (`nros::main!(panic = …)`) and what it SUPPLIES (a provider in its own
source, or a dependency whose whole job is to be one), and fails when the two
disagree.

    panic = "own"                 the image MUST supply a provider
    panic = "platform" | "halt"   the image must supply NONE (the macro emits it)

It exists so M5's default flip rests on a check rather than on a careful grep.
Twice in this phase a count came from matching a NAME that also appears in prose
about the name (`staticlib` in a manifest comment; "`nros::main!` is not used
here"), each time off in a different direction.

WHAT IT CANNOT SEE, stated so the coverage is not overread:

  * a provider reached through a dependency this list does not name;
  * a provider inside a `#[cfg]` arm that this build would not take;
  * the C/C++ side, where the policy is a cargo feature on the staticlib rather
    than a macro argument — `nano_ros_entry(… PANIC …)` is checked by CMake at
    configure time (a conflicting or unappliable PANIC is a FATAL_ERROR there).

So it is a source-level gate, in the same family as `check-weak-symbols.sh`, and
it says so rather than claiming to be the artifact-level one.
"""

import re
import subprocess
import sys
from pathlib import Path
import sys as _sys
from pathlib import Path as _Path
_sys.path.insert(0, str(_Path(__file__).resolve().parent / "lib"))
from tracked import tracked  # issue 0721: index lookup, not a walk

# Crates whose purpose is to define `#[panic_handler]`, plus `zephyr`, which
# supplies the RTOS's own (see `zephyr_entry/src/lib.rs`: "Zephyr's allocator +
# panic + boot belong to the RTOS").
PROVIDER_CRATES = ("panic_semihosting", "panic_halt", "panic_probe",
                   "esp_backtrace", "panic_abort", "zephyr")

# Hosted targets are excluded, because libstd defines the lang item there and
# the macro's own emit is gated the same way
# (`cfg(not(any(target_os = "linux", "macos", "windows")))`). Keeping the two in
# step matters more than either spelling: a gate that demands a provider where
# the macro deliberately emits none would fail every native example.
HOSTED = ("linux", "apple", "darwin", "windows")

MAIN_CALL = re.compile(r"nros::main!\s*\(([^;]*)\)\s*;", re.S)
PANIC_ARG = re.compile(r'panic\s*=\s*"([a-z]+)"')


def is_hosted(crate_dir: Path) -> bool:
    """Does this leaf build for a target whose libstd brings the lang item?

    Read from the leaf's own `.cargo/config.toml` `[build] target`, which is
    where every embedded example in this tree pins its triple. No `target` at all
    means the host triple, which is hosted by definition.
    """
    cfg = crate_dir / ".cargo" / "config.toml"
    if not cfg.is_file():
        return True
    m = re.search(r'^\s*target\s*=\s*"([^"]+)"', cfg.read_text(), re.M)
    if not m:
        return True
    return any(h in m.group(1) for h in HOSTED)


def has_provider(src_dir: Path) -> bool:
    for rs in tracked(src_dir, suffix=".rs"):
        try:
            text = rs.read_text()
        except OSError:
            continue
        for line in text.splitlines():
            code = line.split("//")[0]
            if "#[panic_handler]" in code:
                return True
            if "panic_to_platform!()" in code or "panic_halt!()" in code:
                return True
            for crate in PROVIDER_CRATES:
                if re.search(rf"\b(use|extern\s+crate)\s+{crate}\b", code):
                    return True
    return False


def main() -> int:
    root = Path(__file__).resolve().parents[1]
    bad = []
    checked = 0
    # Enumerate through the GIT INDEX, not the filesystem (issue 0684).
    #
    # `Path.glob("**")` has to DESCEND a tree to discover it, and these two
    # roots contain every cmake build dir, west workspace and `_deps/` checkout
    # in the repo. The previous walk found 974 `main.rs`, kept 776 after
    # filtering — and only 139 of those are tracked. The other 637 were build
    # output: staged per-RMW copies of the same entry (`build-zenoh/`,
    # `build-cyclonedds/`, `build-xrce/`) and vendored third-party sources, of
    # which the most eloquent was
    #
    #     …/talker/build-zenoh/_deps/corrosion-src/test/hostbuild/hostbuild/src/main.rs
    #
    # i.e. Corrosion's own test fixture, judged by a nano-ros panic gate.
    #
    # The old filter — `("target", "build", "generated")` — is an EXACT
    # component match, so `build-zenoh` never matched `build`. Extending that
    # list is the wrong repair: it is a denylist against a set nobody controls,
    # and it still pays the traversal to build the list it then throws away.
    # The index knows what is ours, costs no walk, and cannot drift.
    listed = subprocess.run(
        ["git", "ls-files", "examples/**/src/main.rs",
         "packages/testing/**/src/main.rs"],
        capture_output=True, text=True, check=True, cwd=root,
    ).stdout.split()
    for rel in listed:
        main_rs = root / rel
        # Strip comment lines BEFORE matching, rather than matching and then
        # trying to tell prose from code.
        #
        # The first version did the latter and had a false NEGATIVE that this
        # phase has now produced three times in different clothes: `[^;]*` spans
        # newlines, so a `//! nros::main!()` in a doc comment matched and
        # swallowed everything down to the real call's `;`. The prose filter then
        # skipped that one match — and with it the file. The rule that keeps
        # working is: match the CALL, not the name, and remove the prose first.
        code = "\n".join(
            line.split("//")[0] for line in main_rs.read_text().splitlines()
        )
        call = MAIN_CALL.search(code)
        if call is None:
            continue
        if is_hosted(main_rs.parent.parent):
            continue
        checked += 1
        arg = PANIC_ARG.search(call.group(1))
        # Must track the macro's default (M5 flipped it Own -> Platform). If the
        # two drift, this gate demands a provider exactly where the macro emits
        # one, or vice versa.
        policy = arg.group(1) if arg else "platform"
        supplies = has_provider(main_rs.parent)
        rel = main_rs.relative_to(root)
        if policy == "own" and not supplies:
            bad.append(
                f"  {rel}: says `panic = \"own\"` (or defaults to it) but supplies "
                f"no provider.\n"
                f"      Either declare `panic = \"platform\"` and let the entry emit "
                f"one, or add the provider this image brings."
            )
        elif policy in ("platform", "halt") and supplies:
            bad.append(
                f"  {rel}: declares `panic = \"{policy}\"` AND supplies its own "
                f"provider — that is two, which is `E0152 duplicate lang item`.\n"
                f"      Say `panic = \"own\"` if the image's own provider is the "
                f"one it wants."
            )
    if bad:
        print("check-image-panic-policy: images whose declaration and provider "
              "disagree\n", file=sys.stderr)
        print("\n".join(bad), file=sys.stderr)
        print(f"\n  ({checked} image(s) with a real `nros::main!()` call "
              f"examined)", file=sys.stderr)
        return 1
    print(f"check-image-panic-policy: OK ({checked} image(s) declare exactly one "
          f"ending)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
