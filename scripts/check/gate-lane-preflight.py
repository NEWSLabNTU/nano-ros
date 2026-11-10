#!/usr/bin/env python3
"""Is every capability a gate lane CLAIMS actually present? (phase-543 W5)

A lane's `claims` in `.config/gate-lanes.toml` put the gates that need those
capabilities in its scope. A claimed capability that is missing turns each of
those gates red, one at a time, each with its own message. This asks first and
names every missing claim at once, before any gate runs. Where a probe has an
existing check (`_require-worktree-provisioning`, this checkout's CLI), it
calls that check rather than spell it again.

A probe is deliberately coarse: "is it there at all", in a few milliseconds.
The precise question stays with the gate, whose skip under the harness is a red
(`NROS_CHECK_SKIP_STRICT=1`). A coarse probe that passes over a partial
provision is caught there. A probe that fails is never wrong about absence.

  gate-lane-preflight.py <lane>     # exit 1 naming every missing claim
  gate-lane-preflight.py --check    # probes cover the capabilities (no host)
"""

from __future__ import annotations

import os
import shutil
import subprocess
import sys
from pathlib import Path

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib

ROOT = Path(__file__).resolve().parents[2]
LANES = ROOT / ".config" / "gate-lanes.toml"


def on_path(*tools: str) -> str | None:
    if any(shutil.which(t) for t in tools):
        return None
    return f"none of {', '.join(tools)} is on PATH"


def all_on_path(*tools: str) -> str | None:
    missing = [t for t in tools if not shutil.which(t)]
    return None if not missing else f"not on PATH: {', '.join(missing)}"


def env_dir(var: str) -> str | None:
    v = os.environ.get(var, "")
    if v and Path(v).is_dir():
        return None
    return f"${var} is {'unset' if not v else f'{v!r}, not a directory'}"


def probe_submodules(tiers: list[str]) -> str | None:
    # The one provisioning check `ci gate` already runs (store-first sources
    # included, RFC-0103 D5), narrowed to the rows the lane's tiers read.
    # Its binary lines are a different claim (`cli`), so they are not read.
    env = dict(os.environ, NROS_PROVISION_TIERS=" ".join(tiers))
    r = subprocess.run(["just", "_require-worktree-provisioning"], cwd=ROOT,
                       capture_output=True, text=True, env=env)
    lines = [l.strip() for l in r.stderr.splitlines()
             if l.strip().startswith(("submodule", "NOT A"))]
    return "; ".join(lines) if lines else None


def probe_arm_gcc_any() -> str | None:
    # ANY arm-none-eabi-g++, pinned or not, in the order
    # check-px4-uorb-nuttx-compile resolves it (issue 2419).
    prefix = os.environ.get("NROS_ARM_NONE_EABI_PREFIX")
    if prefix:
        return None if shutil.which(prefix + "-g++") else f"{prefix}-g++ is not executable"
    return on_path("arm-none-eabi-g++")


def probe_cli() -> str | None:
    # This checkout's CLI, never PATH or the store (issue 1850).
    r = subprocess.run(["bash", "-c", "source scripts/build/cargo.sh && NROS_CLI= nros_cli_bin --checkout-only"],
                       cwd=ROOT, capture_output=True, text=True)
    return None if r.returncode == 0 else "this checkout's `nros` is not built (just setup-cli)"


def probe_git_history() -> str | None:
    r = subprocess.run(["git", "rev-parse", "--is-shallow-repository"], cwd=ROOT,
                       capture_output=True, text=True)
    if r.returncode != 0:
        return "not a git checkout"
    return "the clone is shallow" if r.stdout.strip() == "true" else None


def probe_sources() -> str | None:
    d = Path(os.environ.get("NROS_HOME") or Path.home() / ".nros") / "sources"
    if d.is_dir() and any(d.iterdir()):
        return None
    return f"no provisioned source clone under {d} (nros setup --source)"


