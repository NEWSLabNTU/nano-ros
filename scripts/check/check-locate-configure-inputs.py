#!/usr/bin/env python3
"""Issue 3019 — a cmake configure that located a source tree re-configures
when what the locate ladder read moves.

`cmake/NanoRosLocate.cmake` asks the ladder once per configure
(`nros locate --all --format cmake`). The CLI also states the files the answers
depend on (`NROS_LOCATE_INPUTS`: a project's system.toml, each store row
directory, each submodule checkout and its git HEAD), and ONE helper,
`_nros_locate_register_inputs`, adds them to CMAKE_CONFIGURE_DEPENDS for every
`nros_locate_var` / `nros_locate_source` caller. This gate holds that shape:

  1. nothing outside NanoRosLocate.cmake asks the source ladder from cmake
     (`<nros> locate --all` or `<nros> locate <source>`), so no caller can
     bypass the helper and keep an answer without its inputs;
  2. `_nros_locate_load` hands the CLI's output to the helper;
  3. self-test: a real cmake + ninja configure of a project that includes the
     module, with a stand-in CLI whose answer and inputs the test controls.
     A no-change build must not re-configure; moving each stated input must;
     and a stated input that does not exist is not registered, so a missing
     file cannot make every build re-configure.

What the CLI states for each rung is the Rust side's test
(`locate::watch_inputs`); what no file states (an environment variable, a
checkout's uncommitted edits) is `nros build`'s re-ask
(`locate_answers_current`).

  python3 scripts/check/check-locate-configure-inputs.py
"""

from __future__ import annotations

import os
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MODULE = "cmake/NanoRosLocate.cmake"
# `<nros> locate` followed by --all or a source NAME. `locate configured`,
# `locate artifact` and `locate tool` are other questions with their own stamps.
ASK = re.compile(r'"\$\{[A-Za-z0-9_]+\}"\s+locate\s+(?!configured\b|artifact\b|tool\b)\S')


def tracked_cmake() -> list[str]:
    out = subprocess.run(
        ["git", "-C", str(ROOT), "ls-files", "--", "*.cmake", "*CMakeLists.txt"],
        check=True, capture_output=True, text=True).stdout
    return [p for p in out.splitlines() if p]


def scan(files: dict[str, str]) -> list[str]:
    """Errors for a {path: text} set (the self-test feeds it fixtures)."""
    errs = []
    for path, text in files.items():
        if path == MODULE:
            continue
        for n, line in enumerate(text.splitlines(), 1):
            if ASK.search(line):
                errs.append(f"{path}:{n}: asks the source ladder outside {MODULE}; "
                            "call nros_locate_var() / nros_locate_source() so its "
                            "inputs are registered")
    module = files.get(MODULE, "")
    body = module.split("function(_nros_locate_load)", 1)[-1].split("endfunction()", 1)[0]
    if "_nros_locate_register_inputs(" not in body:
        errs.append(f"{MODULE}: _nros_locate_load no longer calls "
                    "_nros_locate_register_inputs, so no located answer has its inputs")
    return errs


def static_self_test() -> None:
    good = {MODULE: "function(_nros_locate_load)\n  _nros_locate_register_inputs(a b)\nendfunction()\n"}
    assert scan(good) == [], scan(good)
    bad = dict(good, **{"cmake/x.cmake": 'execute_process(COMMAND "${_nros}" locate --all)\n'})
    assert len(scan(bad)) == 1, scan(bad)
    named = dict(good, **{"cmake/y.cmake": 'execute_process(COMMAND "${n}" locate freertos-kernel)\n'})
    assert len(scan(named)) == 1, scan(named)
    other = dict(good, **{"cmake/z.cmake": 'execute_process(COMMAND "${_nros}" locate configured --project p)\n'})
    assert scan(other) == [], scan(other)
    lost = {MODULE: "function(_nros_locate_load)\nendfunction()\n"}
    assert len(scan(lost)) == 1, scan(lost)


