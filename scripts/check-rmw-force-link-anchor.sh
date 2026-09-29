#!/usr/bin/env bash
#
# Issue 0330 / 0155 / 0163 — a pure-Rust image must FORCE-LINK its RMW backend.
#
# On a Rust-only image the Zephyr module emits a weak `nros_rmw_<x>_register`
# and calls it only if it resolves. The strong definition is the backend crate's
# `#[no_mangle]` export, and rustc's staticlib DCE drops it unless the app crate
# references that crate. Without the reference the symbol sits in the rlib,
# vanishes from `librustapp.a`, the weak call sees NULL, and the image boots
# with NO backend registered.
#
# The failure is SILENT: verified by mutation during issue 0330 — deleting the
# anchor from examples/zephyr/rust/talker still BUILT and still linked, the
# symbol simply disappeared from the staticlib. Nothing but this gate catches it
# before runtime.
#
# The anchor used to be emitted by `nros::zephyr_component_main!` itself, which
# is why no gate existed: it could not go missing. Issue 0330 moved it to the
# app crate (the facade must not name concrete backends), so it CAN now go
# missing — hence this gate.
#
# Rule: a Zephyr Rust example that declares a NON-INERT `rmw-zenoh` / `rmw-xrce`
# feature (one that forwards to a real `dep:`) must invoke
# `nros::force_link_backend!` for that backend. Inert marker rows (`rmw-x = []`)
# link nothing and need no anchor, and neither does cyclonedds, whose register
# entry lives in the Zephyr module's C++ library.

set -euo pipefail
cd "$(dirname "$0")/.."

# shellcheck source=scripts/lib/grep-q.sh
source scripts/lib/grep-q.sh

# check_example <manifest> <src files…> — the rule for ONE example; returns 1 on
# a finding (printed to stderr). Shared by the scan and the self-test below, so
# the negative control drives the code the scan runs (phase-472 F1 / W9).
check_example() {
    local manifest="$1"; shift
    local src_files="$*" src_text src_bytes cat_rc pair feature rest krate dep row f found=0
    local src
    src="$(dirname "$manifest")/src"
    [ -n "$src_files" ] || return 0
    # phase-338 W2 — the whole `src/` tree, not just `lib.rs`: the entry macro
    # and its anchors live in a glue module (`src/app_main.rs`), and reading the
    # crate rather than one file matches the rule's reach (issue-0196).
    #
    # Issue 0726 — capture what the read actually returned. Under a 32-way gate
    # fan-out this gate intermittently reported a missing anchor for an example
    # that plainly has one; the read is the only step that can lose content
    # while leaving enough behind to pass the scope test below. The capture is
    # an `if`, not `cmd; rc=$?`: under `set -e` a non-zero read ends the shell
    # at the assignment, so `cat_rc` could only ever hold 0.
    #
    # phase-472 W3: read as CODE. A `// nros::force_link_backend!(…)` anchors
    # nothing, and a raw `cat` let it satisfy the anchor grep below.
    # shellcheck disable=SC2086
    if src_text=$(python3 scripts/lib/comments.py --lang rust $src_files); then cat_rc=0; else cat_rc=$?; fi
    src_bytes=${#src_text}
    # Only examples that actually use the facade entry macro are in scope.
    nros_grep_q 'zephyr_component_main!' <<<"$src_text" || return 0
    for pair in "rmw-zenoh:nros_rmw_zenoh:nros-rmw-zenoh" \
                "rmw-xrce:nros_rmw_xrce_cffi:nros-rmw-xrce-cffi"; do
        feature="${pair%%:*}"
        rest="${pair#*:}"
        krate="${rest%%:*}"
        dep="${rest##*:}"
        # The feature row must exist AND forward to a real dependency. An inert
        # `rmw-zenoh = []` marker links nothing.
        row="$(grep -E "^${feature}[[:space:]]*=" "$manifest" || true)"
        [ -n "$row" ] || continue
        case "$row" in
        *"dep:${dep}"*) ;;
        *) continue ;;
        esac
        # Issue 0726 — `nros_grep_q`, not `grep -q`: a grep that failed to start
        # under fan-out must exit 2, never read as a missing anchor. And it is a
        # CONDITIONAL: a bare `grep -q` returning 1 in statement position under
        # `set -e` killed the shell before the finding could print.
        if ! nros_grep_q "force_link_backend!(${krate})" <<<"$src_text"; then
            # Issue 0726 — say what was READ, not just what was concluded; the
            # per-file re-read settles "read lost it" vs a real absence.
            {
                echo "--- 0726 diagnostics ---"
                echo "    cat rc=${cat_rc}, src_text bytes=${src_bytes}"
                echo "    files read:"
                printf '%s\n' $src_files | sed 's/^/      /'
                echo "    per-file re-read for force_link_backend!(${krate}):"
                for f in $src_files; do
                    if nros_grep_q "force_link_backend!(${krate})" "$f"; then
                        echo "      PRESENT on disk: $f  <-- read lost it (or it is in a comment)"
                    else
                        echo "      absent: $f ($(wc -c <"$f" 2>/dev/null) bytes)"
                    fi
                done
            } >&2
            echo "ERROR: $src declares '${feature}' forwarding to dep:${dep}," >&2
            echo "       but never invokes nros::force_link_backend!(${krate})." >&2
            echo "       Without the anchor rustc's staticlib DCE drops" >&2
            echo "       ${krate}_register and the image boots with NO backend" >&2
            echo "       registered — and it builds and links cleanly (0155/0163)." >&2
            found=1
        fi
    done
    return "$found"
}

