#!/usr/bin/env python3
"""Refuse a Zephyr workspace that lacks a module this checkout's manifest lists.

Issue 2607 (phase-524).

WHAT IT CATCHES
---------------
A west workspace does not read the checkout's `west.yml`. It reads a COPY in
its own manifest project (`[manifest] path` / `file` in `.west/config`), and
`west update` fetches only what that copy names. So when the checkout's
manifest gains a module (phase-524 W2 put `hal_stm32` back for the
Nucleo-F767ZI twin), a workspace provisioned before it builds every OLD board
and fails the new one deep in Zephyr's devicetree step:

    nucleo_f767zi.dts:9:10: fatal error: st/f7/stm32f767zitx-pinctrl.dtsi:
    No such file or directory

That line names neither the module, nor the manifest, nor the remedy, and it
reads like a broken board port. Two workspace shapes produce it, and only one
of them is visible to a directory check:

  * NOT LISTED — the workspace's manifest copy predates the module, so
    `west list` does not report it and Zephyr's module discovery never adds
    it. The module's DIRECTORY may still be present (measured: the main
    checkout's workspace had `modules/hal/stm32` at the right revision), so
    "is the directory there" passes this workspace.
  * NOT FETCHED — the copy lists it, but `west update` never ran after the
    copy changed, so the project was never cloned.

WHAT IT ASKS
------------
The modules WEST REPORTS for the workspace (`west list`, which reads the
workspace's own copy, plus west's own `{cloned}` answer) against the modules
`[zephyr_module.*]` in `nros-sdk-index.toml` declares for that Zephyr line.
The index, not the YAML, is the module SSoT (phase-447 F1), and
`check-zephyr-module-allowlist` already keeps the committed manifests equal
to it — so there is one source, read by the same parser that gate uses.

The line is the one the WORKSPACE was provisioned for (its `[manifest] file`:
`west.yml` is 3.7, `west-4.4.yml` is 4.4), because that is the manifest the
workspace's build will read.

Additive staleness only: a module the workspace has and the index no longer
lists is harmless (issue 1282 — `west update` never prunes).

The remedy is `just zephyr setup`, which since issue 2249 replaces the
workspace's manifest copy when it differs from the checkout's and runs
`west update`.

WHO IS REFUSED
--------------
With `--boards b1,b2,...` (the boards a build is about to configure) a missing
module REFUSES only when one of those boards needs it, read from the module's
`needed_by` in the index (board names without their `/qualifiers`). Any other
missing module is a WARNING and the exit is 0: a stale workspace still builds
every board it already built, so refusing those builds would turn green
lanes red over a module none of them reads. `needed_by` is authored, so a
board it omits is warned about rather than refused; the warning names the
module and the remedy, so the devicetree error that may follow is explained
by the lines above it. Without `--boards` (`just zephyr doctor`) every missing
module refuses.

Exit codes: 0 every module present (or only warnings); 1 a module refused
(each one named); 78 nothing to check (no workspace, no `.west/config`, no
`west`) — a caller decides whether that is a skip, as
`check-zephyr-workspace-checkout.sh` does with the same code.

Run:  scripts/zephyr/check-workspace-modules.py <workspace> [--boards B,...] [--quiet]
      scripts/zephyr/check-workspace-modules.py --self-test
"""

from __future__ import annotations

import configparser
import importlib.util
import re
import shutil
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
INDEX = REPO / "nros-sdk-index.toml"
NOTHING_TO_CHECK = 78


def _allowlist_gate():
    """The module-allowlist gate, for its index parser and line table.

    Imported rather than copied: two parsers of `[zephyr_module.*]` would be
    the drift that gate exists to stop, one level up.
    """
    path = REPO / "scripts" / "check-zephyr-module-allowlist.py"
    spec = importlib.util.spec_from_file_location("zephyr_module_allowlist", path)
    mod = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(mod)
    return mod


def parse_needed_by(text: str) -> dict:
    """`[zephyr_module.<name>]` -> its `needed_by` boards.

    Same fixed authoring shape the allowlist gate's `lines` parser reads.
    """
    out: dict = {}
    current = None
    for raw in text.splitlines():
        line = raw.strip()
        if line.startswith("[") and line.endswith("]"):
            section = line[1:-1]
            current = (
                section[len("zephyr_module.") :]
                if section.startswith("zephyr_module.")
                else None
            )
            if current is not None:
                out[current] = []
            continue
        if current is None:
            continue
        m = re.match(r"needed_by\s*=\s*\[(.*)\]\s*$", line)
        if m:
            out[current] = re.findall(r'"([^"]+)"', m.group(1))
    return out


