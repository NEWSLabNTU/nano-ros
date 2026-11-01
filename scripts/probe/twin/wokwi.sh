# shellcheck shell=bash
# Emulator kind `wokwi` for a twin probe track (phase-524 W4a, RFC-0115).
#
# Sourced into the probe script when the track's board states
# `[board.run] emulator = "wokwi:<board>"`. Same four functions as every kind
# (see renode.sh beside this file): nros_twin_up, nros_twin_start,
# nros_twin_down, nros_twin_console.
#
# Board facts (probe-tracks.json, from the registry row and the descriptor):
# NROS_TWIN_MACHINE is the Wokwi board, whose diagram part is
# `board-<machine>`; NROS_TWIN_LINK is `[board.run.wokwi] link`, the board's
# `<tx> <rx>` pins wired to Wokwi's serial monitor (the UART the image's zenoh
# link uses).
#
# Wokwi is a hosted simulator: `wokwi-cli` needs network access and a
# `WOKWI_CLI_TOKEN`, and every run spends that account's simulation minutes
# (free plan: 50 per rolling 30 days, 5 per session). The harness refuses the
# track before any container starts when the token is absent; the check here is
# the same refusal for a script run by hand.
#
# The link is phase-524 W1's rerun (docs/roadmap/data/phase-524-w1-wokwi-pico-
# rerun.log): `wokwi-cli --interactive` puts its stdin on the UART's RX and the
# serial monitor on its stdout, both byte-transparent, and `socat` turns that
# pair into the host pty <run-dir>/link that the router opens as `serial/`.
#
# Interim by design, like renode.sh: phase-528's runner packages replace it.

nros_twin_up() {
    local elf_dir="$1" run="$2"
    if [ -z "${WOKWI_CLI_TOKEN:-}" ]; then
        echo "PROBE FAIL: [UNMET PRECONDITION] the Wokwi twin needs WOKWI_CLI_TOKEN (in CI the repository secret of that name)" >&2
        return 1
    fi
    for tool in wokwi-cli socat; do
        if ! command -v "$tool" >/dev/null; then
            echo "PROBE FAIL: no \`$tool\` on PATH (wokwi-cli: \`nros setup $NROS_TWIN_BOARD\`; socat: [prereq.socat])" >&2
            return 1
        fi
    done
    local tx rx
    read -r tx rx <<<"$NROS_TWIN_LINK"
    mkdir -p "$run/project"
    printf '%s\n' \
        '{' \
        '  "version": 1,' \
        '  "parts": [' \
        "    { \"type\": \"board-$NROS_TWIN_MACHINE\", \"id\": \"mcu\", \"top\": 0, \"left\": 0, \"attrs\": {} }" \
        '  ],' \
        '  "connections": [' \
        "    [ \"mcu:$tx\", \"\$serialMonitor:RX\", \"\", [] ]," \
        "    [ \"mcu:$rx\", \"\$serialMonitor:TX\", \"\", [] ]" \
        '  ],' \
        '  "serialMonitor": { "display": "terminal", "baud": 115200 },' \
        '  "dependencies": {}' \
        '}' >"$run/project/diagram.json"
    # wokwi-cli resolves the image paths against the project directory.
    local rel
    rel="$(realpath --relative-to="$run/project" "$elf_dir")"
    printf '[wokwi]\nversion = 1\nelf = "%s"\nfirmware = "%s"\n' \
        "$rel/zephyr/zephyr.elf" "$rel/zephyr/zephyr.uf2" >"$run/project/wokwi.toml"
    rm -f "$run/in.fifo" "$run/out.fifo"
    mkfifo "$run/in.fifo" "$run/out.fifo"
    # RDWR opens, so neither FIFO open blocks before wokwi-cli is up.
    (exec setsid socat -d -d "PTY,link=$run/link,rawer,echo=0" \
        "OPEN:$run/out.fifo,rdwr!!OPEN:$run/in.fifo,rdwr") >"$run/socat.log" 2>&1 &
    echo $! >"$run/socat.pid"
    local i
    for i in $(seq 1 40); do
        [ -e "$run/link" ] && return 0
        sleep 0.25
    done
    tail -20 "$run/socat.log" >&2
    echo "PROBE FAIL: socat did not create the link pty $run/link" >&2
    return 1
}

nros_twin_start() {
    local run="$1"
    # One session, bounded well inside the free plan's 5 minutes: the verifier
    # stops it as soon as a sample is heard.
    (cd "$run/project" && exec setsid wokwi-cli -q --interactive \
        --timeout "${NROS_TWIN_TIMEOUT_MS:-150000}" . \
        <>"$run/in.fifo" 1<>"$run/out.fifo" 2>"$run/wokwi.log") &
    echo $! >"$run/wokwi.pid"
}

nros_twin_down() {
    local run="$1" f
    for f in "$run/wokwi.pid" "$run/socat.pid"; do
        [ -f "$f" ] || continue
        kill -TERM -- "-$(cat "$f")" 2>/dev/null || kill -TERM "$(cat "$f")" 2>/dev/null || true
    done
}

nros_twin_console() {
    local run="$1"
    # The console is the image's second UART, which the diagram does not wire;
    # what wokwi-cli itself said is the useful record.
    echo "=== wokwi-cli ==="
    head -c 4000 "$run/wokwi.log" 2>/dev/null || true
    echo
}