def probe_zephyr_tree() -> str | None:
    inc = ROOT / "zephyr-workspace" / "zephyr" / "lib" / "cpp" / "minimal" / "include"
    return None if inc.is_dir() else f"{inc.relative_to(ROOT)} is absent"


def probe_zephyr() -> str | None:
    r = subprocess.run(["bash", "scripts/lib/zephyr-workspace.sh", "--absolute", "resolve"],
                       cwd=ROOT, capture_output=True, text=True)
    ws = r.stdout.strip()
    # A west workspace, not a source tree (issue 2308). The resolver answers
    # that itself now, so a resolved candidate is one; `.west/` is asked here
    # too only for `$NROS_ZEPHYR_WORKSPACE`, which the resolver returns
    # untested because it is also the install target.
    if r.returncode == 0 and ws and (Path(ws) / ".west").is_dir():
        return None
    return "no west workspace resolves (`.west/` absent)"


def probe_west() -> str | None:
    r = subprocess.run(["bash", "-c", 'source scripts/build/zephyr-python.sh && py="$(NROS_PYTHON= nros_zephyr_python)" && [ -n "$py" ] && "$py" -c "import west"'],
                       cwd=ROOT, capture_output=True, text=True)
    return None if r.returncode == 0 else "no interpreter that imports west"


def probe_build_tree() -> str | None:
    stamp = ROOT / os.environ.get("NROS_FIXTURE_STAMP", "target/nextest/.fixtures-built")
    return None if stamp.exists() else "no fixture build stamp (just build-test-fixtures)"


def probe_nextest() -> str | None:
    """The harness's own runner, at the index's pin. Not a capability a gate
    needs: every lane needs it, so it is asked first, with the same remedy."""
    r = subprocess.run(["python3", "scripts/lib/index_tool.py", "resolve", "cargo-nextest",
                        "cargo-nextest"], cwd=ROOT, capture_output=True, text=True)
    return None if r.returncode == 0 else "the pinned cargo-nextest is not installed"


# What fixes each missing claim. A refusal names the fix, never only the gap:
# a fresh agent worktree is how most sessions work, and `check fast` is what
# its pre-push hook runs.
REMEDY = {
    "cli": "just setup-cli",
    "submodules": "just setup-worktree",
    "git-history": "git fetch --unshallow",
    "sources": "nros setup --source <name> (the index's [source.*] rows)",
    "ros2": "source /opt/ros/<distro>/setup.bash (or ./activate.sh with ROS installed)",
    "colcon": "install colcon (python3 -m pip install colcon-common-extensions)",
    "bindgen": "just setup linux",
    "cmake": "install cmake and ninja",
    "cc": "install a C/C++ compiler",
    "arm-gcc-any": "any arm-none-eabi-g++ on PATH works: the distro package (gcc-arm-none-eabi, as in the CI image) or nros setup --tool arm-none-eabi-gcc then re-source activate.sh; or set NROS_ARM_NONE_EABI_PREFIX",
    "zephyr-tree": "the sparse header clone in gate.yml's fast job, or a west workspace",
    "zephyr": "just zephyr setup",
    "nuttx": "nros setup --source nuttx",
    "fsp": "provision NV_SPE_FSP_DIR (not on any runner)",
    "west": "just zephyr setup",
    "build-tree": "just build-test-fixtures",
    "nextest": "just setup-worktree",
}


PROBES = {
    "cli": probe_cli,
    "cmake": lambda: all_on_path("cmake", "ninja"),
    "cc": lambda: on_path("cc", "gcc", "clang") or all_on_path("ar", "nm", "objcopy"),
    "arm-gcc-any": probe_arm_gcc_any,
    "bindgen": lambda: on_path("bindgen"),
    "ros2": lambda: None if os.environ.get("AMENT_PREFIX_PATH") else "AMENT_PREFIX_PATH is unset",
    "colcon": lambda: on_path("colcon"),
    "sources": probe_sources,
    "submodules": None,  # needs the lane's tiers: probed in main()
    "git-history": probe_git_history,
    "zephyr-tree": probe_zephyr_tree,
    "zephyr": probe_zephyr,
    "nuttx": lambda: env_dir("NUTTX_DIR"),
    "fsp": lambda: env_dir("NV_SPE_FSP_DIR"),
    "west": probe_west,
    "build-tree": probe_build_tree,
}


