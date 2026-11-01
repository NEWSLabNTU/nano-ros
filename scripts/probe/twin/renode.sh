# shellcheck shell=bash
# Emulator kind `renode` for a twin probe track (phase-524 W4a, RFC-0115).
#
# Sourced into the probe script (`extract-book-steps.py --prelude`) when the
# track's board states `[board.run] emulator = "renode:<machine>"`; the harness
# picks this file by that kind, so a board names no script. Every kind's file
# defines the same four functions, and verify-installed-twin.sh calls only
# those:
#
#   nros_twin_up <elf-dir> <run-dir>  start the emulator PAUSED, with the image's
#                                     serial link on the host pty <run-dir>/link;
#                                     return once that pty exists
#   nros_twin_start <run-dir>         start the simulation
#   nros_twin_down <run-dir>          stop what nros_twin_up started
#   nros_twin_console <run-dir>       print what the image's console said
#
# Board facts arrive as NROS_TWIN_* (probe-tracks.json: the registry row and
# the board descriptor, RFC-0114 D12): NROS_TWIN_MACHINE is the emulator's
# machine word, which is Renode's platform file `platforms/boards/<m>.repl`;
# NROS_TWIN_LINK and NROS_TWIN_CONSOLE are `[board.run.renode] link` and
# `console`, the `sysbus` peripherals carrying the zenoh link and the console.
#
# Interim by design: phase-528's runner contract (RFC-0119) moves this into a
# Renode runner package that `nros run` drives (phase-524 W4b); this file
# is what that package replaces, so it carries no logic a runner would not.
#
# The pattern is phase-524 W1's spike (docs/roadmap/data/phase-524-w1-renode-
# nucleo.log): Renode creates the pty itself (`CreateUartPtyTerminal`), so no
# bridge is needed. Renode writes `~/.config/renode` even with `--config`
# (measured), so its HOME is the run dir.

nros_twin_up() {
    local elf_dir="$1" run="$2"
    local renode_bin renode_dir port=33524
    if ! renode_bin="$(command -v renode)"; then
        echo "PROBE FAIL: no \`renode\` on PATH; \`nros setup $NROS_TWIN_BOARD\` installs it ([tool.renode])" >&2
        return 1
    fi
    # The portable build resolves `@platforms/...` against its own directory.
    renode_dir="$(dirname "$(readlink -f "$renode_bin")")"
    mkdir -p "$run/home"
    printf '%s\n' \
        "mach create \"twin\"" \
        "machine LoadPlatformDescription @platforms/boards/$NROS_TWIN_MACHINE.repl" \
        "emulation CreateUartPtyTerminal \"zlink\" \"$run/link\" true" \
        "connector Connect sysbus.$NROS_TWIN_LINK zlink" \
        ${NROS_TWIN_CONSOLE:+"sysbus.$NROS_TWIN_CONSOLE CreateFileBackend @$run/console.log true"} \
        "sysbus LoadELF @$elf_dir/zephyr/zephyr.elf" >"$run/twin.resc"
    (cd "$renode_dir" && HOME="$run/home" XDG_CONFIG_HOME="$run/home/.config" \
        exec setsid "$renode_bin" --disable-gui --plain -P "$port" "$run/twin.resc") \
        >"$run/renode.log" 2>&1 &
    echo $! >"$run/renode.pid"
    echo "$port" >"$run/monitor.port"
    local i
    for i in $(seq 1 120); do
        [ -e "$run/link" ] && return 0
        kill -0 "$(cat "$run/renode.pid")" 2>/dev/null || break
        sleep 0.5
    done
    tail -30 "$run/renode.log" >&2
    echo "PROBE FAIL: Renode did not create the link pty $run/link" >&2
    return 1
}

nros_twin_start() {
    local run="$1" port
    port="$(cat "$run/monitor.port")"
    local i
    for i in $(seq 1 60); do
        if exec 3<>"/dev/tcp/127.0.0.1/$port" 2>/dev/null; then
            printf 'start\n' >&3
            sleep 1
            exec 3>&-
            return 0
        fi
        sleep 0.5
    done
    echo "PROBE FAIL: Renode's monitor on 127.0.0.1:$port never answered" >&2
    return 1
}

nros_twin_down() {
    local run="$1" p
    [ -f "$run/renode.pid" ] || return 0
    p="$(cat "$run/renode.pid")"
    kill -TERM -- "-$p" 2>/dev/null || kill -TERM "$p" 2>/dev/null || true
}

nros_twin_console() {
    local run="$1"
    echo "=== the image's console (sysbus.${NROS_TWIN_CONSOLE:-?}) ==="
    head -c 4000 "$run/console.log" 2>/dev/null || true
    echo
}
