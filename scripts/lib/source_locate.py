"""Where a `[source.*]` tree is, from a Python script — RFC-0103 D4/D5.

The one ladder lives in `nros_build_paths::locate`, reached through `nros
locate` (env override, a local edit in the checkout submodule, the store copy
at the pin, the checkout). A store-first row may have NO checkout copy at all —
an agent worktree that never initialised the submodule, or an installed SDK
root — so a script that audits a vendored tree asks here instead of joining
the checkout path itself (`check-store-source-readers` refuses the latter).

The shell twin is `nros_locate_source` in `scripts/build/cargo.sh`; the cmake
twin is `nros_locate_source()` in `cmake/NanoRosLocate.cmake`.

As a script: `python3 scripts/lib/source_locate.py <name> [<marker>]` prints
the located tree and exits 0, or exits 1 when it is provisioned nowhere (or
`<marker>` is absent inside it) — for a `just` recipe that skips.
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def _cli() -> str | None:
    explicit = os.environ.get("NROS_CLI")
    if explicit and os.access(explicit, os.X_OK):
        return explicit
    built = ROOT / "packages/cli/target/release/nros"
    if built.is_file() and os.access(built, os.X_OK):
        return str(built)
    return shutil.which("nros")


def _checkout_dest(name: str) -> Path | None:
    try:
        import tomllib
    except ImportError:  # Python < 3.11
        import tomli as tomllib
    index = tomllib.loads((ROOT / "nros-sdk-index.toml").read_text())
    dest = index.get("source", {}).get(name, {}).get("dest")
    return ROOT / dest if dest else None


def source_dir(name: str) -> Path | None:
    """The located tree of `[source.<name>]`, or None when it is nowhere.

    With no `nros` CLI to ask (a bare checkout before `just setup-cli`), the
    answer is the checkout's copy when it is populated — the ladder's last
    rung, the same fallback the cmake and shell twins take.
    """
    cli = _cli()
    if cli:
        r = subprocess.run(
            [cli, "locate", "--format", "path", "--index", str(ROOT / "nros-sdk-index.toml"), name],
            capture_output=True,
            text=True,
        )
        if r.returncode == 0 and r.stdout.strip():
            return Path(r.stdout.strip())
        return None
    dest = _checkout_dest(name)
    if dest and dest.is_dir() and any(dest.iterdir()):
        return dest
    return None


def located_or_dest(name: str) -> Path:
    """[`source_dir`], else the row's checkout `dest` (read from the index,
    never spelled) — for a script whose own skip logic tests a marker file and
    should report the checkout path when nothing is provisioned."""
    d = source_dir(name)
    if d is not None:
        return d
    dest = _checkout_dest(name)
    if dest is None:
        raise KeyError(f"no [source.{name}] with a dest in nros-sdk-index.toml")
    return dest


def self_test() -> None:
    """A negative control on every run: an unknown row is located nowhere and
    has no dest to fall back to, and a real row's dest is read from the index."""
    assert source_dir("nros-no-such-source-row") is None
    try:
        located_or_dest("nros-no-such-source-row")
    except KeyError:
        pass
    else:
        raise AssertionError("an unknown row produced a path")
    assert _checkout_dest("zenoh-pico") is not None, "the index lost [source.zenoh-pico]"


def main(argv: list[str]) -> int:
    self_test()
    if not argv or len(argv) > 2:
        print("usage: source_locate.py <name> [<marker>]", file=sys.stderr)
        return 2
    d = source_dir(argv[0])
    if d is None or (len(argv) == 2 and not (d / argv[1]).exists()):
        return 1
    print(d)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
