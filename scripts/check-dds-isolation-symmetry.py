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

The ctest arm -- the C++ suite, which neither arm above could see
-----------------------------------------------------------------
`nros-rmw-cyclonedds`'s own ctest suite starts Cyclone in every test, from
CMake, so it is neither a Rust test nor a shell cell. It ran on Cyclone's own
interface pick: on a host whose first interface does not loop multicast back
to itself, `nros_rmw_cyclonedds_session_smoke` never finished discovery and the
whole suite hit its 2400 s gate timeout, while the same binary on the loopback
profile passed in under a second. So a tracked CMake file that names
`nros_rmw_cyclonedds` and registers tests must register each one through
`nros_rmw_cyclonedds_add_test`, the helper that runs it under
`dds_loopback_exec.sh` (the committed loopback profile, the same file the Rust
harness reads). A bare `add_test` there is refused.

The launcher pins through `CYCLONEDDS_URI`, which Cyclone reads only when a
domain is created WITHOUT a config. So a test source under `tests/` that calls
`dds_create_domain(id, config)` with its own string must pass a buffer that
`nros_test_pinned_cyclone_config` (nros_test_domain.h) filled, pin first.
`config_compose` skipped it and hung 12 runs in 40 in `dds_delete`.

Dependency-free Python 3.10, house style per `scripts/check-ros-env-spelling.py`.
"""

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

# phase-472 W3 — both halves are read as CODE. A commented-out
# `// dds_isolation::apply_to_command(&mut cmd);` applied no pin and used to
# satisfy `applies_the_pin`; a `# nros_export_cyclone_config` likewise.
sys.path.insert(0, str(ROOT / "scripts" / "lib"))
import comments  # noqa: E402
import per_item  # noqa: E402  phase-472 W6 — per spawning function, not per file
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
    "packages/rmw/cyclonedds/nros-rmw-cyclonedds/tests/dds_loopback.sh",
}

SHELL_ALLOWLIST = {}

# The ctest arm: a CMake file naming this token registers Cyclone tests.
CTEST_STARTS_CYCLONE = "nros_rmw_cyclonedds"
# The one sanctioned registration, and the launcher it must route through.
CTEST_HELPER = "nros_rmw_cyclonedds_add_test"
CTEST_LAUNCHER = "dds_loopback_exec.sh"
BARE_ADD_TEST = re.compile(r"(?<![\w])add_test\s*\(")
HELPER_BODY = re.compile(
    r"function\s*\(\s*" + CTEST_HELPER + r"\b(.*?)endfunction\s*\(", re.S)
CTEST_REMEDY = (
    f"register the test with `{CTEST_HELPER}(NAME <n> COMMAND <cmd> ...)`, "
    f"which runs it under `{CTEST_LAUNCHER}` (the committed loopback profile; "
    "`NROS_DDS_ALLOW_LAN` or an operator's `CYCLONEDDS_URI` opts out)"
)
CTEST_CONSEQUENCE = (
    "the test runs on Cyclone's own interface pick, so its result depends on "
    "the host's network: measured, `session_smoke` hung in discovery on a host "
    "whose first interface carries no multicast back, and the suite hit its "
    "2400 s gate timeout; on the loopback profile it passed in under a second"
)

# The ctest arm's second half: a test SOURCE that hands Cyclone its own config.
# `dds_create_domain(id, config)` with a non-null config reads that string and
# nothing else, so the launcher's `CYCLONEDDS_URI` never reaches it. Such a call
# must pass a buffer `nros_test_pinned_cyclone_config` filled (pin first, then
# the test's fragments). A null config is fine: Cyclone reads the variable then.
CTEST_SOURCE_SUFFIXES = (".c", ".cc", ".cpp")
CTEST_SOURCE_DIR = "/tests/"
CREATE_DOMAIN = re.compile(r"(?<![\w])dds_create_domain\s*\(")
CREATE_DOMAIN_CONFIG = re.compile(
    r"(?<![\w])dds_create_domain\s*\(\s*[^,;]+,\s*([A-Za-z_]\w*)\s*\)")
NULL_CONFIGS = {"NULL", "nullptr"}
SOURCE_PIN_HELPER = "nros_test_pinned_cyclone_config"
SOURCE_PIN_FILLS = re.compile(
    r"(?<![\w])" + SOURCE_PIN_HELPER + r"\s*\(\s*([A-Za-z_]\w*)\s*,")
SOURCE_REMEDY = (
    f"build the config with `{SOURCE_PIN_HELPER}(buf, sizeof(buf), config)` "
    "(nros_test_domain.h) and pass `buf`: it puts the launcher's loopback pin "
    "first, so the test's own fragments still override it"
)
SOURCE_CONSEQUENCE = (
    "the domain comes up on Cyclone's own interface pick even under the "
    "launcher: measured, `config_compose` hung 12 runs in 40 in `dds_delete`, "
    "joining a multicast receive thread whose wake-up never looped back, and "
    "took the whole suite to its 2400 s gate timeout"
)

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
    TESTS
    + "tests/cyclone_slirp_pair.rs": (
        "Issue 1251. Neither half is on the LOOPBACK profile, by design: the "
        "nano side is a guest behind slirp, which that profile cannot reach. "
        "Both halves come from one `dds_isolation::CycloneSlirpPair` (the "
        "peer through `ros2_env_setup_cyclonedds_with_profile`, the guest "
        "through `guest_config_uri`), so they are pinned SYMMETRICALLY, by "
        "tag rather than by interface. `apply_to_command` here would put the "
        "guest on the loopback profile and recreate the collision the test "
        "measures."
    ),
    TESTS
    + "tests/pubsub_freertos_an536_cyclone_ros2_interop_e2e.rs": (
        "phase-441 W6, the same pair on a real image. The nano side is a QEMU "
        "guest behind slirp whose Cyclone config is BAKED: the bringup's "
        "`rmw/cyclonedds.xml` is `CycloneSlirpPair::guest_xml`, held to it by "
        "`the_bringup_bakes_the_pairs_guest_half`, and the peer takes the "
        "pair's host half through `ros2_env_setup_cyclonedds_with_profile`. "
        "Pinned symmetrically by tag; the only Command spawned for our side is "
        "QEMU, which joins no DDS bus itself."
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


FN_HEAD = re.compile(r"\bfn\s+(\w+)")


def unpinned_functions(text):
    """Names of functions that start a pinned peer AND spawn a `Command` of
    their own, yet neither apply the pin nor call a same-file function that does.

    phase-472 W6 — the rule is per PROCESS and this read it per FILE: one pinned
    spawn anywhere satisfied every spawn beside it, so deleting the pin from
    `cyclone_a_peer_leaving_fires_the_graph_change_guard` stayed green because
    `cyclone_enumerates_a_stock_ros2_node` still had one. A function is the
    granularity that can be read without dataflow: it names the peer and the
    `Command` it pairs with.
    """
    fns = per_item.blocks(text, FN_HEAD)
    pinning = {m.group(1) for m, o, e in fns if applies_the_pin(text[o:e])}

    def calls(body, name):
        return re.search(rf"(?<![\w:.]){re.escape(name)}\s*\(", body) is not None

    def pinned(body):
        return applies_the_pin(body) or any(calls(body, p) for p in pinning)

    # issue 1737 — a SAME-FILE spawn helper is the peer function's process too.
    # `bridge_zenoh_to_cyclonedds.rs` spawns through `spawn_bridge(...)`; the
    # ros2-peer test that calls it holds no `Command::new(` of its own, so the
    # per-fn rule skipped it and dropping the pin inside the helper passed.
    # A helper whose process is a ZENOH session — it is handed a zenoh locator
    # (`NROS_LOCATOR`, `ros2_env_setup_with_locator`) and names no DDS domain —
    # is not on the peer's DDS bus and has nothing to pin.
    def zenoh_only(hbody):
        return (re.search(r'"NROS_LOCATOR"|\bros2_env_setup_with_locator\b', hbody) is not None
                and re.search(r'ROS_DOMAIN_ID|(?i:cyclone|fastrtps|dds)', hbody) is None)

    spawners = {m.group(1): text[o:e] for m, o, e in fns
                if "Command::new(" in text[o:e] and not zenoh_only(text[o:e])}
    out = []
    for m, o, e in fns:
        body = text[o:e]
        if starts_a_pinned_peer(body) is None:
            continue
        name = m.group(1)
        if "Command::new(" in body and not pinned(body):
            out.append(name)
        for helper, hbody in sorted(spawners.items()):
            if helper != name and calls(body, helper) and not pinned(hbody):
                out.append(f"{name} (spawns via {helper})")
    return out


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


def tracked_cmake_files():
    """Tracked CMake sources (`CMakeLists.txt`, `*.cmake`)."""
    out = subprocess.run(
        ["git", "ls-files", "*CMakeLists.txt", "*.cmake"],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    return out.stdout.splitlines()


def ctest_pin_findings(text):
    """Why this CMake text registers an unpinned Cyclone test, one per site.

    `text` has comments stripped. Empty when the file registers no Cyclone
    test, or registers each through the pinned helper.
    """
    if CTEST_STARTS_CYCLONE not in text or not BARE_ADD_TEST.search(text):
        return []
    out = []
    helper = HELPER_BODY.search(text)
    allowed = None
    if helper is not None:
        if CTEST_LAUNCHER not in text or "_nros_dds_loopback_exec" not in helper.group(1):
            out.append(f"`{CTEST_HELPER}` does not run its test under `{CTEST_LAUNCHER}`")
        allowed = helper.span()
    for m in BARE_ADD_TEST.finditer(text):
        if allowed and allowed[0] <= m.start() < allowed[1]:
            continue
        line = text.count("\n", 0, m.start()) + 1
        out.append(f"line {line}: a bare `add_test`")
    return out


def tracked_test_sources():
    """Tracked C/C++ sources under a `tests/` directory."""
    out = subprocess.run(
        ["git", "ls-files", *(f"*{s}" for s in CTEST_SOURCE_SUFFIXES)],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    return [p for p in out.stdout.splitlines() if CTEST_SOURCE_DIR in "/" + p]


def source_pin_findings(text):
    """Why this test source starts a Cyclone domain off the pin, one per call.

    `text` has comments stripped. A call passes when its config is null or a
    buffer `nros_test_pinned_cyclone_config` fills in the same file. A config
    the regex cannot read as one identifier (an expression, a literal) is
    refused: the rule is about where the string came from, and an expression
    hides that.
    """
    pinned = set(SOURCE_PIN_FILLS.findall(text))
    out = []
    for m in CREATE_DOMAIN.finditer(text):
        line = text.count("\n", 0, m.start()) + 1
        arg = CREATE_DOMAIN_CONFIG.match(text, m.start())
        if arg is None:
            out.append(f"line {line}: `dds_create_domain` with a config that is not "
                       "a single buffer name")
            continue
        name = arg.group(1)
        if name in NULL_CONFIGS or name in pinned:
            continue
        out.append(f"line {line}: `dds_create_domain(..., {name})`, and `{name}` is "
                   f"not filled by `{SOURCE_PIN_HELPER}`")
    return out


def self_test(quiet=False):
    bad = []
    per_item.self_test()  # the shared helper's own controls (phase-472 W6)

    # A host DDS peer with no pin on our side is the defect.
    defect = 'Ros2DdsProcess::topic_echo_cyclonedds_with_domain(...);\nCommand::new(bin)'
    if starts_a_pinned_peer(defect) is None or applies_the_pin(defect):
        bad.append("the 1137 shape is not detected")

    # phase-472 W3 — the pin in a COMMENT is no pin (the reader strips first).
    comments.self_test()
    commented = comments.strip_comments(
        defect + "\n// nros_tests::dds_isolation::apply_to_command(&mut cmd);", "rust")
    if applies_the_pin(commented):
        bad.append("a commented-out apply_to_command was read as the pin")

    # The same file, fixed.
    fixed = defect + "\nnros_tests::dds_isolation::apply_to_command(&mut cmd);"
    if not applies_the_pin(fixed):
        bad.append("a fixed file is still reported")

    # phase-472 W6 — per spawning function: a pin in ONE function does not
    # cover a second one that starts a peer and spawns its own Command.
    two = (
        "fn a() { Ros2DdsProcess::x(); let mut c = Command::new(b);\n"
        "  nros_tests::dds_isolation::apply_to_command(&mut c); }\n"
        "fn b() { Ros2DdsProcess::x(); let mut c = Command::new(b); }\n"
        "fn spawn_pinned() { let mut c = Command::new(b);\n"
        "  nros_tests::dds_isolation::apply_to_command(&mut c); }\n"
        "fn c() { Ros2DdsProcess::x(); let mut d = Command::new(b); spawn_pinned(); }\n"
    )
    if unpinned_functions(two) != ["b"]:
        bad.append(f"per-function pinning misjudged: {unpinned_functions(two)}")
    # issue 1737 — the spawn goes through a same-file HELPER the peer fn calls.
    via = (
        "fn spawn_bridge(b: &Path) -> P { let mut c = Command::new(b); P::new(c) }\n"
        "fn t() { Ros2DdsProcess::x(); let _b = spawn_bridge(p); }\n"
    )
    if unpinned_functions(via) != ["t (spawns via spawn_bridge)"]:
        bad.append(f"an unpinned same-file spawn helper was not followed: {unpinned_functions(via)}")
    via_ok = via.replace("let mut c = Command::new(b);",
                         "let mut c = Command::new(b); nros_tests::dds_isolation::apply_to_command(&mut c);")
    if unpinned_functions(via_ok):
        bad.append(f"a PINNED spawn helper was reported: {unpinned_functions(via_ok)}")
    zen = via.replace("let mut c = Command::new(b);",
                      'let mut c = Command::new(b); c.env("NROS_LOCATOR", l);')
    if unpinned_functions(zen):
        bad.append(f"a zenoh-session spawn helper was asked for a DDS pin: {unpinned_functions(zen)}")
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

    # --- the ctest arm ---
    helper_ok = (
        'set(_nros_dds_loopback_exec "${D}/dds_loopback_exec.sh")\n'
        "function(nros_rmw_cyclonedds_add_test)\n"
        '  add_test(NAME ${n} COMMAND bash "${_nros_dds_loopback_exec}" ${c})\n'
        "endfunction()\n"
    )
    pinned = helper_ok + "nros_rmw_cyclonedds_add_test(NAME t COMMAND nros_rmw_cyclonedds_x)\n"
    if ctest_pin_findings(pinned):
        bad.append(f"a suite registered through the helper was reported: {ctest_pin_findings(pinned)}")
    # The negative control: the shape the suite had, one bare registration.
    bare = pinned + "add_test(NAME u COMMAND nros_rmw_cyclonedds_y)\n"
    if ctest_pin_findings(bare) != ["line 6: a bare `add_test`"]:
        bad.append(f"a bare Cyclone add_test was not refused: {ctest_pin_findings(bare)}")
    # A helper that does not route through the launcher pins nothing.
    unrouted = pinned.replace('bash "${_nros_dds_loopback_exec}" ', "")
    if not ctest_pin_findings(unrouted):
        bad.append("a helper that skips the launcher was accepted")
    # A commented-out registration is not one.
    commented_ct = comments.strip_comments(pinned + "# add_test(NAME v COMMAND w)\n", "cmake")
    if ctest_pin_findings(commented_ct):
        bad.append("a commented-out add_test was reported")
    # A suite of another backend is out of scope.
    other = "add_test(NAME x COMMAND nros_rmw_xrce_x)\n"
    if ctest_pin_findings(other):
        bad.append("a non-Cyclone suite was asked for a Cyclone pin")

    # --- the ctest arm, test sources (the config_compose shape) ---
    src_ok = (
        "char cfg[64];\n"
        "nros_test_pinned_cyclone_config(cfg, sizeof(cfg), frag);\n"
        "dds_entity_t d = dds_create_domain(id, cfg);\n"
    )
    if source_pin_findings(src_ok):
        bad.append(f"a pinned dds_create_domain was reported: {source_pin_findings(src_ok)}")
    # The negative control: the shape the test had, its own string straight in.
    src_bad = src_ok + "dds_entity_t e = dds_create_domain(id + 1, frag);\n"
    want_bad = (
        "line 4: `dds_create_domain(..., frag)`, and `frag` is not filled by "
        "`nros_test_pinned_cyclone_config`"
    )
    if source_pin_findings(src_bad) != [want_bad]:
        bad.append(f"an unpinned dds_create_domain was not refused: {source_pin_findings(src_bad)}")
    # A null config reads CYCLONEDDS_URI itself, so it is pinned already.
    if source_pin_findings("dds_create_domain(id, NULL);\ndds_create_domain(id, nullptr);\n"):
        bad.append("a null-config dds_create_domain was reported")
    # An expression hides where the string came from: refused, not guessed.
    if not source_pin_findings("dds_create_domain(id, ok ? cfg : frag);\n"):
        bad.append("an expression config was accepted")
    # A commented-out call is not one.
    if source_pin_findings(comments.strip_comments("// dds_create_domain(id, frag);\n", "c")):
        bad.append("a commented-out dds_create_domain was reported")
    # Nor is one named in a string (a diagnostic that prints the call).
    msg = 'fprintf(stderr, "dds_create_domain(%s) failed", tag);\n'
    if source_pin_findings(comments.strip_comments(msg, "c", strings=True)):
        bad.append("a dds_create_domain inside a string literal was reported")

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
        print("check-dds-isolation-symmetry --self-test: OK (19 case(s), "
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
            text = comments.strip_comments(full.read_text(encoding="utf-8"), "rust")
        except (OSError, UnicodeDecodeError):
            continue
        token = starts_a_pinned_peer(text)
        if token is None:
            continue
        peers += 1
        if path in ALLOWLIST:
            continue
        if not applies_the_pin(text):
            findings.append((path, token))
            continue
        for fn in unpinned_functions(text):
            findings.append((f"{path} (fn `{fn}`)", token))

    shell_findings = []
    shell_cells = 0
    for path in tracked_shell_cells():
        if path in SHELL_SANCTIONED:
            continue
        full = ROOT / path
        if not full.is_file():
            continue
        try:
            text = comments.strip_comments(full.read_text(encoding="utf-8"), "sh")
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

    ctest_findings = []
    ctest_files = 0
    for path in tracked_cmake_files():
        full = ROOT / path
        if not full.is_file():
            continue
        try:
            text = comments.strip_comments(full.read_text(encoding="utf-8"), "cmake")
        except (OSError, UnicodeDecodeError):
            continue
        if CTEST_STARTS_CYCLONE in text and BARE_ADD_TEST.search(text):
            ctest_files += 1
        for why in ctest_pin_findings(text):
            ctest_findings.append((path, why))

    source_findings = []
    source_files = 0
    for path in tracked_test_sources():
        full = ROOT / path
        if not full.is_file():
            continue
        lang = "c" if path.endswith(".c") else "cpp"
        try:
            text = comments.strip_comments(full.read_text(encoding="utf-8"), lang, strings=True)
        except (OSError, UnicodeDecodeError):
            continue
        if not CREATE_DOMAIN.search(text):
            continue
        source_files += 1
        for why in source_pin_findings(text):
            source_findings.append((path, why))

    print(
        f"DDS isolation symmetry: {peers} file(s) start a pinned host ROS 2 DDS "
        f"peer, {len(ALLOWLIST)} allowlisted; {shell_cells} shell cell(s) start "
        f"a DDS `ros2` peer, {len(SHELL_ALLOWLIST)} allowlisted; "
        f"{ctest_files} CMake file(s) register Cyclone ctests; "
        f"{source_files} test source(s) create a Cyclone domain directly"
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

    if ctest_findings:
        print(
            "\n[FAIL] a Cyclone ctest registered without the loopback pin:",
            file=sys.stderr,
        )
        for path, why in ctest_findings:
            print(f"  - {path}: {why}", file=sys.stderr)
        print(f"\n  WHAT TO DO: {CTEST_REMEDY}", file=sys.stderr)
        print(f"\n  WHY IT MATTERS: {CTEST_CONSEQUENCE}", file=sys.stderr)

    if source_findings:
        print(
            "\n[FAIL] a test source that starts a Cyclone domain off the loopback pin:",
            file=sys.stderr,
        )
        for path, why in source_findings:
            print(f"  - {path}: {why}", file=sys.stderr)
        print(f"\n  WHAT TO DO: {SOURCE_REMEDY}", file=sys.stderr)
        print(f"\n  WHY IT MATTERS: {SOURCE_CONSEQUENCE}", file=sys.stderr)

    if findings or shell_findings or ctest_findings or source_findings:
        return 1

    print("Every pinned peer has a pinned partner.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
