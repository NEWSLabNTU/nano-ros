#!/usr/bin/env bash
# Make every pinned Rust toolchain complete, from `config/rust-toolchains.txt`
# (issue 1868). The ONE installer for the list `scripts/gen-rust-toolchains.py`
# derives from the tree's `rust-toolchain.toml` files.
#
#   bash scripts/lib/install-rust-toolchains.sh [--list <file>] [--set-default]
#
# Two callers, one behaviour:
#
#   * `ci/docker/ci-base/Dockerfile` — bakes every channel into the image, so a
#     merge-gating job never downloads one (`--set-default` makes line 1, the
#     root `rust-toolchain.toml`'s channel, the default toolchain).
#   * the gate fan-outs (`scripts/build/run-gates-parallel.sh`,
#     `check::workspace-all`) — run it SERIALLY before forking, so a toolchain
#     the image does not carry yet (a pin bump that has not republished the
#     image) is installed once, with a bounded retry, instead of by N rustup
#     proxies racing on `$RUSTUP_HOME/downloads` mid-gate.
#
# ASKS BEFORE IT FETCHES. `rustup toolchain list`, `component list --installed`
# and `target list --installed` read local state only (measured with
# `RUSTUP_DIST_SERVER` pointed at a dead port: all three answer). So on a
# complete image this script makes NO network request, and its presence in a
# merge-gating lane adds no network dependency; it only removes the one the
# rustup proxy would otherwise take at first use.
#
# A retry here does not hide the dependency (the issue's own warning): the
# image is what removes it, and a channel that is downloaded here is printed as
# such, so a lane that keeps downloading is visible in its log.
set -euo pipefail

here="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
list="$here/../../config/rust-toolchains.txt"
set_default=0
while [ $# -gt 0 ]; do
    case "$1" in
        --list) list="${2:?--list needs a file}"; shift 2 ;;
        --set-default) set_default=1; shift ;;
        *) echo "install-rust-toolchains: unknown argument: $1" >&2; exit 2 ;;
    esac
done
[ -r "$list" ] || { echo "install-rust-toolchains: cannot read $list" >&2; exit 2; }
list="$(cd "$(dirname "$list")" && pwd)/$(basename "$list")"

# Ask rustup from OUTSIDE any checkout, and with auto-install off. Measured: run
# under this tree, a plain `rustup toolchain list` resolves the ACTIVE toolchain
# (the root `rust-toolchain.toml`'s) and, when it is absent, auto-installs it on
# the spot — unretried, the exact fetch this script front-runs. Every call below
# names its toolchain explicitly, so nothing needs the directory override.
cd /
export RUSTUP_AUTO_INSTALL=0
command -v rustup >/dev/null 2>&1 || {
    echo "install-rust-toolchains: rustup not on PATH" >&2
    exit 1
}

attempts="${NROS_RUSTUP_ATTEMPTS:-3}"
# `retry <what> <cmd…>` — bounded: `attempts` tries, 15 s × n between them.
retry() {
    local what="$1"; shift
    local n=1
    while :; do
        if "$@"; then return 0; fi
        if [ "$n" -ge "$attempts" ]; then
            echo "install-rust-toolchains: $what failed after $n attempt(s)" \
                 "(a static.rust-lang.org fetch, issue 1868)" >&2
            return 1
        fi
        echo "install-rust-toolchains: $what failed (attempt $n/$attempts); retrying" >&2
        sleep $((15 * n))
        n=$((n + 1))
    done
}

# The host triple, WITHOUT asking `rustc`: under this checkout `rustc` is the
# rustup proxy, and the proxy would install the root toolchain on the spot —
# unretried, which is the very fetch this script exists to front-run.
case "$(uname -s)-$(uname -m)" in
    Linux-x86_64)  host=x86_64-unknown-linux-gnu ;;
    Linux-aarch64) host=aarch64-unknown-linux-gnu ;;
    Darwin-arm64)  host=aarch64-apple-darwin ;;
    Darwin-x86_64) host=x86_64-apple-darwin ;;
    *) host="$(uname -m)-unknown-linux-gnu" ;;
