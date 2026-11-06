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
# phase-537 W4: both twin boards now replay their own starters (the Nucleo's
# and the Pico W's `book_run`), which run the twin through `nros run`, so no
# track takes this path without `twin_build` today.
#
# A track with `twin_build` is the exception: its chapter builds the image
# (and documents `west flash` for the board), so this file only boots it.

echo "=== probe verify: $NROS_TWIN_BOARD in its twin ($NROS_TWIN_EMULATOR) ==="
# The cargo `nros setup` installed (installation.md): the C image links the
# nano-ros runtime, a Rust staticlib.
if [ -f "$HOME/.cargo/env" ]; then
    . "$HOME/.cargo/env"
fi
twin_ws="$PWD"
if [ -n "${NROS_TWIN_CHAPTER_BUILD:-}" ]; then
    # The chapter built the image itself (zephyr-west-app.md's flash section,
    # whose steps stop at `west flash`): boot THAT image, not a scaffold.
    if [ ! -f "$twin_ws/$NROS_TWIN_CHAPTER_BUILD/zephyr/zephyr.elf" ]; then
        echo "PROBE FAIL: the chapter left no $twin_ws/$NROS_TWIN_CHAPTER_BUILD/zephyr/zephyr.elf" >&2
        exit 1
    fi
    nros_twin_hear "$twin_ws/$NROS_TWIN_CHAPTER_BUILD" /tmp/nros-twin
else
    nros_twin_provision "$twin_ws"
    nros_twin_build "$twin_ws" "$twin_ws/twin_app" "$twin_ws/build-twin"
    nros_twin_hear "$twin_ws/build-twin" /tmp/nros-twin
fi