def split_refused(problems: list, needed_by: dict, boards) -> tuple:
    """(refused, warned): refused = a problem a board in `boards` needs.

    `boards` None means every problem refuses (the doctor's question).
    """
    if boards is None:
        return problems, []
    bases = {b.split("/", 1)[0] for b in boards if b}
    refused = [p for p in problems if bases & set(needed_by.get(p[0], []))]
    warned = [p for p in problems if p not in refused]
    return refused, warned


def expected_modules(index_text: str, line: str, gate) -> list:
    """The modules the index declares for `line`, sorted."""
    return sorted(
        n for n, lines in gate.parse_index_modules(index_text).items() if line in lines
    )


def parse_west_list(text: str) -> dict:
    """`west list -f "{name} {cloned}"` output -> {name: cloned?}."""
    out = {}
    for raw in text.splitlines():
        parts = raw.split()
        if len(parts) == 2:
            out[parts[0]] = parts[1] == "cloned"
    return out


def diagnose(expected: list, listed: dict) -> list:
    """(module, problem) for each expected module the workspace cannot build with."""
    problems = []
    for name in expected:
        if name not in listed:
            problems.append((name, "not listed by the workspace's west manifest copy"))
        elif not listed[name]:
            problems.append((name, "listed but never fetched"))
    return problems


def workspace_manifest(ws: Path):
    """(manifest project path, manifest file) from `<ws>/.west/config`, or None."""
    cfg = ws / ".west" / "config"
    if not cfg.is_file():
        return None
    parser = configparser.ConfigParser()
    try:
        parser.read(cfg)
    except configparser.Error:
        return None
    if not parser.has_section("manifest"):
        return None
    return (
        parser.get("manifest", "path", fallback=""),
        parser.get("manifest", "file", fallback="west.yml"),
    )


