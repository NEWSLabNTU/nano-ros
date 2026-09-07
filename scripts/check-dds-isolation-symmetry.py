#!/usr/bin/env python3
"""issue 1137 — if the PEER's bus is pinned, OUR side's must be too.

Issue 1009 confined every DDS interop peer to loopback, because a participant on
the LAN is otherwise a peer: `ros2_env_setup_rmw_with_domain` exports a
`FASTRTPS_DEFAULT_PROFILES_FILE` / `CYCLONEDDS_URI` naming a whitelist profile.
That helper builds a `source setup.bash && export …` STRING, so it reaches
exactly one kind of process — a host `ros2` peer. Our own side of every pair is
a bare `std::process::Command`, and it got nothing.

The issue's own headline says what that costs: **pin both sides or neither; half
is no discovery.** It was measured for Fast-DDS (0 of 15 with the variable on one
side) and then half-applied. Two days later `cyclone_enumerates_a_stock_ros2_node`
— which had PASSED live on 2026-08-30, issue 0927 — enumerated one node, itself,
against a stock talker that was up. The stock talker was pinned to `127.0.0.1`
with `AllowMulticast=false`; the nano-ros probe kept Cyclone's default interface
pick. Neither side's SPDP could reach the other, and the symptom is
indistinguishable from a broken `ros_discovery_info` reader — which is what it
was filed as, for the second time.

What this checks
----------------
A tracked test source that starts a HOST ROS 2 DDS peer (the helpers that pin,
listed in PINNING_PEERS) must also apply the matching pin to the processes it
spawns itself — `nros_tests::dds_isolation::apply_to_command` /
`apply_cyclone_config` / `apply_fastdds_profile`.

Issue 1139 — and the SHELL cells, which this gate could not see
---------------------------------------------------------------
The scan above walks `packages/testing/nros-tests/**/*.rs` and nothing else, so
its reach was narrower than the rule it enforces — issue 0196's shape, which
`check-grep-q-error-conflation` has recorded four times already. Two DDS
interop cells are
bash, not Rust — `ros2_{pubsub,srv}_e2e.sh` — and each carried its own inline
CycloneDDS config pinning the bus to a real ETHERNET interface with multicast
left on. That is the LAN: measured on this host, a nano-ros subscriber on the
old config took a foreign publisher's sample 5 of 5 times, and 0 of 5 on the
loopback config. Every OTHER DDS lane here had been confined since issue 1009;
these two were exempt by accident of language.

So the shell arm asks the same question in the shell vocabulary: a tracked
`*.sh` under a `tests/` directory that puts a stock `ros2` process on a DDS bus
must get its config from `nros_export_cyclone_config` (ros2_e2e_common.sh),
never from an inline `<CycloneDDS>` document of its own. One spelling, which is
the point — the two scripts had two copies of the same nine lines.

Two things are structurally out of scope rather than allowlisted:

  1. The implementation files themselves (`src/dds_isolation.rs`, `src/ros2.rs`,
     `src/ros_env.rs`). Those ARE the procedure.
  2. `DockerRosEnv` peers. A container has its own mount namespace, so it cannot
     read a host profile path, and those pairs are symmetric-UNPINNED today.
     Pinning our half of one would CREATE this bug rather than fix it — so the
     detector keys on `HostRosEnv` / the host `Ros2*Process` helpers, never on
     the `Middleware` enum both backends share.

Anything else legitimate is in ALLOWLIST, path-keyed, one reason each.

Dependency-free Python 3.10, house style per `scripts/check-ros-env-spelling.py`.
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TESTS = "packages/testing/nros-tests/"

# Helpers that start a host ROS 2 peer whose env is pinned by
# `ros2_env_setup_rmw_with_domain`. Every one of these funnels through it.
PINNING_PEERS = (
    "Ros2DdsProcess::",
    "ros2_env_setup_dds",
    "ros2_env_setup_cyclonedds",
    "ros2_env_setup_rmw_with_domain",
)

# `HostRosEnv` is only a pinning peer for a DDS middleware — a zenoh peer is
# keyed by locator, not by a bus, and nothing pins it. Requires BOTH tokens,
# because `DockerRosEnv` names the same `Middleware` variants and must not match.
HOST_ENV = "HostRosEnv::new"
DDS_MIDDLEWARE = ("Middleware::Cyclonedds", "Middleware::FastRtps")

# Our side of the pair, applied to a `Command`.
APPLIES_PIN = (
    "dds_isolation::apply_to_command",
    "dds_isolation::apply_cyclone_config",
    "dds_isolation::apply_fastdds_profile",
)

# The files that ARE the procedure — see docstring rule 1.
SANCTIONED = {
    TESTS + "src/dds_isolation.rs",
    TESTS + "src/ros2.rs",
    TESTS + "src/ros_env.rs",
}

# --- issue 1139, the shell arm -------------------------------------------
#
# A shell cell starts its ROS 2 peer by running the `ros2` CLI, and it puts that
# peer on a DDS bus by naming a DDS `RMW_IMPLEMENTATION`. Both tokens are
# required: a script that names the RMW but never runs `ros2` starts no peer,
# and `ros2` alone is the zenoh cells too, which are keyed by locator and pinned
# by nothing.
SHELL_DDS_RMW = ("rmw_cyclonedds_cpp", "rmw_fastrtps_cpp", "rmw_fastrtps_dynamic_cpp")
SHELL_RUNS_ROS2 = "ros2 "

# The shared helper that writes the loopback config and exports CYCLONEDDS_URI
# for BOTH halves of the pair.
SHELL_APPLIES_PIN = "nros_export_cyclone_config"

# An inline `<CycloneDDS>` document in a cell is the defect itself, even when
# the helper is also called: two configs, and the last `export` wins.
SHELL_INLINE_CONFIG = "<CycloneDDS"

# The file that IS the procedure, same rule as SANCTIONED above.
SHELL_SANCTIONED = {
    "packages/rmw/cyclonedds/nros-rmw-cyclonedds/tests/ros2_e2e_common.sh",
}

SHELL_ALLOWLIST = {}

ALLOWLIST = {
    TESTS
    + "tests/xrce_ros2_interop.rs": (
        "The nano-ros side is an XRCE client, not a DDS participant: it speaks "
        "XRCE to the micro-XRCE Agent, and the Agent is the process that joins "
        "the peer's DDS bus. The Agent IS pinned, by "
        "`fixtures::xrce_agent`'s `apply_fastdds_profile` (issue 1009). "
        "Pinning the XRCE client would export a variable it cannot read, which "
        "is the `ROS_LOCALHOST_ONLY` mistake one layer over."
    ),
}

REMEDY = (
    "call `nros_tests::dds_isolation::apply_to_command(&mut cmd)` on every "
    "Command this file spawns that joins the peer's DDS bus"
)
CONSEQUENCE = (
    "the peer is confined to 127.0.0.1 with AllowMulticast=false and our side "
    "keeps the middleware's default interface pick, so neither one's discovery "
    "traffic reaches the other. The cell then reports an EMPTY graph / no "
    "delivery, which reads as a backend defect (issues 0927 and 1137 were both "
    "filed as exactly that)."
)

SHELL_REMEDY = (
    "source `ros2_e2e_common.sh` and call "
    "`nros_export_cyclone_config <path>` instead of writing a `<CycloneDDS>` "
    "document in the script; it exports CYCLONEDDS_URI, which both the `ros2` "
    "peer and our own binaries inherit from the same shell"
)
SHELL_CONSEQUENCE = (
    "the cell's bus reaches the LAN, so any DDS participant a colleague, a "
    "robot or a CI runner leaves running is a peer of it. Measured on this "
    "host with a foreign /chatter publisher on the old ethernet config: the "
    "nano-ros subscriber took the foreign sample 5 of 5 times, and 0 of 5 on "
    "the loopback config. Issue 0741 lost five diagnoses to exactly one such "
    "peer, on a different machine."
)


def tracked_files():
    out = subprocess.run(
        ["git", "ls-files", TESTS + "tests", TESTS + "src", TESTS + "bins"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    return [p for p in out.stdout.splitlines() if p.endswith(".rs")]


def starts_a_pinned_peer(text):
    """Does this source start a HOST ROS 2 peer whose bus issue 1009 pinned?"""
    for token in PINNING_PEERS:
        if token in text:
            return token
    if HOST_ENV in text:
        for mw in DDS_MIDDLEWARE:
            if mw in text:
                return f"{HOST_ENV} + {mw}"
    return None


def applies_the_pin(text):
    return any(token in text for token in APPLIES_PIN)


def tracked_shell_cells():
    """Tracked `*.sh` living under a `tests/` directory."""
    out = subprocess.run(
        ["git", "ls-files", "*.sh"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    return [p for p in out.stdout.splitlines() if "/tests/" in p]


def shell_starts_a_dds_peer(text):
    """Does this script run a stock `ros2` process on a DDS bus?"""
    if SHELL_RUNS_ROS2 not in text:
        return None
    for rmw in SHELL_DDS_RMW:
        if rmw in text:
            return rmw
    return None


def shell_pin_finding(text):
    """None when the script is correct, else why it is not."""
    if SHELL_INLINE_CONFIG in text:
        return "writes its own inline <CycloneDDS> config"
    if SHELL_APPLIES_PIN not in text:
        return f"never calls `{SHELL_APPLIES_PIN}`"
    return None


def self_test(quiet=False):
    bad = []

    # A host DDS peer with no pin on our side is the defect.
    defect = 'Ros2DdsProcess::topic_echo_cyclonedds_with_domain(...);\nCommand::new(bin)'
    if starts_a_pinned_peer(defect) is None or applies_the_pin(defect):
        bad.append("the 1137 shape is not detected")

    # The same file, fixed.
    fixed = defect + "\nnros_tests::dds_isolation::apply_to_command(&mut cmd);"
    if not applies_the_pin(fixed):
        bad.append("a fixed file is still reported")

    # A DOCKER peer must NOT be treated as a pinned peer: those pairs are
    # symmetric-unpinned, and pinning our half would create the bug. This is the
    # case that makes the `HostRosEnv`/`Middleware` distinction load-bearing.
    docker = 'DockerRosEnv::new(&ed, Middleware::Cyclonedds { domain_id: d });'
    if starts_a_pinned_peer(docker) is not None:
        bad.append("a DockerRosEnv peer was read as a pinned peer")

    # A zenoh host peer is not a DDS bus at all.
    zenoh = "HostRosEnv::new(distro, Middleware::zenoh_default());"
    if starts_a_pinned_peer(zenoh) is not None:
        bad.append("a zenoh HostRosEnv peer was read as a pinned peer")

    # --- the shell arm (issue 1139) ---
    #
    # The specimens below name the RMW without spelling `export
    # RMW_IMPLEMENTATION=`, which `check-ros-env-spelling` forbids outside its
    # own sanctioned files. Nothing is lost: this gate keys on the RMW NAME plus
    # a `ros2 ` invocation, and how the variable got set is not part of its rule.
    #
    # The pre-1139 shape: a cyclone cell with its own inline config.
    inline = (
        'RMW=rmw_cyclonedds_cpp\n'
        'cat > "$CYCLONE_XML" <<XML\n<CycloneDDS xmlns="https://cdds.io/config">\n'
        'XML\nros2 topic echo /chatter\n'
    )
    if shell_starts_a_dds_peer(inline) is None:
        bad.append("an inline-config shell cell is not read as a DDS peer")
    if shell_pin_finding(inline) is None:
        bad.append("the 1139 shape (inline <CycloneDDS>) is not detected")

    # The same cell, fixed.
    fixed_sh = (
        'RMW=rmw_cyclonedds_cpp\n'
        'nros_export_cyclone_config "$CYCLONE_XML" || exit 0\n'
        'ros2 topic echo /chatter\n'
    )
    if shell_pin_finding(fixed_sh) is not None:
        bad.append("a fixed shell cell is still reported")

    # A cell that names no DDS RMW starts no bus peer — a zenoh script must not
    # be asked to pin a CycloneDDS config it never uses.
    zenoh_sh = 'RMW=rmw_zenoh_cpp\nros2 topic echo /chatter\n'
    if shell_starts_a_dds_peer(zenoh_sh) is not None:
        bad.append("a zenoh shell cell was read as a DDS peer")

    # A script that names the RMW but runs no `ros2` starts no peer either.
    no_peer_sh = 'RMW=rmw_cyclonedds_cpp\n"$SOME_BIN"\n'
    if shell_starts_a_dds_peer(no_peer_sh) is not None:
        bad.append("a script that runs no ros2 CLI was read as a peer")

    # Every allowlisted / sanctioned path still exists — a stale entry is a hole
    # that reads like a decision.
    for path in sorted(SANCTIONED | set(ALLOWLIST) | SHELL_SANCTIONED | set(SHELL_ALLOWLIST)):
        if not (ROOT / path).is_file():
            bad.append(f"{path} is named here but does not exist")

    if bad:
        for b in bad:
            sys.stderr.write("check-dds-isolation-symmetry --self-test: " + b + "\n")
        return 2
    if not quiet:
        print("check-dds-isolation-symmetry --self-test: OK (8 case(s), "
              f"{len(SANCTIONED) + len(SHELL_SANCTIONED)} sanctioned, "
              f"{len(ALLOWLIST) + len(SHELL_ALLOWLIST)} allowlisted)")
    return 0


def main():
    if "--self-test" in sys.argv:
        return self_test()
    if self_test(quiet=True) != 0:
        return 2

    findings = []
    peers = 0
    for path in tracked_files():
        if path in SANCTIONED:
            continue
        full = ROOT / path
        if not full.is_file():
            continue
        try:
            text = full.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        token = starts_a_pinned_peer(text)
        if token is None:
            continue
        peers += 1
        if path in ALLOWLIST or applies_the_pin(text):
            continue
        findings.append((path, token))

    shell_findings = []
    shell_cells = 0
    for path in tracked_shell_cells():
        if path in SHELL_SANCTIONED:
            continue
        full = ROOT / path
        if not full.is_file():
            continue
        try:
            text = full.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError):
            continue
        if shell_starts_a_dds_peer(text) is None:
            continue
        shell_cells += 1
        if path in SHELL_ALLOWLIST:
            continue
        why = shell_pin_finding(text)
        if why is not None:
            shell_findings.append((path, why))

    print(
        f"DDS isolation symmetry: {peers} file(s) start a pinned host ROS 2 DDS "
        f"peer, {len(ALLOWLIST)} allowlisted; {shell_cells} shell cell(s) start "
        f"a DDS `ros2` peer, {len(SHELL_ALLOWLIST)} allowlisted"
    )

    if findings:
        print(
            "\n[FAIL] a pinned ROS 2 DDS peer with an UNPINNED nano-ros side:",
            file=sys.stderr,
        )
        for path, token in findings:
            print(f"  - {path}: starts a peer via `{token}`", file=sys.stderr)
        print(f"\n  WHAT TO DO: {REMEDY}", file=sys.stderr)
        print(f"\n  WHY IT MATTERS: {CONSEQUENCE}", file=sys.stderr)

    if shell_findings:
        print(
            "\n[FAIL] a shell DDS interop cell whose bus is not confined:",
            file=sys.stderr,
        )
        for path, why in shell_findings:
            print(f"  - {path}: {why}", file=sys.stderr)
        print(f"\n  WHAT TO DO: {SHELL_REMEDY}", file=sys.stderr)
        print(f"\n  WHY IT MATTERS: {SHELL_CONSEQUENCE}", file=sys.stderr)

    if findings or shell_findings:
        return 1

    print("Every pinned peer has a pinned partner.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
