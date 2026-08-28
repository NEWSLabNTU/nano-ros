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

fail=0

for manifest in examples/zephyr/rust/*/Cargo.toml examples/zephyr/rust/*/*/Cargo.toml; do
    [ -f "$manifest" ] || continue
    dir="$(dirname "$manifest")"
    [ -d "$dir/src" ] || continue

    # phase-338 W2 — search the whole `src/` tree, not just `lib.rs`. The entry
    # macro and its anchors now live in a dedicated glue module
    # (`src/app_main.rs`) so the node logic in `lib.rs` stays byte-identical
    # across platforms; a gate that only read `lib.rs` reported every migrated
    # example as missing its anchor. Reading the crate rather than one file also
    # makes the gate's coverage match the rule it enforces (issue-0196).
    # `git ls-files`, not `find`: every source here is tracked, so this is an
    # index lookup rather than a directory walk (check-no-tracked-file-find).
    src_files=$(git ls-files -- "$dir/src/*.rs" | sort)
    [ -n "$src_files" ] || continue
    # Issue 0726 — capture what `cat` actually returned. Under a 32-way gate
    # fan-out this gate intermittently reports a missing anchor for an example
    # that plainly has one, and the read is the only step that can lose content
    # while still leaving enough behind to pass the `zephyr_component_main!`
    # scope test below. `cat` reports a per-file failure on stderr and CONTINUES
    # with the rest, so a partial read is indistinguishable from a real absence
    # by the time the anchor grep runs.
    #
    # The capture is an `if`, not `cmd; rc=$?`: under this script's `set -e` a
    # non-zero `cat` ends the shell at the assignment, so `cat_rc` could only
    # ever hold 0 and the diagnostic that prints it could never say anything.
    # Same shape as the anchor grep below, same fix.
    # shellcheck disable=SC2086
    if src_text=$(cat $src_files); then cat_rc=0; else cat_rc=$?; fi
    src_bytes=${#src_text}
    src="$dir/src"

    # Only examples that actually use the facade entry macro are in scope.
    nros_grep_q 'zephyr_component_main!' <<<"$src_text" || continue

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

        # Issue 0726 — `grep -q` exits 1 for "not found" and >=2 for an ERROR,
        # and `if !` cannot tell them apart. Under a 32-way gate fan-out a
        # forked grep can fail to start (EAGAIN) or be killed, and this gate
        # then reported a missing anchor for an example that has one: a
        # confident, specific, wrong finding, green->red under load and never
        # the other way. `nros_grep_q` exits 2 on >=2 instead.
        #
        # It replaces a hand-rolled `grep -q …; rc=$?` that could not run: this
        # script sets `-e`, so a `grep -q` returning 1 in STATEMENT position
        # killed the shell before the next line, and the whole finding below —
        # the diagnostics, the ERROR text, even "gate FAILED" — was unreachable.
        # A genuinely missing anchor exited 1 in silence. Verified against this
        # file at HEAD on a scratch tree with the anchor removed, and verified
        # again after the change: the finding now prints. So the arms are a
        # CONDITIONAL, which is also the only shape `set -e` leaves intact.
        if ! nros_grep_q "force_link_backend!(${krate})" <<<"$src_text"; then
            # Issue 0726 — say what was READ, not just what was concluded. If
            # the anchor is present on disk but absent from `src_text`, this is
            # the fan-out flake and not a real finding; per-file greps below
            # settle which, because they re-read from disk independently.
            {
                echo "--- 0726 diagnostics ---"
                echo "    cat rc=${cat_rc}, src_text bytes=${src_bytes}"
                echo "    git ls-files returned:"
                printf '%s\n' "$src_files" | sed 's/^/      /'
                echo "    per-file re-read for force_link_backend!(${krate}):"
                # shellcheck disable=SC2086
                for f in $src_files; do
                    if nros_grep_q "force_link_backend!(${krate})" "$f"; then
                        echo "      PRESENT on disk: $f  <-- read lost it"
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
            fail=1
        fi
    done
done

if [ "$fail" -ne 0 ]; then
    echo "RMW force-link anchor gate FAILED." >&2
    exit 1
fi

echo "RMW force-link anchors present in every Zephyr Rust example that needs one."