esac

# `missing_of <have> <host> <a,b,…>` — print each wanted name absent from
# `<have>` (rustup's `--installed` listing), one per line. A component may be
# listed bare (`rust-src`) or host-suffixed (`clippy-x86_64-unknown-linux-gnu`);
# a target always bare. Exact string comparison: `thumbv8m.main-none-eabi` has a
# dot, and a regex `.` would let a different name satisfy it.
# `has_entry <listing> <name> [<alt>]` — does any line's FIRST word equal
# <name> (or <alt>)? Plain string comparison over a captured listing: no `grep`
# whose error reads as a miss (issue 0726), no pipe whose SIGPIPE does (1077).
has_entry() {
    local line
    while IFS= read -r line; do
        line="${line%% *}"
        [ -n "$line" ] || continue
        if [ "$line" = "$2" ] || { [ -n "${3:-}" ] && [ "$line" = "$3" ]; }; then
            return 0
        fi
    done <<< "$1"
    return 1
}

missing_of() {
    local have="$1" h="$2" item
    for item in ${3//,/ }; do
        has_entry "$have" "$item" "$item-$h" || printf '%s\n' "$item"
    done
}

# The negative control, on the normal path (check-gate-selftests): the
# "is it installed?" answer is the whole reason this script can promise to make
# no request on a complete image, so it is re-proved on every run. Pure bash.
selftest() {
    local have got
    have="$(printf '%s\n' rust-src clippy-x86_64-unknown-linux-gnu thumbv8mXmain-none-eabi)"
    got="$(missing_of "$have" x86_64-unknown-linux-gnu \
        rust-src,clippy,miri,thumbv8m.main-none-eabi | tr '\n' ' ')"
    if [ "$got" != "miri thumbv8m.main-none-eabi " ]; then
        echo "install-rust-toolchains: self-test FAILED: missing_of answered '$got'," \
             "want 'miri thumbv8m.main-none-eabi '" >&2
        return 1
    fi
}
selftest || exit 1

first=""
while IFS=$'\t' read -r channel comps targets _sources; do
    case "$channel" in ''|'#'*) continue ;; esac
    [ -n "$first" ] || first="$channel"

    if ! has_entry "$(rustup toolchain list)" "$channel" "$channel-$host"; then
        echo "install-rust-toolchains: installing $channel (absent)"
        retry "rustup toolchain install $channel" \
            rustup toolchain install --profile minimal --no-self-update "$channel"
    fi

    # Two-step, never `toolchain install -c … -t …` on an existing toolchain:
    # that re-validates the whole toolchain and can roll back mid-way (see
    # `just workspace rust-pinned-toolchains`).
    if [ "$comps" != "-" ]; then
        mapfile -t missing < <(missing_of \
            "$(rustup component list --installed --toolchain "$channel")" "$host" "$comps")
        if [ ${#missing[@]} -gt 0 ]; then
            echo "install-rust-toolchains: $channel components: ${missing[*]}"
            retry "rustup component add (+$channel) ${missing[*]}" \
                rustup component add --toolchain "$channel" "${missing[@]}"
        fi
    fi
    if [ "$targets" != "-" ]; then
        mapfile -t missing < <(missing_of \
            "$(rustup target list --installed --toolchain "$channel")" "$host" "$targets")
        if [ ${#missing[@]} -gt 0 ]; then
            echo "install-rust-toolchains: $channel targets: ${missing[*]}"
            retry "rustup target add (+$channel) ${missing[*]}" \
                rustup target add --toolchain "$channel" "${missing[@]}"
        fi
    fi
done < "$list"

if [ "$set_default" = 1 ] && [ -n "$first" ]; then
    rustup default "$first"
fi