def lane_claims(cfg: dict, lane: str) -> list[str]:
    lanes = cfg.get("lane", {})
    if lane not in lanes:
        raise SystemExit(f"gate-lane-preflight: no lane `{lane}` in {LANES.relative_to(ROOT)} "
                         f"(lanes: {', '.join(sorted(lanes))})")
    return list(lanes[lane]["claims"])


def coverage(cfg: dict) -> list[str]:
    """Every capability has a probe and a remedy, and every probe a capability."""
    caps = set(cfg.get("capabilities", {}))
    return ([f"capability `{c}` has no probe here" for c in sorted(caps - PROBES.keys())]
            + [f"probe `{p}` names no capability" for p in sorted(PROBES.keys() - caps)]
            + [f"capability `{c}` has no remedy here" for c in sorted(caps - REMEDY.keys())])


def self_test() -> None:
    cfg = {"capabilities": {k: "" for k in PROBES}, "lane": {"x": {"claims": ["cmake"]}}}
    assert coverage(cfg) == [], coverage(cfg)
    cfg["capabilities"]["new"] = ""
    assert coverage(cfg) == ["capability `new` has no probe here",
                             "capability `new` has no remedy here"], coverage(cfg)
    assert lane_claims(cfg, "x") == ["cmake"]
    assert on_path("nros-no-such-tool-543") is not None
    assert env_dir("NROS_NO_SUCH_VAR_543") == "$NROS_NO_SUCH_VAR_543 is unset"


def main(argv: list[str]) -> int:
    self_test()
    if not argv:
        print(__doc__, file=sys.stderr)
        return 2
    cfg = tomllib.loads(LANES.read_text())
    problems = coverage(cfg)
    if problems:
        print("gate-lane-preflight: probes and capabilities disagree:", file=sys.stderr)
        print("\n".join(f"  {p}" for p in problems), file=sys.stderr)
        return 1
    if argv[0] == "--check":
        # The gate (`check gate-lane-preflight`): every capability has a probe
        # and every lane claims only listed capabilities. Asks no host.
        bad = [f"lane `{n}` claims `{c}`, which [capabilities] does not list"
               for n, l in sorted(cfg.get("lane", {}).items())
               for c in l["claims"] if c not in PROBES]
        if bad:
            print("\n".join(bad), file=sys.stderr)
            return 1
        print(f"gate-lane-preflight OK — {len(PROBES)} capabilities probed, "
              f"{len(cfg.get('lane', {}))} lanes")
        return 0
    lane = argv[0]
    claims = lane_claims(cfg, lane)
    tiers = list(cfg["lane"][lane]["tiers"])
    missing = [(c, why) for c in claims
               if (why := (probe_submodules(tiers) if c == "submodules" else PROBES[c]())) is not None]
    if (why := probe_nextest()) is not None:
        missing.insert(0, ("nextest", why))
    if missing:
        print(f"gate-lane-preflight: lane `{lane}` claims what this host does not have:",
              file=sys.stderr)
        for c, why in missing:
            print(f"  {c}: {why}\n      fix: {REMEDY.get(c, '(see .config/gate-lanes.toml)')}",
                  file=sys.stderr)
        fixes = sorted({REMEDY[c] for c, _ in missing if REMEDY.get(c, "").startswith(("just ", "git "))})
        if fixes:
            print("  In one go: " + " && ".join(fixes), file=sys.stderr)
        print("  Or run a lane that claims less, e.g. `NROS_GATE_SCOPE=pristine just check fast`\n"
              "  (its out-of-scope gates are listed by name, never passed). No gate ran.",
              file=sys.stderr)
        return 1
    print(f"gate-lane-preflight: lane `{lane}`: {len(claims)} claimed capabilities present")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
