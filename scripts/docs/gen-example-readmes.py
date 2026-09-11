#!/usr/bin/env python3
"""Generate the minimal per-leaf README every canonical example owes its copy-out
contract (issue #170 / RFC-0026).

A canonical leaf is `examples/<platform>/<language>/<case>/` carrying a
`package.xml`. Workspaces, templates and bridges have their own README
conventions and are skipped.

The generated page is deliberately small: how to copy the directory out and
build it standalone, where to run it, and which file carries the deploy knobs.
Anything platform-specific (flashing, QEMU invocation, SDK env) stays in the
platform README, which we link rather than duplicate — one source of truth.

Hand-written pages (`native/c/custom-platform`, `zephyr/cpp/talker`, …) always
win: a page is only ever rewritten if it still carries the generated banner
(`GENERATED_BANNER` below), and without `--force` nothing existing is touched
at all.

Usage:
    scripts/docs/gen-example-readmes.py [--check] [--force] [<leaf> ...]

    --check   exit 1 and list leaves missing a README (no writes) — the shape
              gate's shell equivalent.
    --force   also rewrite the pages this script previously generated, so a
              template change reaches all of them. Hand-written pages are
              still left alone.
"""

from __future__ import annotations

import argparse
import pathlib
import re
import subprocess
import sys

REPO = pathlib.Path(__file__).resolve().parents[2]
EXAMPLES = REPO / "examples"
SKIP_TOP = {"workspaces", "templates", "bridges"}

# Platforms whose binaries run on the host; everything else is cross-built and
# run under QEMU / on hardware, so we point at the platform README instead of
# inventing a run line.
NATIVE_PLATFORMS = {"native", "threadx-linux"}


def canonical_leaves() -> list[pathlib.Path]:
    """`examples/<platform>/<language>/<case>` dirs carrying a package.xml."""
    out = subprocess.run(
        ["git", "ls-files", "examples/**/package.xml"],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=True,
    ).stdout.split()
    leaves = []
    for p in out:
        d = pathlib.Path(p).parent
        parts = d.parts
        if len(parts) != 4 or parts[1] in SKIP_TOP:
            continue
        leaves.append(REPO / d)
    return sorted(set(leaves))


GH = "https://github.com/NEWSLabNTU/nano-ros/blob/main"


def deploy_target(leaf: pathlib.Path) -> str | None:
    """The `<id>` of the leaf's `[image.<id>]` in `system.toml`, if any.

    phase-445 W3b (RFC-0098 D3/D5): a single-package leaf states its board and
    network identity in `system.toml` beside its manifest; the retired
    `[package.metadata.nros.deploy.<target>]` table is gone.
    """
    system = leaf / "system.toml"
    if not system.is_file():
        return None
    m = re.search(r"^\[image\.([^\]]+)\]", system.read_text(), re.M)
    return m.group(1) if m else None


def system_rmw(leaf: pathlib.Path) -> str | None:
    """`[system] rmw` from the leaf's `system.toml`, if it has one."""
    system = leaf / "system.toml"
    if not system.is_file():
        return None
    m = re.search(r'^rmw\s*=\s*"([^"]+)"', system.read_text(), re.M)
    return m.group(1) if m else None


def bin_name(leaf: pathlib.Path) -> str | None:
    """The single `[[bin]] name` a Rust leaf declares, else the package name.

    Used only to name the artifact `nros build` leaves behind; a leaf with more
    than one binary gets no path printed rather than a guessed one.
    """
    cargo = leaf / "Cargo.toml"
    if not cargo.is_file():
        return None
    text = cargo.read_text()
    bins = re.findall(r'\[\[bin\]\][^\[]*?name\s*=\s*"([^"]+)"', text, re.S)
    if len(bins) == 1:
        return bins[0]
    if bins:
        return None
    m = re.search(r'^\[package\][^\[]*?^name\s*=\s*"([^"]+)"', text, re.S | re.M)
    return m.group(1) if m else None


# The line that marks a page as this script's output. `--force` rewrites only
# pages carrying it, so a hand-written README can never be clobbered.
GENERATED_BANNER = (
    "Standalone copy-out example: copy this directory anywhere, nothing above it"
)


