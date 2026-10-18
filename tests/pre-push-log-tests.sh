#!/usr/bin/env bash
# issue 1964 — the pre-push hook keeps its fast-gate output, outside the checkout.
#
# Exercises `scripts/lib/prepush-log.sh` directly (the hook itself costs ~20 s,
# almost all `issue-ids-check.sh`): the verdict is the command's, the log holds
# both streams, the per-gate timings file is routed beside it, an unwritable log
# dir never changes the verdict, retention is bounded, and the hook actually
# routes `just check fast` through the helper. Buildless, tmpdir, <1 s.

set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
lib="$repo_root/scripts/lib/prepush-log.sh"
hook="$repo_root/.githooks/pre-push"
failures=0
checks=0
ok()   { echo "  [ok]   $1"; checks=$((checks + 1)); }
fail() { echo "  [FAIL] $1" >&2; failures=$((failures + 1)); checks=$((checks + 1)); }

[ -r "$lib" ] || { echo "FATAL: $lib missing" >&2; exit 2; }
# shellcheck source=scripts/lib/prepush-log.sh
. "$lib"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
unset NROS_GATE_TIMINGS_FILE NROS_PREPUSH_LOG_KEEP
export NROS_PREPUSH_LOG_DIR="$work/logs"

newest_log() { ls -1 "$NROS_PREPUSH_LOG_DIR" 2>/dev/null | grep '\.log$' | sort | tail -1; }

# 1. A red command: its status survives the tee, both streams are kept.
#    The markers are COMPUTED (OUT-42 / ERR-54) so the log header, which echoes
#    the command line, cannot satisfy the stream assertions by itself. And
#    pipefail is OFF: the helper is sourced into callers either way, so the
#    status must come from PIPESTATUS, not from a caller's shell option.
err="$(set +o pipefail
       nros_prepush_run_logged bash -c 'echo "OUT-$((6*7))"; echo "ERR-$((6*9))" >&2; exit 3' 2>&1 >/dev/null)"
rc=$?
[ "$rc" -eq 3 ] && ok "a red command's exit status is returned (3)" \
    || fail "a red command returned $rc, not 3 — the tee swallowed the verdict"
log="$(newest_log)"
if [ -n "$log" ]; then
    body="$(cat "$NROS_PREPUSH_LOG_DIR/$log")"
    case "$body" in *OUT-42*) ok "stdout is in the log" ;; *) fail "stdout missing from $log" ;; esac
    case "$body" in *ERR-54*) ok "stderr is in the log" ;; *) fail "stderr missing from $log" ;; esac
    case "$body" in *"# exit: 3"*) ok "the log records the exit status" ;; *) fail "no '# exit: 3' in $log" ;; esac
    case "$err" in *"$NROS_PREPUSH_LOG_DIR/$log"*) ok "the log path is printed" ;;
        *) fail "the log path was not printed to stderr" ;; esac
    case "$err" in *OUT-42*ERR-54*|*ERR-54*OUT-42*) ok "the output still reaches the terminal" ;;
        *) fail "the output no longer reaches the terminal" ;; esac
else
    fail "no log written under $NROS_PREPUSH_LOG_DIR"
fi
rm -rf "$NROS_PREPUSH_LOG_DIR"

# 2. A green command returns 0.
nros_prepush_run_logged true 2>/dev/null
rc=$?
[ "$rc" -eq 0 ] && ok "a green command returns 0" || fail "a green command returned $rc"
rm -rf "$NROS_PREPUSH_LOG_DIR"

# 3. The runner's per-gate verdicts land beside the log.
seen="$(nros_prepush_run_logged bash -c 'printf "%s\n" "$NROS_GATE_TIMINGS_FILE"' 2>&1 | head -1)"
case "$seen" in
    "$NROS_PREPUSH_LOG_DIR"/*.gates.tsv) ok "NROS_GATE_TIMINGS_FILE is routed beside the log" ;;
    *) fail "NROS_GATE_TIMINGS_FILE was '$seen', not a .gates.tsv in the log dir" ;;
esac
rm -rf "$NROS_PREPUSH_LOG_DIR"

# 4. An unwritable log dir never changes the verdict, and the command still runs.
: >"$work/a-file"
out="$(NROS_PREPUSH_LOG_DIR="$work/a-file/sub" nros_prepush_run_logged bash -c 'echo RAN; exit 5' 2>/dev/null)"
rc=$?
[ "$rc" -eq 5 ] && [ "$out" = "RAN" ] && ok "an unwritable log dir runs the command unlogged, verdict intact" \
    || fail "unwritable log dir: rc=$rc out='$out' (want 5 / RAN)"

# 5. Retention: only the newest NROS_PREPUSH_LOG_KEEP runs remain.
mkdir -p "$NROS_PREPUSH_LOG_DIR"
for s in 20000101T000000Z-1 20000101T000001Z-2 20000101T000002Z-3; do
    : >"$NROS_PREPUSH_LOG_DIR/$s.log"; : >"$NROS_PREPUSH_LOG_DIR/$s.gates.tsv"
done
NROS_PREPUSH_LOG_KEEP=2 nros_prepush_run_logged true 2>/dev/null
left="$(ls -1 "$NROS_PREPUSH_LOG_DIR" | grep '\.log$' | sort | tr '\n' ' ')"
case "$left" in
    "20000101T000002Z-3.log "*Z-*.log" ")
        [ ! -e "$NROS_PREPUSH_LOG_DIR/20000101T000000Z-1.gates.tsv" ] \
            && ok "retention keeps the newest 2 and drops an old run's .gates.tsv too" \
            || fail "retention left an evicted run's .gates.tsv behind" ;;
    *) fail "retention kept: $left (want the newest 2)" ;;
esac
rm -rf "$NROS_PREPUSH_LOG_DIR"

# 6. The hook routes the fast tier through the helper — otherwise all of the
#    above is true of a function nothing calls.
hook_text="$(cat "$hook" 2>/dev/null)" || hook_text=""
if [[ "$hook_text" == *'logged=(nros_prepush_run_logged)'* ]] \
   && [[ "$hook_text" == *'${logged[@]+"${logged[@]}"} just check fast'* ]]; then
    ok "the hook runs \`just check fast\` through nros_prepush_run_logged"
else
    fail "$hook no longer runs \`just check fast\` through nros_prepush_run_logged"
fi

echo ""
if [ "$failures" -eq 0 ]; then
    echo "pre-push-log: $checks check(s) passed"
    exit 0
fi
echo "pre-push-log: $failures of $checks check(s) FAILED" >&2
exit 1
