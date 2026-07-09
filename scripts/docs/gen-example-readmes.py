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

Existing README.md files are NEVER overwritten: hand-written pages
(`native/c/custom-platform`, `zephyr/cpp/talker`, …) win.

Usage:
    scripts/docs/gen-example-readmes.py [--check] [<leaf> ...]

    --check   exit 1 and list leaves missing a README (no writes) — the shape
              gate's shell equivalent.
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
    """The `<target>` in `[package.metadata.nros.deploy.<target>]`, if any."""
    cargo = leaf / "Cargo.toml"
    if not cargo.is_file():
        return None
    m = re.search(
        r"^\[package\.metadata\.nros\.deploy\.([^\]]+)\]", cargo.read_text(), re.M
    )
    return m.group(1) if m else None


def rmw_features(leaf: pathlib.Path) -> list[str]:
    """`rmw-*` cargo features this leaf actually declares."""
    cargo = leaf / "Cargo.toml"
    if not cargo.is_file():
        return []
    return re.findall(r"^(rmw-[a-z0-9-]+)\s*=", cargo.read_text(), re.M)


def cmake_knob(leaf: pathlib.Path) -> str:
    """Which nano-ros CMake function carries this leaf's deploy knobs."""
    cml = leaf / "CMakeLists.txt"
    if not cml.is_file():
        return "none"
    text = cml.read_text()
    if "nano_ros_deploy(" in text:
        return "deploy"
    if "nano_ros_entry(" in text:
        return "entry"
    return "none"


def render(leaf: pathlib.Path) -> str:
    platform, language, case = leaf.parts[-3:]
    is_rust = (leaf / "Cargo.toml").is_file()
    is_cmake = (leaf / "CMakeLists.txt").is_file()
    native = platform in NATIVE_PLATFORMS

    # Links are absolute on purpose: a copied-out directory has no repo above it,
    # so relative paths back into the checkout would 404 — which is the exact
    # failure #170 is about.
    lines = [
        f"# `{case}` — {platform} / {language}",
        "",
        "Standalone copy-out example: copy this directory anywhere, nothing above it",
        f"is required ([RFC-0026]({GH}/docs/design/0026-example-directory-layout.md)).",
        "",
        "## Build",
        "",
        "```bash",
        f"cp -r examples/{platform}/{language}/{case} ~/my-{case} && cd ~/my-{case}",
    ]

    if is_rust:
        lines += [
            "NROS_REPO_DIR=/path/to/nano-ros nros sync   # msg crates + [patch.crates-io]",
            "cargo build",
            "```",
        ]
    elif is_cmake:
        lines += [
            "cmake -S . -B build -DNANO_ROS_ROOT=/path/to/nano-ros   # or: export NROS_REPO_DIR=…",
            "cmake --build build",
            "```",
        ]
    else:  # pragma: no cover — every leaf is one or the other today
        lines += ["```"]

    lines += ["", "## Run", ""]
    if native and is_rust:
        lines += [
            "Needs a zenoh router (`just native zenohd` in the nano-ros checkout):",
            "",
            "```bash",
            "cargo run",
            "```",
        ]
    elif native and is_cmake:
        lines += [
            "Needs a zenoh router (`just native zenohd` in the nano-ros checkout);",
            "the built binary lands under `build/`.",
        ]
    else:
        lines += [
            "Cross-built. SDK env comes from `source activate.sh` in the checkout;",
            f"QEMU / flashing steps live in the [{platform} README]"
            f"({GH}/examples/{platform}/README.md).",
        ]

    lines += ["", "## Config", ""]
    tgt = deploy_target(leaf)
    knob = cmake_knob(leaf)
    if is_rust and tgt:
        lines += [
            "Board, RMW, domain and locator: `Cargo.toml` →",
            f"`[package.metadata.nros.deploy.{tgt}]`.",
        ]
    elif is_rust:
        feats = rmw_features(leaf)
        if feats:
            lines += [
                "RMW is a Cargo feature (`--features " + " | ".join(feats) + "`);",
                "locator and domain come from `NROS_LOCATOR` / `ROS_DOMAIN_ID`.",
            ]
        else:
            lines += [
                "Locator and domain come from `NROS_LOCATOR` / `ROS_DOMAIN_ID`.",
            ]
    elif knob == "deploy":
        lines += [
            "Deploy knobs: `nano_ros_deploy(TARGET … RMW … DOMAIN_ID … LOCATOR …)`",
            "in `CMakeLists.txt`; override the backend with `-DNROS_RMW=<backend>`.",
        ]
    elif knob == "entry":
        lines += [
            "Wiring lives in `nano_ros_entry(…)` in `CMakeLists.txt`; select the",
            "backend with `-DNROS_RMW=<backend>`, locator/domain via",
            "`NROS_LOCATOR` / `ROS_DOMAIN_ID`.",
        ]
    else:
        lines += [
            "See `CMakeLists.txt`; select the backend with `-DNROS_RMW=<backend>`.",
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

    for leaf in missing:
        (leaf / "README.md").write_text(render(leaf))
        print(f"wrote {leaf.relative_to(REPO)}/README.md")
    print(f"\n{len(missing)} generated, {len(leaves) - len(missing)} already present")
    return 0


if __name__ == "__main__":
    sys.exit(main())
