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

fail=0

for manifest in examples/zephyr/rust/*/Cargo.toml examples/zephyr/rust/*/*/Cargo.toml; do
    [ -f "$manifest" ] || continue
    dir="$(dirname "$manifest")"
    src="$dir/src/lib.rs"
    [ -f "$src" ] || continue

    # Only examples that actually use the facade entry macro are in scope.
    grep -q 'zephyr_component_main!' "$src" || continue

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

        if ! grep -q "force_link_backend!(${krate})" "$src"; then
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
