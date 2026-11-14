#!/usr/bin/env python3
"""phase-561 W4 -- NuttX images take no `libc` crate, patched or plain.

NuttX sizes several opaque C types (`pthread_attr_t`, `sem_t`, ...) from its
Kconfig. A Rust mirror of a fixed size corrupts its caller whenever it is
smaller than the kernel's. Issues 0167 and 0570 were two such mirrors in the
vendored `libc` fork, and each smashed a stack frame with no diagnostic.
Phase-561 retired that fork once no NuttX graph compiled `libc` for the
target. This gate keeps it that way, in two halves.

Static (fast line, default):
  - no tracked TOML file has a `[patch.*]` row for `libc`;
  - the SDK index has no `nuttx-libc` source, and `.gitmodules` has no
    `third-party/nuttx/libc` submodule.

Graph (`--graph`, run by the NuttX fixture lane after it builds, so every
root it resolves exists):
  - for each NuttX cargo root, `cargo tree -i libc --target <nuttx triple>`
    over normal, non-proc-macro edges finds nothing. That is what compiles
    for the image; a host-only `libc` (a build script's, a proc macro's)
    is fine.

Usage:  check-nuttx-libc-retired.py [--graph] [--self-test]
"""

from __future__ import annotations

import os
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]

LESSON = """\
  NuttX sizes several opaque C types from its Kconfig, so a Rust mirror of a
  fixed size corrupts its caller whenever it is smaller than the kernel's
  (issues 0167 and 0570: two stack-smashing mirrors, no diagnostic). A NuttX
  graph that needs `libc` again needs struct mirrors sized from the kernel's
  own configuration, with a size gate measuring them against the built
  headers. It never needs plain crates.io `libc`, and never a fork restored
  without that gate."""

HEADER = re.compile(r"^\s*\[\s*([^\]]+?)\s*\]\s*(#.*)?$")
KEY = re.compile(r"^\s*(\"?)libc\1\s*=")


def patch_rows(text: str) -> list[int]:
    """1-based line numbers of `libc` rows inside a `[patch.*]` table, or of a
    `[patch.<registry>.libc]` header."""
    hits = []
    table = ""
    for n, line in enumerate(text.splitlines(), 1):
        m = HEADER.match(line)
        if m:
            table = m.group(1).replace('"', "").replace(" ", "")
            if table.startswith("patch.") and table.split(".")[-1] == "libc":
                hits.append(n)
            continue
        if (table == "patch" or table.startswith("patch.")) and KEY.match(line):
            hits.append(n)
    return hits