# Negative controls on the normal path (phase-472 F1 / W9): each drives
# `check_example`, the function the scan runs.
self_test() {
    local t rc
    t="$(mktemp -d)"
    mkdir -p "$t/src"
    printf '%s\n' '[features]' 'rmw-zenoh = ["dep:nros-rmw-zenoh"]' > "$t/Cargo.toml"
    printf '%s\n' 'nros::zephyr_component_main!(x);' > "$t/src/app_main.rs"
    rc=0; check_example "$t/Cargo.toml" "$t/src/app_main.rs" 2>/dev/null || rc=$?
    [ "$rc" -eq 1 ] || { echo "check-rmw-force-link-anchor SELFTEST FAILED: a missing anchor passed" >&2; rm -rf "$t"; exit 1; }
    printf '%s\n' '// nros::force_link_backend!(nros_rmw_zenoh);' >> "$t/src/app_main.rs"
    rc=0; check_example "$t/Cargo.toml" "$t/src/app_main.rs" 2>/dev/null || rc=$?
    [ "$rc" -eq 1 ] || { echo "check-rmw-force-link-anchor SELFTEST FAILED: an anchor in a COMMENT passed" >&2; rm -rf "$t"; exit 1; }
    printf '%s\n' 'nros::force_link_backend!(nros_rmw_zenoh);' >> "$t/src/app_main.rs"
    rc=0; check_example "$t/Cargo.toml" "$t/src/app_main.rs" 2>/dev/null || rc=$?
    [ "$rc" -eq 0 ] || { echo "check-rmw-force-link-anchor SELFTEST FAILED: a real anchor failed" >&2; rm -rf "$t"; exit 1; }
    printf '%s\n' '[features]' 'rmw-zenoh = []' > "$t/Cargo.toml"
    printf '%s\n' 'nros::zephyr_component_main!(x);' > "$t/src/app_main.rs"
    rc=0; check_example "$t/Cargo.toml" "$t/src/app_main.rs" 2>/dev/null || rc=$?
    rm -rf "$t"
    [ "$rc" -eq 0 ] || { echo "check-rmw-force-link-anchor SELFTEST FAILED: an inert marker row failed" >&2; exit 1; }
}
self_test

fail=0
n=0
for manifest in examples/zephyr/rust/*/Cargo.toml examples/zephyr/rust/*/*/Cargo.toml; do
    [ -f "$manifest" ] || continue
    dir="$(dirname "$manifest")"
    [ -d "$dir/src" ] || continue
    # `git ls-files`, not `find`: every source here is tracked, so this is an
    # index lookup rather than a directory walk (check-no-tracked-file-find).
    src_files=$(git ls-files -- "$dir/src/*.rs" | sort)
    [ -n "$src_files" ] || continue
    n=$((n + 1))
    # shellcheck disable=SC2086
    check_example "$manifest" $src_files || fail=1
done
[ "$n" -gt 0 ] || { echo "check-rmw-force-link-anchor: examined no Zephyr Rust example — refusing to pass" >&2; exit 1; }

if [ "$fail" -ne 0 ]; then
    echo "RMW force-link anchor gate FAILED." >&2
    exit 1
fi
echo "RMW force-link anchors present in every Zephyr Rust example that needs one ($n example(s) read)."
