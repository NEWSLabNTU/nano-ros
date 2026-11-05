# shellcheck shell=bash
# PROBE-OWNED verification of the release underlay (phase-527 W6, RFC-0119
# D4) on the `installed` track. Appended after verify-installed-platform.sh
# for the default chapter (first-project.md), in the same shell: cwd is the
# chapter's `sensor_ws` workspace (phase-531 W4), whose `sensor_node` `nros run`
# just booted on QEMU through the qemu runner the release ships PREBUILT under
# share/nros/packages/.
#
# Three claims, each asked of the installed toolchain with no checkout:
#   1. the runners resolve from that underlay, not from a checkout;
#   2. `nros run` runs the NATIVE talker (the native runner, a host process);
#   3. a user package announcing `runner/qemu` is refused by `nros run` while
#      undeclared, replaces ours once declared, and `nros doctor` reports it
#      either way (W2b/W2c's rules, unchanged over the released underlay).

echo '=== probe verify: the release underlay (phase-527 W6) ==='
underlay_fail() {
    echo "PROBE FAIL: $*" >&2
    exit 1
}
ws="$PWD"
node=sensor_node   # first-project.md's node: `nros new sensor_ws` names it
# The chapter leaves its `nros run` running for its `ros2 topic echo` step
# (`probe-keep`); its router holds port 7447, which every run below needs.
for pgid in ${NROS_PROBE_KEPT_PGIDS:-}; do
    kill -- "-$pgid" 2>/dev/null || true
    for _ in 1 2 3 4 5 6 7 8 9 10; do
        kill -0 -- "-$pgid" 2>/dev/null || break
        sleep 1
    done
done
[ -d "$ws/src/$node" ] || underlay_fail "expected the chapter's workspace as cwd (with src/$node), got $ws"
# The chapter sourced ROS 2 only inside its `nros run` steps' own sessions,
# and every `nros run` below starts the zenoh router ROS 2 provides (the
# image's serial link), so source it here the way the chapter tells a reader
# to. setup.bash reads unset variables, hence the `+u` around it.
set +u
# shellcheck disable=SC1091
source /opt/ros/humble/setup.bash
set -u

# --- 1. the underlay is the release's own ------------------------------------
sdk_root="$(nros dev sdk root)" || underlay_fail "\`nros dev sdk root\` failed"
prefix="$(dirname "$(dirname "$sdk_root")")"
packages="$prefix/share/nros/packages"
[ -x "$packages/nros_runner_qemu/bin/nros-runner-qemu" ] \
    || underlay_fail "the release carries no prebuilt qemu runner at $packages/nros_runner_qemu/bin"
[ -x "$packages/nros_runner_native/bin/nros-runner-native" ] \
    || underlay_fail "the release carries no prebuilt native runner at $packages/nros_runner_native/bin"
[ ! -e "$sdk_root/packages/runners/nros_runner_qemu" ] \
    || underlay_fail "the SDK root still carries the qemu runner's source copy (the underlay would announce it twice)"
why="$(nros run "$node" --dry-run --why 2>&1)" || underlay_fail "\`nros run $node --dry-run --why\` failed: $why"
printf '%s\n' "$why" | tail -3
nros_grep_q "nros_runner_qemu" <<<"$why" \
    || underlay_fail "\`nros run --why\` names no nros_runner_qemu: $why"
echo "probe: the qemu runner resolves from $packages"

# --- 3. a user's runner/qemu over the released underlay -----------------------
# An overlay package (NROS_PACKAGE_PATH) whose executable wraps ours and
# leaves a mark, so a run that went through it is provable.
user_root="$HOME/my_runners"
user_pkg="$user_root/my_qemu"
mark="$HOME/my_qemu.used"
mkdir -p "$user_pkg/bin"
printf '%s\n' \
    '<?xml version="1.0"?>' \
    '<package format="3">' \
    '  <name>my_qemu</name><version>0.0.0</version>' \
    '  <description>A user qemu runner (phase-527 W6 probe).</description>' \
    '  <maintainer email="probe@example.com">probe</maintainer><license>MIT</license>' \
    '  <export><nano_ros_provides kind="runner" name="qemu"/></export>' \
    '</package>' >"$user_pkg/package.xml"
