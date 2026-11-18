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
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
from git_hook_env import nros_clear_inherited_git_env  # noqa: E402
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


def probe_submodule_drift() -> tuple[str, str] | None:
    """A checked-out submodule BEHIND or DIVERGED from its recorded pin (issue
    2701), as (what, remedy). A rebase across a pin move leaves an
    already-checked-out submodule where it was, and `setup-worktree` keeps it
    there on purpose (the update would discard work in it), so the gates then
    trip on the old tree with messages that never name the submodule. Refused
    here, never updated: the remedy is printed per path. AHEAD (a fork fix
    mid-flight) and NOT VERIFIED (a shallow store) pass, as in the script."""
    r = subprocess.run(["bash", str(ROOT / "scripts" / "check-submodule-drift.sh")], cwd=ROOT,
                       capture_output=True, text=True,
                       env=dict(os.environ, NROS_SKIP_SUBMODULE_DRIFT_CHECK="0"))
    return None if r.returncode == 0 else parse_drift(r.stderr)


def probe_zephyr_pin_drift() -> tuple[str, str] | None:
    """A provisioned Zephyr workspace whose projects sit off THIS checkout's
    west pins (issue 2697): a build there uses a kernel the pin does not name,
    and only the boot banner says so. Same shape as `probe_submodule_drift`,
    over the same classifier (`scripts/lib/pin-drift.sh`); refused, never
    moved, because the workspace is shared by every session on the host."""
    r = subprocess.run([sys.executable, str(ROOT / "scripts" / "check-zephyr-pin-drift.py")],
                       cwd=ROOT, capture_output=True, text=True)
    if r.returncode == 0:
        return None
    what, fix = parse_drift(r.stderr)
    return what, (fix if fix != "bash scripts/check-submodule-drift.sh (read it)"
                  else "python3 scripts/check-zephyr-pin-drift.py (read it)")


def parse_drift(stderr: str) -> tuple[str, str]:
    """`check-submodule-drift.sh`'s failure text -> (each `[x]` row, each remedy)."""
    lines = [l.strip() for l in stderr.splitlines()]
    rows = [l[4:] for l in lines if l.startswith("[x] ")]
    fixes = [l[len("remedy: "):].split("   (")[0] for l in lines if l.startswith("remedy: ")]
    what = "; ".join(rows) or next((l for l in lines if l), "check-submodule-drift.sh failed")
    return what, " && ".join(dict.fromkeys(fixes)) or "bash scripts/check-submodule-drift.sh (read it)"


def probe_arm_gcc_any() -> str | None:
    # ANY arm-none-eabi-g++, pinned or not, in the order
    # check-px4-uorb-nuttx-compile resolves it (issue 2419).
    prefix = os.environ.get("NROS_ARM_NONE_EABI_PREFIX")
    if prefix:
        return None if shutil.which(prefix + "-g++") else f"{prefix}-g++ is not executable"
    return on_path("arm-none-eabi-g++")


def probe_tiarmclang() -> str | None:
    # [gated.ti-arm-clang]: the install TI_ARM_CLANG_ROOT names (phase-575 W3).
    root = os.environ.get("TI_ARM_CLANG_ROOT")
    if not root:
        return "TI_ARM_CLANG_ROOT is unset"
    tool = os.path.join(root, "bin", "tiarmclang")
    return None if os.access(tool, os.X_OK) else f"{tool} is not executable"


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


def probe_arm_gcc() -> str | None:
    """The PINNED cross compiler (issue 2413): the store's dist, or a PATH copy
    that passes the row's own smoke probe. The distro 10.3 on PATH does not."""
    r = subprocess.run(["python3", "scripts/lib/index_tool.py", "resolve", "arm-none-eabi-gcc",
                        "arm-none-eabi-gcc"], cwd=ROOT, capture_output=True, text=True)
    return None if r.returncode == 0 else "the pinned arm-none-eabi-gcc is not installed"


def probe_nextest() -> str | None:
    """The harness's own runner, at the index's pin. Not a capability a gate
    needs: every lane needs it, so it is asked first, with the same remedy."""
    r = subprocess.run(["python3", "scripts/lib/index_tool.py", "resolve", "cargo-nextest",
                        "cargo-nextest"], cwd=ROOT, capture_output=True, text=True)
    return None if r.returncode == 0 else "the pinned cargo-nextest is not installed"