def tracked_tomls() -> list[str]:
    out = subprocess.run(
        ["git", "-C", str(REPO), "ls-files", "-z", "--", "*.toml"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    return [p for p in out.split("\0") if p]


def static_half() -> list[str]:
    errors = []
    for rel in tracked_tomls():
        path = REPO / rel
        try:
            text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        for n in patch_rows(text):
            errors.append(f"{rel}:{n}: a [patch] row for `libc`")
    index = (REPO / "nros-sdk-index.toml").read_text()
    if re.search(r"^\s*\[source\.nuttx-libc\]", index, re.M):
        errors.append("nros-sdk-index.toml: a `[source.nuttx-libc]` row")
    if re.search(r'"nuttx-libc"', index):
        errors.append("nros-sdk-index.toml: `nuttx-libc` named in a source list")
    gm = REPO / ".gitmodules"
    if gm.is_file() and "third-party/nuttx/libc" in gm.read_text():
        errors.append(".gitmodules: the `third-party/nuttx/libc` submodule")
    return errors


def nuttx_roots() -> list[tuple[str, Path, list[str], str]]:
    """(label, dir, extra cargo args, target) for every NuttX cargo root this
    tree has built. The FFI crates are tracked; workspace entries are found by
    their generated `nros-cargo.toml` under a `nuttx-*` build dir."""
    board = REPO / "packages/boards/nros-board-nuttx-qemu"
    riscv = str(board / "riscv32imac-unknown-nuttx-elf.json")
    roots = [
        ("cmake FFI crate (arm)", board / "nros-nuttx-ffi", [], "armv7a-nuttx-eabihf"),
        ("cmake FFI crate (riscv)", board / "nros-nuttx-riscv-ffi", [], riscv),
    ]
    # Roots come from the INDEX (a tracked `system.toml` marks a buildable
    # project); only each root's own `build/` dir is globbed, so this never
    # walks the tree (`check-no-tracked-file-find`).
    listed = subprocess.run(
        ["git", "-C", str(REPO), "ls-files", "-z", "--", "*system.toml"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    # A bringup's `system.toml` sits below its workspace; the workspace's
    # `build/` is at one of its ancestors, so every ancestor up to the repo
    # root is a candidate (a bounded set, never a walk).
    project_dirs = set()
    for p in listed.split("\0"):
        if not p:
            continue
        d = (REPO / p).parent
        while d != REPO and REPO in d.parents:
            project_dirs.add(d)
            d = d.parent
    project_dirs = sorted(project_dirs)
    configs = sorted(
        cfg
        for d in project_dirs
        if (d / "build").is_dir()
        for cfg in (d / "build").glob("nuttx-*/*/nros-cargo.toml")
    )
    for cfg in configs:
        entry = cfg.parent
        if not (entry / "Cargo.toml").is_file():
            continue
        text = cfg.read_text(errors="replace")
        target = riscv if "riscv" in text else "armv7a-nuttx-eabihf"
        rel = entry.relative_to(REPO)
        roots.append(
            (f"workspace entry {rel}", entry, ["--config", str(cfg)], target)
        )
    return roots


def nightly_channel() -> str:
    """The pinned nightly a NuttX board's `toolchain = "nightly"` resolves to:
    the channel of `tools/rust-toolchain.toml`, never a bare `nightly` (which
    would download an unpinned one). The riscv target is a JSON spec, which
    cargo accepts only under a nightly with `-Zjson-target-spec`."""
    text = (REPO / "tools/rust-toolchain.toml").read_text()
    m = re.search(r'^\s*channel\s*=\s*"([^"]+)"', text, re.M)
    if not m:
        raise RuntimeError("tools/rust-toolchain.toml names no channel")
    return m.group(1)


def libc_for_target(d: Path, extra: list[str], target: str) -> tuple[bool, str]:
    """(reached, text): does `libc` compile for `target` in the graph at `d`?

    `cargo tree -i` prints nothing on stdout both when the lock has no `libc`
    (stderr: "did not match any packages") and when `libc` is only reached
    under another target's cfg (stderr: "nothing to print"); either way the
    image takes none. A non-empty stdout is a path to `libc` for `target`.
    """
    cmd = [
        "cargo", "tree", "--offline", "-i", "libc", "--target", target,
        "-e", "normal,no-proc-macro", "--manifest-path", str(d / "Cargo.toml"),
        *extra,
    ]
    env = dict(os.environ, RUSTUP_TOOLCHAIN=nightly_channel())
    if target.endswith(".json"):
        cmd.insert(1, "-Zjson-target-spec")
    r = subprocess.run(cmd, cwd=d, capture_output=True, text=True, env=env)
    if r.stdout.strip():
        return True, r.stdout.strip()
    benign = "did not match any packages" in r.stderr or "nothing to print" in r.stderr
    if r.returncode != 0 and not benign:
        raise RuntimeError(r.stderr.strip())
    return False, ""


def graph_self_test() -> int:
    """Negative control for the graph half, on a synthetic graph: a `libc`
    dependency is seen for the NuttX target, and a host-cfg-only one is not."""
    import tempfile

    with tempfile.TemporaryDirectory(prefix="nros-libc-retired-") as t:
        root = Path(t)
        (root / "libc/src").mkdir(parents=True)
        (root / "libc/Cargo.toml").write_text(
            '[package]\nname = "libc"\nversion = "0.2.999"\nedition = "2021"\n'
        )
        (root / "libc/src/lib.rs").write_text("")
        for name, dep in (
            ("direct", '[dependencies]\nlibc = { path = "../libc" }\n'),
            (
                "hostonly",
                "[target.'cfg(target_arch = \"x86_64\")'.dependencies]\n"
                'libc = { path = "../libc" }\n',
            ),
        ):
            (root / name / "src").mkdir(parents=True)
            (root / name / "Cargo.toml").write_text(
                f'[package]\nname = "{name}"\nversion = "0.0.0"\nedition = "2021"\n'
                f"[workspace]\n{dep}"
            )
            (root / name / "src/lib.rs").write_text("")
            subprocess.run(
                ["cargo", "generate-lockfile", "--offline"],
                cwd=root / name,
                check=True,
                capture_output=True,
            )
        direct, _ = libc_for_target(root / "direct", [], "armv7a-nuttx-eabihf")
        host, _ = libc_for_target(root / "hostonly", [], "armv7a-nuttx-eabihf")
    if not direct or host:
        print(
            "check-nuttx-libc-retired: graph self-test FAILED: "
            f"direct dep seen={direct} (want True), host-only seen={host} (want False)"
        )
        return 1
    print("check-nuttx-libc-retired: graph self-test OK (2 synthetic graphs)")
    return 0


def graph_half() -> list[str]:
    errors = []
    roots = nuttx_roots()
    entries = [r for r in roots if r[0].startswith("workspace entry")]
    if not entries:
        errors.append(
            "no NuttX workspace entry is built here (no <project>/build/nuttx-*/"
            "*/nros-cargo.toml), so the graph half measured only the FFI crates; "
            "it runs after the NuttX fixture build, which produces them"
        )
    for label, d, extra, target in roots:
        try:
            reached, tree = libc_for_target(d, extra, target)
        except RuntimeError as e:
            errors.append(f"{label}: `cargo tree` failed:\n    " + str(e).replace("\n", "\n    "))
            continue
        if reached:
            errors.append(
                f"{label}: `libc` compiles for {Path(target).stem}:\n    "
                + tree.replace("\n", "\n    ")
            )
        else:
            print(f"  {label}: no `libc` for {Path(target).stem}")
    return errors


def self_test() -> int:
    cases = [
        ('[patch.crates-io]\nlibc = { path = "x" }\n', [2]),
        ('[patch."crates-io"]\n"libc" = { path = "x" }\n', [2]),
        ('[patch.crates-io.libc]\npath = "x"\n', [1]),
        ('[dependencies]\nlibc = "0.2"\n', []),
        ('[patch.crates-io]\nnros-core = { path = "x" }\n[dependencies]\nlibc = "0.2"\n', []),
        ('[patch.crates-io]\nlibc-stubs = { path = "x" }\n', []),
    ]
    bad = 0
    for text, want in cases:
        got = patch_rows(text)
        if got != want:
            print(f"self-test FAILED: {text!r}: want {want}, got {got}")
            bad += 1
    if bad:
        return 1
    print(f"check-nuttx-libc-retired: self-test OK ({len(cases)} cases)")
    return 0


def main(argv: list[str]) -> int:
    if "--self-test" in argv:
        return self_test()
    if self_test() != 0:
        return 1
    graph = "--graph" in argv
    if graph and graph_self_test() != 0:
        return 1
    errors = graph_half() if graph else static_half()
    half = "graph" if graph else "static"
    if errors:
        print(f"check-nuttx-libc-retired ({half}): FAIL")
        for e in errors:
            print(f"  {e}")
        print(LESSON)
        return 1
    print(f"check-nuttx-libc-retired ({half}): OK -- no NuttX `libc`, patched or plain")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
