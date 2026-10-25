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
        echo "probe: step $step printed its documented output; left running for the steps after it"
        return 0
    fi
    kill -- "-$pid" 2>/dev/null || true
    echo "probe: step $step printed its documented output"
}
