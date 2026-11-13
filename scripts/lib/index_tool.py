#!/usr/bin/env python3
"""Find or install one `[tool.*]` dist from `nros-sdk-index.toml`, without the CLI.

phase-543 W0. The gate runner runs BEFORE any build, on a checkout that has no
`nros` CLI (issues 0650, 1373), so the tools it needs cannot be provisioned or
located through `nros setup` / `nros locate`. This reads the same index row
those use and nothing else:

    index_tool.py path <tool> <program>     -> <store>/sdk/<tool>/<version>/<root>/bin/<program>
    index_tool.py install <tool> --prefix <dir> [--index <toml>]
                                            download this host's dist, check its
                                            sha256, run its `install` recipe
    index_tool.py extract <tool>...         print those rows as TOML, verbatim
    index_tool.py check-extract <file> <tool>...
                                            fail if <file> is not that output
    index_tool.py version <tool>            the `[tool.<tool>] version`
    index_tool.py board-packages <board>    the `[board.<board>] packages` list,
                                            one per line (issue 2413)
    index_tool.py sdk-plan --index <toml>   one tab-separated line per `[tool.*]`
                                            in <toml>: name, version, this host's
                                            dist url and sha256, the unpacked
                                            `subdir`, and `post_install.run`
                                            (issue 2511: what the zephyr-ci image
                                            bakes, one SDK per Zephyr line)

`extract` writes `config/ci-base-tools.toml`, the rows the ci-base image bakes.
The image COPYs that file rather than the whole index, so an index edit to
another row does not change the image's content tag or rebuild it; the
`ci-base-tools` gate keeps the two in step.

`path` prints the store path whether or not it exists and exits 1 when the
program is absent there, so a caller can name the remedy. It never searches
PATH: a PATH copy is how an unpinned tool wins (on one host `~/.cargo/bin` held
cargo-nextest 0.9.99 ahead of the 0.9.143 pin).

`resolve <tool> <program>` is `path`, then a PATH copy only if it PASSES THE
ROW'S OWN SMOKE PROBE (`smoke = [{ run = "bin/<program> ...", expect }]`): the
ci-base image installs the pinned dist under `/usr/local`, outside any store,
and a version check is what tells that copy from an unpinned one. A row with
no smoke probe for the program never resolves from PATH.

`install` takes only the PREBUILT dist for this host. A tool with no dist for
it is an error naming the host, never a source build: this runs in image
builds and lane preflights, where minutes of compiling would be a surprise.
"""

from __future__ import annotations

import hashlib
import os
import subprocess
import sys
import tempfile
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(Path(__file__).resolve().parent))
import index_packages  # noqa: E402

try:
    import tomllib
except ModuleNotFoundError:  # Python 3.10 (the CI image's ROS Humble base)
    import tomli as tomllib


def load_tool(name: str, index: Path | None = None) -> dict:
    with open(index or ROOT / "nros-sdk-index.toml", "rb") as fh:
        tools = tomllib.load(fh).get("tool", {})
    if name not in tools:
        raise SystemExit(f"index_tool: no [tool.{name}] in nros-sdk-index.toml")
    return tools[name]


def store_root() -> Path:
    # scripts/lib/store-root.sh is the shell spelling of this rule.
    for retired in ("NROS_STORE", "NROS_SDK_STORE"):
        if retired in os.environ:
            raise SystemExit(f"index_tool: ${retired} is retired; set NROS_HOME (RFC-0103 D6)")
    return Path(os.environ.get("NROS_HOME") or Path.home() / ".nros")


def program_path(name: str, program: str, tool: dict | None = None) -> Path:
    tool = tool or load_tool(name)
    prefix = store_root() / "sdk" / name / tool["version"]
    root = index_packages.tool_root(tool)
    return (prefix / root if root else prefix) / "bin" / program


def smoke_ok(tool: dict, program: str, exe: str) -> bool:
    """Does `exe` pass every smoke probe the row states for `program`?"""
    probes = [p for p in tool.get("smoke", [])
              if p.get("run", "").split()[:1] == [f"bin/{program}"]]
    if not probes:
        return False
    for p in probes:
        argv = [exe] + p["run"].split()[1:]
        try:
            out = subprocess.run(argv, capture_output=True, text=True, timeout=30)
        except (OSError, subprocess.TimeoutExpired):
            return False
        if p.get("expect", "") not in out.stdout + out.stderr:
            return False
    return True


