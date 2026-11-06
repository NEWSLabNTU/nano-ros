# shellcheck shell=bash
# Sourced into every extracted probe script (extract-book-steps.py --prelude),
# ahead of the book's steps — the probe container has no checkout to source a
# library from, so the helper travels inside the script.
#
# nros_probe_run_expect — run a book step that NEVER EXITS and assert its
# documented output (phase-517 W10).
#
# A chapter's Run step boots QEMU (or a host process) that publishes until it
# is killed, and the chapter shows what it prints in an output block beside it.
# The extractor turns a `probe=NN probe-run=SECS` block into a call to this
# helper, with the step body in a file and every line of the matching
# `probe-expect=NN` block as an expected substring. So the assertion IS the
# documented output: a chapter whose command prints something else, or nothing,
# fails its track on the step that lies.
#
#   nros_probe_run_expect <step> <secs> <keep> <body-file> <expected line>...
#
# The body runs in its own session (setsid), so QEMU and anything else it
# starts is killed as one process group when the expectation is met or the
# deadline passes — unless <keep> is 1 (`probe-keep` on the block): a step the
# chapter starts for LATER steps, the zenoh router, is left running once it has
# printed its readiness line, and dies with the container. The body inherits
# the probe shell's cwd and exported environment, exactly like a second
# terminal opened in the same directory.
nros_probe_run_expect() {
    local step="$1" secs="$2" keep="$3" body="$4"
    shift 4
    local log="/tmp/probe-run-$step.log"
    local pending=("$@")
    : >"$log"
    setsid bash "$body" >"$log" 2>&1 </dev/null &
    local pid=$!
    local deadline=$((SECONDS + secs))
    while :; do
        local left=() line rc
        for line in "${pending[@]}"; do
            rc=0
            nros_grep_q -F -- "$line" "$log" || rc=$?
            [ "$rc" -eq 0 ] || left+=("$line")
        done
        pending=(${left[@]+"${left[@]}"})
        [ "${#pending[@]}" -eq 0 ] && break
        if ! kill -0 "$pid" 2>/dev/null; then
            tail -60 "$log" >&2
            echo "PROBE FAIL: step $step exited before printing its documented output; missing: ${pending[*]}" >&2
            exit 1
        fi
        if ((SECONDS >= deadline)); then
            kill -- "-$pid" 2>/dev/null || true
            tail -60 "$log" >&2
            echo "PROBE FAIL: step $step did not print its documented output within ${secs}s; missing: ${pending[*]}" >&2
            exit 1
        fi
        sleep 2
    done
    head -40 "$log"
    if [ "$keep" = 1 ]; then
        # Recorded so a probe-owned check after the chapter can stop it
        # (verify-installed-underlay.sh needs the router's port back).
        NROS_PROBE_KEPT_PGIDS="${NROS_PROBE_KEPT_PGIDS:-} $pid"
        echo "probe: step $step printed its documented output; left running for the steps after it"
        return 0
    fi
    kill -- "-$pid" 2>/dev/null || true
    echo "probe: step $step printed its documented output"
}

# nros_probe_stop_kept — close the terminals a chapter left open (phase-537
# W5). A Tutorial track replays several chapters in one shell, and each
# chapter's reader starts with the last one's `nros run` stopped. Every
# process group a `probe-keep` step left running is killed and waited for, so
# the next chapter's router finds its port free.
nros_probe_stop_kept() {
    local pgid
    for pgid in ${NROS_PROBE_KEPT_PGIDS:-}; do
        kill -- "-$pgid" 2>/dev/null || true
        for _ in 1 2 3 4 5 6 7 8 9 10; do
            kill -0 -- "-$pgid" 2>/dev/null || break
            sleep 1
        done
    done
    NROS_PROBE_KEPT_PGIDS=""
}

# nros_probe_clock_start / nros_probe_clock_report — the field budget's wall
# clock (phase-525 W4, RFC-0116 D1: 30 minutes from an unopened box to
# `ros2 topic echo`). extract-book-steps.py --clock-from places the start
# before the first step that runs `nros setup`, and a report after every step
# from there on, so the time to each asserted line is printed as the run goes
# and the step that ends past PROBE_CLOCK_BUDGET (seconds) fails the track.
# The report carries the host's load average, because a time measured on a
# loaded host says less than its number.
nros_probe_clock_start() {
    PROBE_CLOCK_T0="$(date +%s)"
    echo "probe: clock started at step $1 (the first nros setup); budget ${PROBE_CLOCK_BUDGET:-none}s"
}
nros_probe_clock_report() {
    local step="$1" elapsed load
    elapsed=$(($(date +%s) - PROBE_CLOCK_T0))
    load="$(cut -d' ' -f1-3 /proc/loadavg 2>/dev/null || echo '?')"
    printf 'probe: clock: step %s done %dm%02ds after the first nros setup (load %s)\n' \
        "$step" $((elapsed / 60)) $((elapsed % 60)) "$load"
    if [ -n "${PROBE_CLOCK_BUDGET:-}" ] && [ "$elapsed" -gt "$PROBE_CLOCK_BUDGET" ]; then
        echo "PROBE FAIL: step $step ended ${elapsed}s after the first nros setup, past the ${PROBE_CLOCK_BUDGET}s field budget" >&2
        exit 1
    fi
}
