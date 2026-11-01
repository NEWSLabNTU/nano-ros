# shellcheck shell=bash
# The twin smoke run of a twin probe track (phase-524 W4a, RFC-0115): build
# the board's serial C talker through the leaf road, boot it in the board's
# emulator twin, put ROS's `rmw_zenohd` on the twin's serial link, and assert
# that `ros2 topic echo` hears one message. Boot, link up, one message heard.
#
# Sourced into the probe script as a prelude, after the emulator kind's file
# (scripts/probe/twin/<kind>.sh, which defines nros_twin_up / _start / _down /
# _console). Runs in the probe shell, after the Zephyr chapter's setup blocks
# have provisioned the board and made the west workspace, so ZEPHYR_BASE and
# ZEPHYR_SDK_INSTALL_DIR are set and `nros` is the installed release.
#
# Board facts, all from probe-tracks.json (the registry's `[[probe]]` row and
# the board descriptor it names, RFC-0114 D12), assigned by the harness:
#   NROS_TWIN_BOARD       the board key (`nros-board.toml` `names`)
#   NROS_TWIN_WEST_BOARD  `[board.zephyr] west_board`
#   NROS_TWIN_IMAGE_TOML  the `[image.twin]` tables the track's image adds to
#                         the scaffold: board, rmw, the serial locator of the
#                         twin's link, and the RAM knobs (issue 2194)
#
# What the run asserts is output, never a literal of this script's own: the
# router's readiness line is the Zephyr chapter's `probe-expect=60` block, and
# the sample is `ros2 topic echo`'s rendering of the talker's payload, the
# output ros2-interop.md documents. Both are matched by nros_probe_run_expect
# (probe-run.sh), the helper the chapter's own run steps use.
NROS_TWIN_ROUTER_READY="Started Zenoh router with id"
NROS_TWIN_HEARD="data: 'Hello World:"

# The board's west-workspace provisioning (its Zephyr line's patch set and its
# Rust target): what the chapter's own `nros setup board` step does for the
# QEMU board it documents.
nros_twin_provision() {
    local ws="$1"
    nros setup board "$NROS_TWIN_BOARD" --zephyr-workspace "$ws"
}

# Scaffold the C talker, add the track's image, build it through the leaf
# road (west + the installed SDK's Zephyr module). The ELF is
# <build-dir>/zephyr/zephyr.elf. NROS_TWIN_EXTRA_MODULES (unset in the probe)
# names module directories for a west workspace whose manifest predates the
# board's HAL, the way a contributor reruns this by hand.
nros_twin_build() {
    local ws="$1" app="$2" build="$3"
    (
        mkdir -p "$(dirname "$app")"
        (cd "$(dirname "$app")" && nros new "$(basename "$app")" --platform zephyr --lang c) || exit 1
        cd "$ws" || exit 1
        # A single-package leaf builds one image: the scaffold's own `zephyr`
        # image stays, and the system names the track's as the one to build.
        sed -i '/^\[system\]$/a default_images = ["twin"]' "$app/system.toml"
        printf '\n%s\n' "$NROS_TWIN_IMAGE_TOML" >>"$app/system.toml"
        west build -b "$NROS_TWIN_WEST_BOARD" -d "$build" "$app" -- \
            -DNROS_IMAGE=twin \
            -DZEPHYR_EXTRA_MODULES="$(nros sdk-root)${NROS_TWIN_EXTRA_MODULES:+;$NROS_TWIN_EXTRA_MODULES}"
    )
}

# Boot the ELF in the twin and hear one message from stock ROS 2.
nros_twin_hear() {
    local build="$1" run="$2" port="${NROS_TWIN_ROUTER_PORT:-7447}"
    rm -rf "$run"
    mkdir -p "$run"
    nros_twin_up "$build" "$run" || exit 1
    # shellcheck disable=SC2064  # expand $run now
    trap "nros_twin_down '$run'" EXIT

    printf '%s\n' \
        'source /opt/ros/humble/setup.bash' \
        "ZENOH_CONFIG_OVERRIDE='listen/endpoints=[\"serial/$run/link#baudrate=115200\",\"tcp/127.0.0.1:$port\"];scouting/multicast/enabled=false' \\" \
        '    exec ros2 run rmw_zenoh_cpp rmw_zenohd' >"$run/router.sh"
    nros_probe_run_expect twin-router 60 1 "$run/router.sh" "$NROS_TWIN_ROUTER_READY"

    nros_twin_start "$run" || exit 1
    echo "probe: twin started ($NROS_TWIN_EMULATOR)"

    # `--no-daemon` (issue 1333): a ros2 daemon is keyed on the domain alone,
    # so one started elsewhere answers with ITS discovery config, not this one.
    printf '%s\n' \
        'source /opt/ros/humble/setup.bash' \
        "RMW_IMPLEMENTATION=rmw_zenoh_cpp ZENOH_ROUTER_CHECK_ATTEMPTS=-1 \\" \
        "ZENOH_CONFIG_OVERRIDE='connect/endpoints=[\"tcp/127.0.0.1:$port\"];scouting/multicast/enabled=false' \\" \
        '    exec ros2 topic echo --no-daemon /chatter std_msgs/msg/String' >"$run/echo.sh"
    if ! (nros_probe_run_expect twin-echo "${NROS_TWIN_HEAR_SECS:-120}" 0 \
        "$run/echo.sh" "$NROS_TWIN_HEARD"); then
        nros_twin_console "$run"
        echo "PROBE FAIL: $NROS_TWIN_BOARD in $NROS_TWIN_EMULATOR: no sample reached ros2 topic echo" >&2
        exit 1
    fi
    nros_twin_console "$run"
    nros_twin_down "$run"
    trap - EXIT
    echo "probe: $NROS_TWIN_BOARD in $NROS_TWIN_EMULATOR: boot, link up, one message heard"
}
