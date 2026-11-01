# shellcheck shell=bash
# PROBE-OWNED verification for a TWIN track on the `installed` track (phase-524
# W4a, RFC-0115): a board with an emulator twin, run with no chapter of its
# own. Appended after the book-extracted steps by run-bootstrap-probe.sh; runs
# in the same shell.
#
# What ran before this file: installation.md's install of the release asset,
# then the Zephyr chapter's setup blocks (`variant=zephyr-twin`: host
# prerequisites, `nros setup <board> --rmw zenoh`, the west workspace with the
# board's HAL added), which leave the shell in that workspace. What this file
# adds is the twin smoke run (scripts/probe/twin/smoke.sh, with the emulator
# kind's script before it): build the board's serial C talker through the leaf
# road, boot it in the twin, and hear one message from stock ROS 2.
#
# The chapter that will own these steps is phase-524 W4b's; it waits on `nros
# run` (phase-528 W1/W3/W4). Until then the steps are the probe's.

echo "=== probe verify: $NROS_TWIN_BOARD in its twin ($NROS_TWIN_EMULATOR) ==="
# The cargo `nros setup` installed (installation.md): the C image links the
# nano-ros runtime, a Rust staticlib.
if [ -f "$HOME/.cargo/env" ]; then
    . "$HOME/.cargo/env"
fi
twin_ws="$PWD"
nros_twin_provision "$twin_ws"
nros_twin_build "$twin_ws" "$twin_ws/twin_app" "$twin_ws/build-twin"
nros_twin_hear "$twin_ws/build-twin" /tmp/nros-twin
