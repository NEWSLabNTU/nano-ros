# shellcheck shell=bash
# PROBE-OWNED verification of the release underlay (phase-527 W6, RFC-0119
# D4) on the `installed` track. Appended after verify-installed-platform.sh
# for the default chapter (first-project.md), in the same shell: cwd is the
# chapter's `sensor_node` leaf, which `nros run` just booted on QEMU through
# the qemu runner the release ships PREBUILT under share/nros/packages/.
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
leaf="$PWD"
[ -f "$leaf/system.toml" ] || underlay_fail "expected the chapter's leaf as cwd, got $leaf"

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
why="$(nros run --dry-run --why 2>&1)" || underlay_fail "\`nros run --dry-run --why\` failed: $why"
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
out="$("${NROS_DEADLINE[@]}" 600 nros run --expect 'Publishing:' 2>&1)" || rc=$?
[ "$rc" -eq 3 ] || underlay_fail "an undeclared user runner/qemu: \`nros run\` exited $rc, want 3 (refused): $out"
nros_grep_q "my_qemu" <<<"$out" || underlay_fail "the refusal does not name my_qemu: $out"
echo "probe: undeclared user runner/qemu refused (exit 3)"
doc="$(nros doctor 2>&1 || true)"
nros_grep_q "my_qemu" <<<"$doc" || underlay_fail "\`nros doctor\` does not report the undeclared override: $doc"
echo "probe: nros doctor reports the undeclared override"

# Declared: the leaf's root system.toml (a leaf has no nros.toml; W2c's
# transitional fallback, which prints a note naming nros.toml).
printf '\n[overrides]\nqemu = "%s"\n' "$user_pkg" >>"$leaf/system.toml"
# `system.toml` is a SystemModel input, so the edit needs a re-sync.
nros sync || underlay_fail "\`nros sync\` after declaring the override failed"
why="$(nros run --dry-run --why 2>&1)" || underlay_fail "a declared user runner/qemu: \`nros run --dry-run --why\` failed: $why"
nros_grep_q "my_qemu" <<<"$why" || underlay_fail "\`nros run --why\` does not name the declared my_qemu: $why"
doc="$(nros doctor 2>&1 || true)"
nros_grep_q "my_qemu" <<<"$doc" || underlay_fail "\`nros doctor\` does not report the declared override: $doc"
rm -f "$mark"
"${NROS_DEADLINE[@]}" 600 nros run --expect 'Publishing:' \
    || underlay_fail "\`nros run\` through the declared user runner did not reach 'Publishing:'"
[ -e "$mark" ] || underlay_fail "\`nros run\` passed but never executed the declared user runner"
echo "probe: a declared user runner/qemu replaces ours, and nros doctor reports it"
unset NROS_PACKAGE_PATH

# --- 2. nros run on the native talker -----------------------------------------
cd "$HOME" || underlay_fail "cd $HOME"
nros setup linux --rmw zenoh || underlay_fail "\`nros setup linux --rmw zenoh\` failed"
nros new native_talker --board linux --template talker \
    || underlay_fail "\`nros new native_talker --board linux --template talker\` failed"
cd native_talker || underlay_fail "the scaffold wrote no native_talker/"
nros sync || underlay_fail "\`nros sync\` failed in the native talker"
"${NROS_DEADLINE[@]}" 900 nros run --expect 'Publishing:' \
    || underlay_fail "\`nros run --expect 'Publishing:'\` on the native talker failed"
why="$(nros run --dry-run --why 2>&1)" || true
nros_grep_q "nros_runner_native" <<<"$why" || underlay_fail "the native talker ran through no nros_runner_native: $why"
echo "PROBE PASS: the release underlay — native and qemu runners prebuilt, a user runner/qemu refused undeclared and used declared, the native talker runs"