sed '/^bin *=/d; /^# Stated by the release/d; /^# Cargo.toml to derive it/d' \
    "$packages/nros_runner_qemu/nros-runner.toml" >"$user_pkg/nros-runner.toml"
printf 'bin = "my-qemu"\n' >>"$user_pkg/nros-runner.toml"
printf '%s\n' '#!/bin/sh' \
    "touch '$mark'" \
    "exec '$packages/nros_runner_qemu/bin/nros-runner-qemu' \"\$@\"" >"$user_pkg/bin/my-qemu"
chmod +x "$user_pkg/bin/my-qemu"
export NROS_PACKAGE_PATH="$user_root"

# A real run, not `--dry-run`: a dry run prints the plan and only `--why`
# looks the runner up, reporting rather than refusing.
rc=0
out="$("${NROS_DEADLINE[@]}" 600 nros run "$node" --expect 'Publishing:' 2>&1)" || rc=$?
[ "$rc" -eq 3 ] || underlay_fail "an undeclared user runner/qemu: \`nros run\` exited $rc, want 3 (refused): $out"
nros_grep_q "my_qemu" <<<"$out" || underlay_fail "the refusal does not name my_qemu: $out"
echo "probe: undeclared user runner/qemu refused (exit 3)"
doc="$(nros doctor 2>&1 || true)"
nros_grep_q "my_qemu" <<<"$doc" || underlay_fail "\`nros doctor\` does not report the undeclared override: $doc"
echo "probe: nros doctor reports the undeclared override"

# Declared: a workspace states overrides in its root `nros.toml`
# `[workspace.overrides]` (RFC-0119 D4); a bringup's system.toml may not.
printf '\n[workspace.overrides]\nqemu = "%s"\n' "$user_pkg" >>"$ws/nros.toml"
# The override changes what the build resolves, so re-configure.
nros configure || underlay_fail "\`nros configure\` after declaring the override failed"
why="$(nros run "$node" --dry-run --why 2>&1)" || underlay_fail "a declared user runner/qemu: \`nros run $node --dry-run --why\` failed: $why"
nros_grep_q "my_qemu" <<<"$why" || underlay_fail "\`nros run --why\` does not name the declared my_qemu: $why"
doc="$(nros doctor 2>&1 || true)"
nros_grep_q "my_qemu" <<<"$doc" || underlay_fail "\`nros doctor\` does not report the declared override: $doc"
rm -f "$mark"
"${NROS_DEADLINE[@]}" 600 nros run "$node" --expect 'Publishing:' \
    || underlay_fail "\`nros run $node\` through the declared user runner did not reach 'Publishing:'"
[ -e "$mark" ] || underlay_fail "\`nros run\` passed but never executed the declared user runner"
echo "probe: a declared user runner/qemu replaces ours, and nros doctor reports it"
unset NROS_PACKAGE_PATH

# --- 2. nros run on the native talker -----------------------------------------
cd "$HOME" || underlay_fail "cd $HOME"
nros setup linux --rmw zenoh || underlay_fail "\`nros setup linux --rmw zenoh\` failed"
nros new native_ws --board linux --template talker \
    || underlay_fail "\`nros new native_ws --board linux --template talker\` failed"
cd native_ws || underlay_fail "the scaffold wrote no native_ws/"
# ROS 2 is sourced above: a new project's first configure generates std_msgs
# from it (issue 2275).
nros configure || underlay_fail "\`nros configure\` failed in the native talker"
"${NROS_DEADLINE[@]}" 900 nros run native_node --expect 'Publishing:' \
    || underlay_fail "\`nros run native_node --expect 'Publishing:'\` on the native talker failed"
why="$(nros run native_node --dry-run --why 2>&1)" || true
nros_grep_q "nros_runner_native" <<<"$why" || underlay_fail "the native talker ran through no nros_runner_native: $why"
echo "PROBE PASS: the release underlay — native and qemu runners prebuilt, a user runner/qemu refused undeclared and used declared, the native talker runs"