def resolve(name: str, program: str, tool: dict | None = None) -> Path | None:
    import shutil
    tool = tool or load_tool(name)
    p = program_path(name, program, tool)
    if os.access(p, os.X_OK):
        return p
    found = shutil.which(program)
    if found and smoke_ok(tool, program, found):
        return Path(found)
    return None


def install(name: str, prefix: Path, tool: dict | None = None) -> None:
    tool = tool or load_tool(name)
    host = index_packages.host_key()
    dist = (tool.get("dist") or {}).get(host)
    if not dist:
        raise SystemExit(f"index_tool: [tool.{name}] has no prebuilt dist for {host}")
    prefix.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix=f"index-tool-{name}-") as tmp:
        archive = Path(tmp) / Path(dist["url"]).name
        with urllib.request.urlopen(dist["url"], timeout=120) as resp, open(archive, "wb") as out:
            while chunk := resp.read(1 << 20):
                out.write(chunk)
        got = hashlib.sha256(archive.read_bytes()).hexdigest()
        if got != dist["sha256"]:
            raise SystemExit(
                f"index_tool: {dist['url']}: sha256 {got} != pinned {dist['sha256']}"
            )
        cmd = dist["install"].replace("{prefix}", str(prefix)).replace("{archive}", str(archive))
        subprocess.run(["bash", "-euo", "pipefail", "-c", cmd], check=True)


def extract(names: list[str]) -> str:
    """The `[tool.<name>]` sections of the index, verbatim, in index order."""
    text = (ROOT / "nros-sdk-index.toml").read_text()
    out, keep = [], False
    for line in text.splitlines():
        if line.startswith("["):
            head = line.strip().strip("[]").strip()
            keep = any(head == f"tool.{n}" or head.startswith(f"tool.{n}.") for n in names)
        elif line.startswith("#") and not keep:
            continue
        if keep:
            out.append(line)
    if not out:
        raise SystemExit(f"index_tool: no rows for {names}")
    header = (
        "# GENERATED by `python3 scripts/lib/index_tool.py extract "
        + " ".join(names)
        + "` from nros-sdk-index.toml.\n# Do not edit: change the index row and regenerate "
        "(gate `ci-base-tools`).\n"
    )
    return header + "\n".join(out).rstrip() + "\n"


def self_test() -> None:
    tool = {"version": "1.2.3", "subdir": "t-{version}", "dist": {}}
    os.environ["NROS_HOME"] = "/h"
    assert program_path("t", "p", tool) == Path("/h/sdk/t/1.2.3/t-1.2.3/bin/p")
    tool.pop("subdir")
    assert program_path("t", "p", tool) == Path("/h/sdk/t/1.2.3/bin/p")
    # A PATH copy counts only when the row's smoke probe passes on it.
    assert smoke_ok({"smoke": [{"run": "bin/echo 1.2.3", "expect": "1.2.3"}]}, "echo", "/bin/echo")
    assert not smoke_ok({"smoke": [{"run": "bin/echo 9.9.9", "expect": "1.2.3"}]}, "echo", "/bin/echo")
    assert not smoke_ok({"smoke": []}, "echo", "/bin/echo"), "no probe must never resolve from PATH"
    try:
        install("t", Path("/nonexistent"), tool)
    except SystemExit as e:
        assert "no prebuilt dist" in str(e), e
    else:
        raise AssertionError("a tool with no dist for this host installed")
    # sdk-plan: one line per row, `{version}` expanded; a row it cannot bake
    # (no post_install) is refused rather than skipped.
    host = index_packages.host_key()
    with tempfile.TemporaryDirectory(prefix="index-tool-selftest-") as tmp:
        idx = Path(tmp) / "i.toml"
        idx.write_text(
            '[tool.s]\nversion = "9.8"\nsubdir = "s-{version}"\n'
            'post_install.run = "./setup.sh -h"\n'
            f'dist.{host} = {{ url = "https://x/s.tar.xz", sha256 = "ab" }}\n'
        )
        assert sdk_plan(idx) == ["s\t9.8\thttps://x/s.tar.xz\tab\ts-9.8\t./setup.sh -h"]
        idx.write_text(idx.read_text().replace('post_install.run = "./setup.sh -h"\n', ""))
        try:
            sdk_plan(idx)
        except SystemExit as e:
            assert "post_install.run" in str(e), e
        else:
            raise AssertionError("sdk-plan baked a row with no post_install")