def run(cmd: list[str], cwd: Path, env: dict[str, str]) -> str:
    p = subprocess.run(cmd, cwd=cwd, env=env, capture_output=True, text=True)
    if p.returncode != 0:
        raise SystemExit(f"self-test: {' '.join(cmd)} failed ({p.returncode}):\n{p.stdout}\n{p.stderr}")
    return p.stdout + p.stderr


def cmake_self_test() -> None:
    for tool in ("cmake", "ninja"):
        if not shutil.which(tool):
            raise SystemExit(f"check-locate-configure-inputs: self-test needs `{tool}` on PATH")
    with tempfile.TemporaryDirectory() as t:
        t = Path(t)
        tree_a, tree_b = t / "tree-a", t / "tree-b"
        tree_a.mkdir()
        tree_b.mkdir()
        head = t / "HEAD"            # stands for a submodule's git HEAD
        store = t / "store-row"      # stands for <store>/sources/<name>
        head.write_text("a\n")
        store.mkdir()
        answer = t / "answer"        # which tree the stand-in CLI locates
        answer.write_text(str(tree_a))
        fake = t / "nros"
        fake.write_text(
            "#!/bin/sh\n"
            f'echo "set(NROS_LOCATE_FREERTOS_KERNEL \\"$(cat {answer})\\")"\n'
            f'echo "set(NROS_LOCATE_ENV_FREERTOS_DIR \\"$(cat {answer})\\")"\n'
            f'echo "set(NROS_LOCATE_INPUTS \\"{head};{store};{t}/absent\\")"\n'
            'echo "set(NROS_LOCATE_PROJECT \\"\\")"\n')
        fake.chmod(0o755)
        src, bld = t / "src", t / "build"
        src.mkdir()
        (src / "CMakeLists.txt").write_text(
            "cmake_minimum_required(VERSION 3.22)\n"
            "project(locate_inputs_probe NONE)\n"
            f'include("{ROOT / MODULE}")\n'
            "nros_locate_var(FREERTOS_DIR)\n"
            'file(WRITE "${CMAKE_BINARY_DIR}/located.txt" "${FREERTOS_DIR}")\n')
        env = dict(os.environ, NROS_CLI=str(fake))
        env.pop("FREERTOS_DIR", None)
        run(["cmake", "-S", str(src), "-B", str(bld), "-G", "Ninja"], t, env)
        located = lambda: (bld / "located.txt").read_text()
        assert located() == str(tree_a), located()
        stamp_file = bld / "nros-locate.stamp"
        assert stamp_file.is_file(), "the configure wrote no nros-locate.stamp for `nros build` to re-check"
        stamp = stamp_file.read_text()
        assert f"nros={fake}" in stamp and "\n---\n" in stamp, stamp

        def build() -> bool:
            out = run(["cmake", "--build", str(bld)], t, env)
            return "Re-running CMake" in out

        assert not build(), "a no-change build re-configured"
        assert not build(), "a second no-change build re-configured (a missing input registered?)"
        for name, move in (("submodule HEAD", lambda: head.write_text("b\n")),
                           ("store row dir", lambda: (store / "11.2.0+c3657012").mkdir())):
            time.sleep(1.1)  # mtime granularity
            answer.write_text(str(tree_b))
            move()
            assert build(), f"moving the {name} did not re-configure"
            assert located() == str(tree_b), f"after the {name} moved, still located {located()}"
            tree_a, tree_b = tree_b, tree_a


def main() -> int:
    static_self_test()
    try:
        cmake_self_test()
    except AssertionError as e:
        print(f"check-locate-configure-inputs self-test: FAILED — {e}", file=sys.stderr)
        return 1
    print("check-locate-configure-inputs self-test: OK (5 scan cases, cmake re-configure on 2 input kinds)")
    files = {p: (ROOT / p).read_text(errors="replace") for p in tracked_cmake()}
    errs = scan(files)
    if errs:
        print("check-locate-configure-inputs: FAILED", file=sys.stderr)
        for e in errs:
            print(f"  {e}", file=sys.stderr)
        return 1
    print(f"check-locate-configure-inputs: OK ({len(files)} cmake file(s); the ladder is asked only through {MODULE})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