def check(ws: Path, quiet: bool, boards=None) -> int:
    manifest = workspace_manifest(ws)
    if manifest is None:
        if not quiet:
            print(
                f"check-workspace-modules: {ws} has no west config, nothing to check",
                file=sys.stderr,
            )
        return NOTHING_TO_CHECK
    gate = _allowlist_gate()
    manifest_to_line = {v: k for k, v in gate.LINE_TO_MANIFEST.items()}
    line = manifest_to_line.get(manifest[1])
    if line is None:
        print(
            f"check-workspace-modules: {ws} reads manifest `{manifest[1]}`, which is not a "
            f"Zephyr line this checkout knows ({', '.join(sorted(manifest_to_line))})",
            file=sys.stderr,
        )
        return 1
    west = shutil.which("west")
    if west is None:
        if not quiet:
            print(
                "check-workspace-modules: no `west` on PATH, nothing checked",
                file=sys.stderr,
            )
        return NOTHING_TO_CHECK
    proc = subprocess.run(
        [west, "list", "-f", "{name} {cloned}"],
        check=False,
        cwd=ws,
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0:
        print(
            f"check-workspace-modules: `west list` failed in {ws}:\n{proc.stderr.strip()}",
            file=sys.stderr,
        )
        return 1
    index_text = INDEX.read_text(encoding="utf-8")
    expected = expected_modules(index_text, line, gate)
    problems = diagnose(expected, parse_west_list(proc.stdout))
    if not problems:
        if not quiet:
            print(
                f"Zephyr workspace has all {len(expected)} module(s) the {line} manifest lists"
            )
        return 0
    needed_by = parse_needed_by(index_text)
    refused, warned = split_refused(problems, needed_by, boards)
    head = "ERROR" if refused else "WARNING"
    print(
        f"{head}: the Zephyr workspace at {ws} is missing module(s) this checkout's "
        f"{manifest[1]} lists:",
        file=sys.stderr,
    )
    for name, problem in refused:
        print(
            f"  {name}: {problem} (needed by {', '.join(needed_by.get(name, []))}, which this build targets)",
            file=sys.stderr,
        )
    for name, problem in warned:
        print(f"  {name}: {problem}", file=sys.stderr)
    if refused:
        print(
            "A board in this build needs it, and would fail deep in configure, often as a\n"
            "devicetree 'No such file or directory'.",
            file=sys.stderr,
        )
    else:
        print(
            "A board that needs one fails deep in configure, often as a devicetree\n"
            "'No such file or directory'.",
            file=sys.stderr,
        )
    print(
        "Run `just zephyr setup` to bring the workspace up to this checkout's manifest.",
        file=sys.stderr,
    )
    return 1 if refused else 0


def self_test() -> int:
    gate = _allowlist_gate()
    index = (
        '[zephyr_module.cmsis]\nlines = ["3.7", "4.4"]\n'
        '[zephyr_module.hal_stm32]\nlines = ["3.7"]\n'
        '[zephyr_module.cmsis_6]\nlines = ["4.4"]\n'
        '[board.x]\nlines = ["3.7"]\n'
    )
    failures = 0

    def expect(what, got, want):
        nonlocal failures
        if got != want:
            print(f"  FAIL: {what}: got {got!r}, want {want!r}")
            failures += 1
        else:
            print(f"  ok    {what}")

    expect(
        "the 3.7 set comes from the index",
        expected_modules(index, "3.7", gate),
        ["cmsis", "hal_stm32"],
    )
    expect(
        "the 4.4 set comes from the index",
        expected_modules(index, "4.4", gate),
        ["cmsis", "cmsis_6"],
    )
    listed = parse_west_list(
        "manifest cloned\nzephyr cloned\ncmsis cloned\nhal_stm32 cloned\n"
    )
    expect("a complete workspace passes", diagnose(["cmsis", "hal_stm32"], listed), [])
    # The main-checkout shape: the directory exists, the copy omits it.
    listed = parse_west_list("manifest cloned\nzephyr cloned\ncmsis cloned\n")
    expect(
        "a module the copy does not list is refused",
        diagnose(["cmsis", "hal_stm32"], listed),
        [("hal_stm32", "not listed by the workspace's west manifest copy")],
    )
    listed = parse_west_list("cmsis cloned\nhal_stm32 not-cloned\n")
    expect(
        "a listed but unfetched module is refused",
        diagnose(["cmsis", "hal_stm32"], listed),
        [("hal_stm32", "listed but never fetched")],
    )
    listed = parse_west_list("cmsis cloned\nhal_stm32 cloned\nhal_nxp cloned\n")
    expect("an extra module is harmless", diagnose(["cmsis", "hal_stm32"], listed), [])
    expect(
        "needed_by is read per module",
        parse_needed_by('[zephyr_module.hal_stm32]\nneeded_by = ["nucleo_f767zi"]\n'),
        {"hal_stm32": ["nucleo_f767zi"]},
    )
    stm = [("hal_stm32", "listed but never fetched")]
    nb = {"hal_stm32": ["nucleo_f767zi"]}
    expect(
        "a build of the board that needs it is refused",
        split_refused(stm, nb, ["nucleo_f767zi"]),
        (stm, []),
    )
    expect(
        "a qualified board name still matches",
        split_refused(stm, nb, ["nucleo_f767zi/stm32f767xx"]),
        (stm, []),
    )
    expect(
        "a build of other boards is only warned",
        split_refused(stm, nb, ["native_sim/native/64", "mps2_an385"]),
        ([], stm),
    )
    expect(
        "no board list (the doctor) refuses all",
        split_refused(stm, nb, None),
        (stm, []),
    )
    real = gate.parse_index_modules(INDEX.read_text(encoding="utf-8"))
    expect(
        "the real index declares a 3.7 module set",
        bool(expected_modules(INDEX.read_text(), "3.7", gate)),
        True,
    )
    expect("the real index declares hal_stm32", "hal_stm32" in real, True)
    expect(
        "the real index says nucleo_f767zi needs hal_stm32",
        "nucleo_f767zi"
        in parse_needed_by(INDEX.read_text(encoding="utf-8")).get("hal_stm32", []),
        True,
    )
    if failures:
        print(f"{failures} check(s) failed")
        return 1
    print("all checks passed")
    return 0


def main(argv: list) -> int:
    if argv[1:2] == ["--self-test"]:
        return self_test()
    args = [a for a in argv[1:] if a != "--quiet"]
    boards = None
    if "--boards" in args:
        i = args.index("--boards")
        if i + 1 >= len(args):
            print("--boards needs a comma-separated list", file=sys.stderr)
            return 2
        boards = [b for b in args[i + 1].split(",") if b]
        del args[i : i + 2]
    if len(args) != 1:
        print(__doc__.strip().splitlines()[-2].strip(), file=sys.stderr)
        return 2
    ws = Path(args[0])
    if not (ws / "zephyr").is_dir():
        if "--quiet" not in argv:
            print(
                f"check-workspace-modules: no Zephyr workspace at {ws}, nothing to check",
                file=sys.stderr,
            )
        return NOTHING_TO_CHECK
    return check(ws.resolve(), "--quiet" in argv, boards)


if __name__ == "__main__":
    sys.exit(main(sys.argv))
