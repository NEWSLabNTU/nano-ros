"""RFC-0070 R1 — the ONE build-cache root, for Python callers.

Issue 1921: `NROS_BUILD_ROOT` is checkout-scoped (a root this checkout does
not own becomes `<root>/checkouts/<name>-<hash>`), and the rule is spelled
once per build system — `nros_build_root` in `scripts/build/build-root.sh`,
`nros_build_paths::build_root` in Rust, `nros_checkout_scoped_root` in cmake.
Python gets no fourth spelling: it RUNS the shell function, so a python gate
looks in exactly the tree the builder writes. `build_root_derivation.sh`
refuses a direct `os.environ` read of the variable anywhere else.
"""

from __future__ import annotations

import subprocess
from pathlib import Path


def build_root(repo: Path | str) -> Path:
    """`nros_build_root` for checkout `repo`: `<repo>/build`, or the
    checkout-scoped `NROS_BUILD_ROOT` when it is set. Raises on failure — a
    caller that silently fell back to `<repo>/build` would be a second
    derivation again."""
    repo = Path(repo).resolve()
    out = subprocess.run(
        [
            "bash",
            "-c",
            '. "$1/scripts/build/build-root.sh" && NROS_REPO_ROOT="$1" nros_build_root',
            "bash",
            str(repo),
        ],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()
    if not out:
        raise RuntimeError(f"nros_build_root printed nothing for {repo}")
    return Path(out)


def build_dir(repo: Path | str, kind: str, *coords: str) -> Path:
    """RFC-0070 R2 — `<root>/<kind>/<coordinate>…`, the `nros_build_dir` shape."""
    if not kind:
        raise ValueError("build_dir: kind is required (RFC-0070 R2)")
    out = build_root(repo) / kind
    for part in coords:
        if part:
            out = out / part
    return out