# What fixes each missing claim: `[provision.claim] <cap> remedy` in
# .config/gate-lanes.toml (phase-565 W2), the same table setup-worktree and CI's
# provisioner run from, so a refusal and a provisioner cannot name different
# fixes. A refusal names the fix, never only the gap: a fresh agent worktree is
# how most sessions work, and `check fast` is what its pre-push hook runs.
# `nextest` is not a capability (every lane needs it), so its fix is here.
def remedies(cfg: dict) -> dict:
    sys.path.insert(0, str(ROOT / "scripts" / "lib"))
    import lane_provision  # the one reader of the table (phase-565 W1)
    out = {c: e.get("remedy", "") for c, e in lane_provision.claim_entries(cfg).items()}
    out["nextest"] = "just setup-worktree"
    return out


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
    "fsp": lambda: env_dir("NV_SPE_FSP_DIR"),
    "west": probe_west,
    "build-tree": probe_build_tree,
    "arm-gcc": probe_arm_gcc,
    "tiarmclang": probe_tiarmclang,
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
            + [f"capability `{c}` has no remedy in [provision.claim]"
               for c in sorted(caps - {k for k, v in remedies(cfg).items() if v})])


def self_test() -> None:
    cfg = {"capabilities": {k: "" for k in PROBES}, "lane": {"x": {"claims": ["cmake"]}},
           "provision": {"claim": {k: {"remedy": "fix"} for k in PROBES}}}
    assert coverage(cfg) == [], coverage(cfg)
    cfg["capabilities"]["new"] = ""
    assert coverage(cfg) == ["capability `new` has no probe here",
                             "capability `new` has no remedy in [provision.claim]"], coverage(cfg)
    assert lane_claims(cfg, "x") == ["cmake"]
    assert on_path("nros-no-such-tool-543") is not None
    assert env_dir("NROS_NO_SUCH_VAR_543") == "$NROS_NO_SUCH_VAR_543 is unset"
    # issue 2701: a behind row names its path and its own remedy, without the
    # script's parenthetical; a diverged row's remedy is a rebase, not an update.
    what, fix = parse_drift(
        "  [x] a/b is 1 commit(s) BEHIND the recorded pointer\n"
        "      at 111, HEAD records 222\n"
        "      remedy: git submodule update a/b   (fast-forward, no local work at risk)\n"
        "  [x] c is INCOMPLETE\n      remedy: git -C c checkout -- .\n")
    assert what == "a/b is 1 commit(s) BEHIND the recorded pointer; c is INCOMPLETE", what
    assert fix == "git submodule update a/b && git -C c checkout -- .", fix
    assert parse_drift("boom\n") == ("boom", "bash scripts/check-submodule-drift.sh (read it)")


def main(argv: list[str]) -> int:
    # Issue 2743 — under `git bisect run` / `git rebase -x` an inherited
    # GIT_DIR makes every probe below read the superproject's index from
    # inside each submodule. Cleared once, before any probe spawns git.
    nros_clear_inherited_git_env()
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
    REMEDY = remedies(cfg)
    claims = lane_claims(cfg, lane)
    tiers = list(cfg["lane"][lane]["tiers"])
    missing = [(c, why) for c in claims
               if (why := (probe_submodules(tiers) if c == "submodules" else PROBES[c]())) is not None]
    if (why := probe_nextest()) is not None:
        missing.insert(0, ("nextest", why))
    if "submodules" in claims and (drift := probe_submodule_drift()) is not None:
        # Not a capability: `setup-worktree` cannot provide it (it keeps a
        # drifted checkout), so its fix is the per-path remedy, never that.
        missing.append(("submodule-drift", drift[0]))
        REMEDY["submodule-drift"] = drift[1]
    if ({"zephyr", "west"} & set(claims)) and (zdrift := probe_zephyr_pin_drift()) is not None:
        # Issue 2697 -- like submodule-drift, a state of the HOST's workspace
        # that no provisioner here may change (it is shared), so the fix is
        # the printed per-project remedy.
        missing.append(("zephyr-pin-drift", zdrift[0]))
        REMEDY["zephyr-pin-drift"] = zdrift[1]
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
