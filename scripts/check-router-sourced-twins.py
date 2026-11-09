#!/usr/bin/env python3
"""The three router launchers agree on what "this ROS prefix is sourced" means.

Issue 2344. Before a launcher starts `rmw_zenohd` it decides whether to run
the router's own prefix `setup.bash` first. Three places make that decision,
in three spellings that no shared helper can reach (two cargo workspaces and a
shell script):

  * scripts/dev/zenohd.sh            `nros_router_env_exec`
  * nros-tests zenohd_router.rs      `unsourced_prefix_setup`
  * nros-cli-core run_exec.rs        `nros run --router auto`

All three used to ask whether AMENT_PREFIX_PATH names the prefix. The runner
image sets AMENT_PREFIX_PATH as a plain ENV with nothing sourced, so all three
skipped setup.bash and the router died on `libament_index_cpp.so`. The test
that matches what running NEEDS is `<prefix>/lib` on LD_LIBRARY_PATH.

This gate holds each twin to that: its decider must test the prefix's lib dir
against LD_LIBRARY_PATH, and must not test the prefix against
AMENT_PREFIX_PATH. A twin that drifts back fails here, by name.

Buildless, milliseconds. The self-test runs on the normal path.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]

# (path, what the decider must say, what it must not say)
TWINS = [
    (
        "scripts/dev/zenohd.sh",
        re.compile(r'":\$\{LD_LIBRARY_PATH:-\}:"\s*!=\s*\*":\$prefix/lib:"\*'),
        re.compile(r'":\$\{AMENT_PREFIX_PATH:-\}:"\s*!=\s*\*":\$prefix:"\*'),
    ),
    (
        "packages/testing/nros-tests/src/fixtures/zenohd_router.rs",
        re.compile(r'let lib = prefix\.join\("lib"\);\s*let sourced = ld_library_path'
                   r'[^;]*split_paths\(v\)\.any\(\|p\| p == lib\)'),
        re.compile(r'ament_prefix_path[^;]*split_paths\(v\)\.any\(\|p\| p == prefix\)'),
    ),
    (
        "packages/cli/nros-cli-core/src/cmd/run_exec.rs",
        re.compile(r'let lib = p\.join\("lib"\);\s*std::env::var_os\("LD_LIBRARY_PATH"\)'
                   r'\s*\.is_some_and\(\|v\| std::env::split_paths\(&v\)\.any\(\|x\| x == lib\)\)'),
        re.compile(r'var_os\("AMENT_PREFIX_PATH"\)\s*\.is_some_and\(\|v\| '
                   r'std::env::split_paths\(&v\)\.any\(\|x\| x == p\)\)'),
    ),
]


def check(texts: dict[str, str]) -> list[str]:
    problems = []
    for path, want, forbid in TWINS:
        text = texts.get(path)
        if text is None:
            problems.append(f"  {path}: missing. A twin that moved must be re-pointed here, "
                            "not dropped (its decider still exists somewhere).")
            continue
        if forbid.search(text):
            problems.append(f"  {path}: decides \"sourced\" by AMENT_PREFIX_PATH naming the "
                            "prefix. The runner image sets that with nothing sourced (issue "
                            "2344); test `<prefix>/lib` on LD_LIBRARY_PATH instead.")
        if not want.search(text):
            problems.append(f"  {path}: no `<prefix>/lib`-on-LD_LIBRARY_PATH test in the "
                            "shape its twins use. Change all three together, and this "
                            "gate's patterns with them.")
    return problems


def self_test() -> None:
    good = {p: (REPO / p).read_text() for p, _, _ in TWINS}
    assert check(good) == [], check(good)
    # Each twin reverted to the AMENT test is caught, by name.
    reverts = {
        "scripts/dev/zenohd.sh": ('":${LD_LIBRARY_PATH:-}:" != *":$prefix/lib:"*',
                                  '":${AMENT_PREFIX_PATH:-}:" != *":$prefix:"*'),
        "packages/testing/nros-tests/src/fixtures/zenohd_router.rs": (
            "ld_library_path.is_some_and(|v| std::env::split_paths(v).any(|p| p == lib))",
            "ament_prefix_path.is_some_and(|v| std::env::split_paths(v).any(|p| p == prefix))"),
        "packages/cli/nros-cli-core/src/cmd/run_exec.rs": (
            'std::env::var_os("LD_LIBRARY_PATH")\n            .is_some_and(|v| std::env::split_paths(&v).any(|x| x == lib))',
            'std::env::var_os("AMENT_PREFIX_PATH")\n            .is_some_and(|v| std::env::split_paths(&v).any(|x| x == p))'),
    }
    for path, (new, old) in reverts.items():
        assert new in good[path], f"self-test: {path} no longer holds the fixed spelling"
        bad = dict(good)
        bad[path] = good[path].replace(new, old)
        got = check(bad)
        assert got and all(path in g for g in got), f"self-test: missed a revert of {path}: {got}"
    # A twin that disappeared is a failure, not a pass.
    gone = dict(good)
    del gone[TWINS[0][0]]
    assert any("missing" in g for g in check(gone)), "self-test: a missing twin passed"


def main() -> int:
    self_test()
    texts = {}
    for path, _, _ in TWINS:
        f = REPO / path
        if f.is_file():
            texts[path] = f.read_text()
    problems = check(texts)
    if problems:
        print("check-router-sourced-twins: the router launchers disagree on \"sourced\":",
              file=sys.stderr)
        print("\n".join(problems), file=sys.stderr)
        return 1
    print(f"check-router-sourced-twins OK -- {len(TWINS)} launchers test <prefix>/lib on "
          "LD_LIBRARY_PATH, none tests AMENT_PREFIX_PATH")
    return 0


if __name__ == "__main__":
    sys.exit(main())
