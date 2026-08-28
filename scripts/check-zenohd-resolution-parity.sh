#!/usr/bin/env bash
# Issue 0653 — the SHELL router resolver answers the shared resolution table.
#
# `scripts/dev/zenohd.sh` (`just <plat> zenohd`) and
# `nros_tests::process::ros_zenohd_path` (the test harness) have to resolve the
# SAME router: a contributor debugging a lane by hand and the lane itself must
# start the same process. Neither can call the other, so the agreement came from
# a comment saying "the two must agree" — and they drifted anyway, both looking
# only under `/opt/ros` while `AMENT_PREFIX_PATH` is what the sourced
# environment actually says. On a host that built ROS from source, or one using
# this repo's own Arch/Fedora/NixOS distrobox route, that is not `/opt/ros`, so
# you could source a working ROS and still be told there is no router.
#
# The fix for two implementations is one TABLE, not a third implementation:
# `scripts/dev/zenohd-resolution-cases.tsv` is written once and answered twice —
# here for the shell, and by `zenohd_resolution_matches_the_shared_table` for
# the Rust. A row is not comparable text, it is a behaviour, so the two are
# checked by BEHAVING rather than by diffing source in two languages.
#
# Run: bash scripts/check-zenohd-resolution-parity.sh
set -euo pipefail
cd "$(dirname "$0")/.."

# issue 0726 — the two staleness probes below exit 2 on ABSENCE, and `grep -q`
# cannot tell absence from a grep that did not run. That direction is the
# quieter one: the gate declares itself stale and stops checking parity, which
# reads as maintenance rather than as a tool failure. `nros_grep_q` exits 2 with
# a message that says which it was.
# shellcheck source=scripts/lib/grep-q.sh
source scripts/lib/grep-q.sh

RESOLVER="scripts/dev/zenohd.sh"
TABLE="scripts/dev/zenohd-resolution-cases.tsv"
RUST="packages/testing/nros-tests/src/process.rs"

for f in "$RESOLVER" "$TABLE" "$RUST"; do
    [ -f "$f" ] || { echo "check-zenohd-resolution-parity: $f missing — this gate is stale" >&2; exit 2; }
done

# Staleness. A gate that has stopped watching must say so, not pass: this one's
# sibling (`check-zenohd-spawn-sites`) caught its own rot exactly this way when
# the helper was renamed in phase-362.
nros_grep_q 'AMENT_PREFIX_PATH' "$RESOLVER" || {
    echo "check-zenohd-resolution-parity: $RESOLVER no longer reads AMENT_PREFIX_PATH — stale" >&2; exit 2; }
nros_grep_q 'zenohd-resolution-cases.tsv' "$RUST" || {
    echo "check-zenohd-resolution-parity: $RUST no longer reads the shared table — stale" >&2; exit 2; }

# Every case runs with a router PREPENDED to PATH (`@/bin/rmw_zenohd`), because
# the property under test is partly a NEGATIVE one: PATH must never be consulted,
# so the rows that expect nothing have to be run in an environment where a PATH
# search WOULD have found something. `rejects-a-path-router` is that row.
#
# Resolved absolutely all the same: an earlier revision replaced PATH outright
# per case, and the prefix assignment applies to the command LOOKUP as well as
# the command, so `PATH=... bash -c` could not find `bash` and every case
# "resolved nothing".
BASH_BIN="$(command -v bash)"

root="$(mktemp -d)"
trap 'rm -rf "$root"' EXIT

rel="lib/rmw_zenoh_cpp/rmw_zenohd"
for p in overlay underlay optros/humble optros/jazzy; do
    mkdir -p "$root/$p/$(dirname "$rel")"
    : > "$root/$p/$rel"; chmod +x "$root/$p/$rel"
done
mkdir -p "$root/bin" "$root/explicit" "$root/nothing"
: > "$root/bin/rmw_zenohd"; chmod +x "$root/bin/rmw_zenohd"
: > "$root/explicit/router"; chmod +x "$root/explicit/router"

expand() { printf '%s' "${1//@/$root}" | sed "s|\$rel|$rel|g"; }

fails=0 ran=0 expecting=0
# Tab is IFS WHITESPACE, so `IFS=$'\t' read` collapses a run of tabs into one
# delimiter and silently shifts every field after an empty column — which is most
# of this table. Translating to a non-whitespace separator first is what makes an
# empty column mean "empty" instead of "absent".
while IFS=$'\x1f' read -r name explicit ament distro optros expected; do
    case "$name" in ''|'#'*) continue ;; esac
    # A row name is a slug. Anything else means the field split did not happen —
    # which is not hypothetical: `IFS=$'\t'` collapsed empty columns here, and
    # before that a `tr` escape this shell does not understand left the whole row
    # in `$name`. Both made every case "pass", because an unparsed row compares
    # an empty expectation against an empty result. A gate that cannot fail is
    # worse than no gate, so the parse is asserted before the behaviour is.
    case "$name" in
        *[!a-z0-9-]*)
            echo "  FAIL row did not split into fields: '$name'" >&2
            fails=$((fails + 1)); ran=$((ran + 1)); continue ;;
    esac
    ran=$((ran + 1))
    explicit="$(expand "$explicit")"; ament="$(expand "$ament")"
    expected="$(expand "$expected")"; optros="$(expand "$optros")"

    got="$(
        NROS_RMW_ZENOHD="$explicit" PATH="$root/bin:$PATH" AMENT_PREFIX_PATH="$ament" \
        ROS_DISTRO="$distro" NROS_ZENOHD_OPT_ROS="$optros" \
        "$BASH_BIN" -c 'source scripts/dev/zenohd.sh; nros_zenohd_bin' 2>/dev/null || true
    )"

    [ -n "$expected" ] && expecting=$((expecting + 1))
    if [ "$got" = "$expected" ]; then
        echo "  ok   $name -> ${expected:-<none, as required>}"
    else
        echo "  FAIL $name: shell resolved '${got:-<none>}', table says '${expected:-<none>}'" >&2
        fails=$((fails + 1))
    fi
done < <(tr '\t' '\037' < "$TABLE")

[ "$ran" -gt 0 ] || { echo "check-zenohd-resolution-parity: the table has NO rows — stale" >&2; exit 2; }
# Most rows must EXPECT a router. An all-empty expectation column is the residue
# of a parse failure that got past the check above, and it would green trivially.
[ "$expecting" -ge 5 ] || {
    echo "check-zenohd-resolution-parity: only $expecting of $ran row(s) expect a router — \
the table or its parse has rotted; this gate would pass on anything" >&2
    exit 2
}
if [ "$fails" -ne 0 ]; then
    echo "check-zenohd-resolution-parity: $fails of $ran case(s) FAILED" >&2
    exit 1
fi
echo "check-zenohd-resolution-parity: OK ($ran cases; the Rust side answers the same table)"