def is_generated(path: pathlib.Path) -> bool:
    return path.is_file() and GENERATED_BANNER in path.read_text()


def render(leaf: pathlib.Path) -> str:
    platform, language, case = leaf.parts[-3:]
    is_rust = (leaf / "Cargo.toml").is_file()
    is_cmake = (leaf / "CMakeLists.txt").is_file()
    native = platform in NATIVE_PLATFORMS
    zephyr = platform == "zephyr"
    image = deploy_target(leaf)
    binary = bin_name(leaf) if is_rust else None
    zephyr_readme = f"[zephyr README]({GH}/examples/zephyr/README.md)"

    # Links are absolute on purpose: a copied-out directory has no repo above it,
    # so relative paths back into the checkout would 404 — which is the exact
    # failure #170 is about.
    lines = [
        f"# `{case}` — {platform} / {language}",
        "",
        GENERATED_BANNER,
        f"is required ([RFC-0026]({GH}/docs/design/0026-example-directory-layout.md)).",
        "",
        "## Build",
        "",
        "```bash",
        f"cp -r examples/{platform}/{language}/{case} ~/my-{case} && cd ~/my-{case}",
    ]

    # The board, the RMW and the deployment identity come from `system.toml`
    # (RFC-0098 D3/D5), so no build command here carries them: `nros sync`
    # turns that one choice into the generated settings the build reads.
    if zephyr and is_rust:
        lines += [
            "export NROS_REPO_DIR=/path/to/nano-ros   # your nano-ros checkout",
            "nros sync                                # generated/ message crates",
            "```",
            "",
            "Zephyr is the carve-out: the build verb is `west`, not `nros build`, and",
            "the board plus the `CONF_FILE` RMW overlay are west arguments — see the",
            f"{zephyr_readme}.",
        ]
    elif zephyr:
        lines += [
            "```",
            "",
            "Zephyr is the carve-out: the build verb is `west`, and the board plus the",
            f"`CONF_FILE` RMW overlay are west arguments — see the {zephyr_readme}.",
            "No `nros sync` here: a C/C++ leaf's message bindings are a build-system",
            "output, generated while the project configures.",
        ]
    elif is_rust and image:
        lines += [
            "export NROS_REPO_DIR=/path/to/nano-ros   # your nano-ros checkout",
            "nros sync     # generated/ message crates + build/<image>/nros-cargo.toml",
            f"nros build    # or: nros build {image}",
            "```",
        ]
    elif is_rust:
        # Not migrated to `system.toml` yet (RFC-0098 D3), so there is no
        # `[image.*]` for `nros build` to resolve — cargo still drives it.
        lines += [
            "export NROS_REPO_DIR=/path/to/nano-ros   # your nano-ros checkout",
            "nros sync                                # generated/ message crates",
            "cargo build",
            "```",
            "",
            "This leaf carries no `system.toml` yet, so it declares no `[image.*]` for",
            "`nros build` to resolve (RFC-0098 D3); cargo drives it until it gains one.",
        ]
    elif is_cmake:
        lines += [
            "cmake -S . -B build -DNANO_ROS_ROOT=/path/to/nano-ros   # or: export NROS_REPO_DIR=…",
            "cmake --build build",
            "```",
            "",
            "No `nros sync` here: a C/C++ leaf's message bindings are a CMake-time",
            "output. `-DNANO_ROS_ROOT` only says where the checkout is — the board, the",
            "RMW and the domain come from `system.toml` (below), never from a `-D` flag.",
        ]
    else:  # pragma: no cover — every leaf is one or the other today
        lines += ["```"]

    lines += ["", "## Run", ""]
    router = (
        "Needs a zenoh router (`ros2 run rmw_zenoh_cpp rmw_zenohd`)"
        if system_rmw(leaf) in (None, "zenoh")
        else "Needs the host daemon of the RMW `system.toml` names"
    )
    if native and is_rust and image and binary:
        lines += [
            f"{router}.",
            f"`nros build` leaves the binary at `build/{image}/target/debug/{binary}`.",
        ]
    elif native and is_rust:
        lines += [
            f"{router}.",
            "The binary lands under `target/debug/`; `cargo run` runs it, after the",
            "`nros sync` above.",
        ]
    elif native and is_cmake:
        lines += [
            f"{router}.",
            "The built binary lands under `build/`.",
        ]
    else:
        lines += [
            "Cross-built. SDK env comes from `source activate.sh` in the checkout;",
            f"QEMU / flashing steps live in the [{platform} README]"
            f"({GH}/examples/{platform}/README.md).",
        ]
        if is_rust and image and binary and not zephyr:
            lines += [
                "",
                "`nros build` leaves the image at",
                f"`build/{image}/target/<triple>/debug/{binary}`",
                "— `<triple>` is the board's Rust target, from the generated settings.",
            ]

    lines += ["", "## Config", ""]
    manifest = "`Cargo.toml`" if is_rust else "`CMakeLists.txt`"
    if image and is_rust:
        # Issue 1305: a single-package leaf IS its own entry, so it still names
        # its board crate in `[dependencies]` — RFC-0098 D6 generates that only
        # for a workspace entry. Do not promise a one-line board switch here.
        lines += [
            f"Board, RMW, domain and locator: `system.toml` beside {manifest}",
            f"(`[image.{image}]` + `[system]`, RFC-0098 D3/D5). No build command",
            "carries any of them.",
            "",
            "Switching board is two edits today, not one: the `[image.*] board` line",
            f"and the board crate this leaf names in {manifest}'s `[dependencies]`. A",
            "single-package leaf is its own entry, and RFC-0098 D6's generated board",
            "dependency reaches only a workspace entry — leave the two disagreeing and",
            "`nros sync` reports success while the build fails in your own crate",
            f"([issue 1305]({GH}/docs/issues/1305-single-package-board-crate-dep-not-generated.md)).",
        ]
    elif image:
        lines += [
            f"Board, RMW, domain and locator: `system.toml` beside {manifest}",
            f"(`[image.{image}]` + `[system]`, RFC-0098 D3/D5). `find_package(nano_ros)`",
            "reads it while CMake configures, so switching board is editing that one",
            "line and re-configuring (`cmake -B build`); no build command and no",
            "manifest names a board.",
        ]
    else:
        lines += [
            f"Board, RMW, domain and locator belong in a `system.toml` beside {manifest}",
            "(`[image.<id>]` + `[system]`, RFC-0098 D3/D5). This leaf has not been",
            "migrated yet, so it still selects its backend with the `rmw-*` Cargo",
            "features in `Cargo.toml` — the spelling RFC-0098 retires.",
        ]

    lines += [
        "",
        f"Copy-out contract + the full example matrix: [`examples/README.md`]({GH}/examples/README.md).",
        "",
    ]
    return "\n".join(lines)


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="report missing, write nothing")
    ap.add_argument(
        "--force",
        action="store_true",
        help="also rewrite pages this script generated (hand-written ones are kept)",
    )
    ap.add_argument("leaves", nargs="*", help="restrict to these leaf dirs")
    args = ap.parse_args()

    leaves = canonical_leaves()
    if args.leaves:
        want = {(REPO / p).resolve() for p in args.leaves}
        leaves = [leaf for leaf in leaves if leaf.resolve() in want]

    missing = [leaf for leaf in leaves if not (leaf / "README.md").is_file()]

    if args.check:
        for leaf in missing:
            print(leaf.relative_to(REPO))
        if missing:
            print(
                f"\n{len(missing)} canonical leaf/leaves lack README.md — run "
                "scripts/docs/gen-example-readmes.py",
                file=sys.stderr,
            )
            return 1
        return 0

    targets = list(missing)
    if args.force:
        targets += [
            leaf
            for leaf in leaves
            if leaf not in missing and is_generated(leaf / "README.md")
        ]

    rewritten = 0
    for leaf in targets:
        page = leaf / "README.md"
        text = render(leaf)
        if page.is_file() and page.read_text() == text:
            continue
        page.write_text(text)
        rewritten += 1
        print(f"wrote {leaf.relative_to(REPO)}/README.md")
    kept = len(leaves) - len(targets)
    print(f"\n{rewritten} written, {len(targets) - rewritten} already current, {kept} hand-written")
    return 0


if __name__ == "__main__":
    sys.exit(main())
