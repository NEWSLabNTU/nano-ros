#!/usr/bin/env bash
# Are the cross Rust targets this tree builds for actually installed?
#
# Issue 0943. `just doctor` has verified this since issue 0833, reading the ONE
# list in `config/rust-targets.txt`. Nothing in the BUILD path asked, so a
# contributor who has not run `just workspace setup` does not learn about it
# from a doctor they had no reason to run — they learn about it from a fixture
# sweep that dies 20 minutes in.
#
# And the way it dies is the problem. Corrosion fails at cmake CONFIGURE with
# "Target armv8r-none-eabihf is not installed", make returns 2, and the tail of
# the build log holds only benign newlib `_close is not implemented` warnings.
# Issue 0833's own header describes exactly that shape; it fixed the doctor's
# copy of the list, which is not on the path anyone building fixtures walks.
#
# So this is the same list, asked at the moment it is about to matter. It reads
# `scripts/lib/rust-targets.sh` — never a second hand-authored copy, which is
# the defect 0833 existed to remove.
#
# `build-std` rows are deliberately excluded: those targets have no prebuilt
# std, are built from source per-invocation, and `rustup target list` never
# reports them.
set -uo pipefail
cd "$(dirname "$0")/.."

command -v rustup >/dev/null 2>&1 || {
    # Fail OPEN, matching `builder/preflight.rs`: a host managing Rust without
    # rustup cannot be probed this way, and guessing would block a working setup.
    echo "check-rust-targets-installed: SKIP (no rustup on PATH)"
    exit 0
}

source scripts/lib/rust-targets.sh

# Resolve the list ONCE, and check the status — issue 1466's class, reached
# through process substitution instead of argument position.
#
# This read `done < <(nros_rust_targets rustup)`, which discards the helper's
# status. `nros_rust_targets` returns 2 and prints NOTHING when
# `config/rust-targets.txt` is unreadable, so the loop read zero lines, `missing`
# stayed empty, and this gate printed `OK (0 rustup target(s) present)` and
# exited 0 — a green verdict over a list it never obtained. The count in that OK
# line was a SECOND invocation of the same failing helper, which is issue 1025's
# shape one lane over: one fact, derived twice, agreeing only while it works.
if ! targets="$(nros_rust_targets rustup)"; then
    echo "check-rust-targets-installed: cannot read the declared target list" >&2
    echo "  (config/rust-targets.txt). Refusing to report OK over a list that was" >&2
    echo "  never produced — that is what this gate looked like before issue 1466." >&2
    exit 1
fi

installed=" $(timeout 10s rustup target list --installed 2>/dev/null | tr '\n' ' ') "
declared=0
missing=()
while read -r t; do
    [ -z "$t" ] && continue
    declared=$((declared + 1))
    case "$installed" in
        *" $t "*) ;;
        *) missing+=("$t") ;;
    esac
done <<< "$targets"

if [ "$declared" -eq 0 ]; then
    echo "check-rust-targets-installed: config/rust-targets.txt declares no rustup" >&2
    echo "  target at all, so this gate examined nothing. That is not a pass." >&2
    exit 1
fi

if [ "${#missing[@]}" -eq 0 ]; then
    echo "check-rust-targets-installed: OK ($declared rustup target(s) present)"
    exit 0
fi

echo "missing rust target(s): ${missing[*]}"
echo "  Declared in config/rust-targets.txt. Without them corrosion fails at cmake"
echo "  CONFIGURE and the build log's tail shows only unrelated linker warnings."
exit 1