def sdk_plan(index: Path) -> list[str]:
    """Issue 2511 — every tool row in `index`, as the image's bake loop reads it.

    The zephyr-ci image used to name its SDK versions in Dockerfile `ARG`s,
    and they drifted from the index: it baked 0.17.4 for the 3.7 line, which
    pins 0.16.9 and refuses any other version. Reading the row makes the pin
    the only place a version is stated.
    """
    with open(index, "rb") as fh:
        tools = tomllib.load(fh).get("tool", {})
    host = index_packages.host_key()
    lines = []
    for name, tool in tools.items():
        dist = (tool.get("dist") or {}).get(host)
        if not dist:
            raise SystemExit(f"index_tool: [tool.{name}] has no prebuilt dist for {host}")
        subdir = tool.get("subdir", "").replace("{version}", tool["version"])
        run = (tool.get("post_install") or {}).get("run", "")
        if not subdir or not run:
            raise SystemExit(
                f"index_tool: [tool.{name}] needs `subdir` and `post_install.run` "
                "to be baked by sdk-plan"
            )
        fields = (name, tool["version"], dist["url"], dist["sha256"], subdir, run)
        if any("\t" in f or "\n" in f for f in fields):
            raise SystemExit(f"index_tool: [tool.{name}] has a tab or newline in a field")
        lines.append("\t".join(fields))
    if not lines:
        raise SystemExit(f"index_tool: no [tool.*] rows in {index}")
    return lines


def board_packages(board: str, index: Path | None = None) -> list[str] | None:
    """`[board.<board>] packages` from the index, or None for an unknown board.

    Issue 2413: what a template-copy-out unit needs beyond the gate itself is a
    fact of the board it builds for, and the index already states it here
    (`nros setup <board>` provisions exactly this list).
    """
    with open(index or ROOT / "nros-sdk-index.toml", "rb") as fh:
        boards = tomllib.load(fh).get("board", {})
    row = boards.get(board)
    return None if row is None else list(row.get("packages", []))


def main(argv: list[str]) -> int:
    if argv[:1] == ["--self-test"]:
        self_test()
        print("index_tool self-test OK")
        return 0
    if len(argv) == 3 and argv[0] == "path":
        p = program_path(argv[1], argv[2])
        print(p)
        return 0 if os.access(p, os.X_OK) else 1
    if len(argv) == 3 and argv[0] == "resolve":
        r = resolve(argv[1], argv[2])
        print(r if r else program_path(argv[1], argv[2]))
        return 0 if r else 1
    if len(argv) in (4, 6) and argv[0] == "install" and argv[2] == "--prefix":
        index = Path(argv[5]) if len(argv) == 6 and argv[4] == "--index" else None
        install(argv[1], Path(argv[3]), load_tool(argv[1], index))
        return 0
    if len(argv) == 2 and argv[0] == "version":
        # issue 2389 — a caller that needs a tool's VERSION (a store path, a log
        # line) reads it here, never restates it: `scripts/zephyr/setup.sh`
        # carried its own copy of the SDK version and kept 0.16.8 after the
        # index moved to 0.16.9.
        print(load_tool(argv[1])["version"])
        return 0
    if len(argv) == 3 and argv[0] == "sdk-plan" and argv[1] == "--index":
        print("\n".join(sdk_plan(Path(argv[2]))))
        return 0
    if len(argv) == 2 and argv[0] == "board-packages":
        pkgs = board_packages(argv[1])
        if pkgs is None:
            print(f"index_tool: no [board.{argv[1]}] in nros-sdk-index.toml", file=sys.stderr)
            return 1
        for p in pkgs:
            print(p)
        return 0
    if len(argv) >= 2 and argv[0] == "extract":
        sys.stdout.write(extract(argv[1:]))
        return 0
    if len(argv) >= 3 and argv[0] == "check-extract":
        self_test()  # the gate's own negative controls, on its normal path
        want = extract(argv[2:])
        have = Path(argv[1]).read_text() if Path(argv[1]).is_file() else ""
        if have != want:
            print(f"{argv[1]} is stale against nros-sdk-index.toml; regenerate:\n"
                  f"  python3 scripts/lib/index_tool.py extract {' '.join(argv[2:])} > {argv[1]}",
                  file=sys.stderr)
            return 1
        print(f"OK: {argv[1]} matches the index rows {', '.join(argv[2:])}")
        return 0
    print(__doc__.split("\n\n")[2], file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
