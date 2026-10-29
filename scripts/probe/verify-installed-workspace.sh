# shellcheck shell=bash
# PROBE-OWNED verification for installation.md's "Provision a whole workspace"
# run on the `installed` track (phase-517 W4). Appended after the
# book-extracted steps by run-bootstrap-probe.sh; runs in the same shell, in
# the workspace the chapter scaffolded (`two_ws`).
#
# The chapter ran ONE bare `nros setup` and then `nros sync && nros build --all`,
# and each step must have exited 0 to get here. What is left: both images
# really were built, a second `nros setup --check` finds nothing missing (so
# the build did not quietly need more than the one setup provisioned), and no
# nano-ros checkout appeared on the host.

echo '=== probe verify: one nros setup provisioned every image the workspace declares ==='
for elf in \
    build/posix-zenoh/linux_entry/target/debug/linux_entry \
    build/freertos-zenoh/freertos_entry/target/thumbv7m-none-eabi/debug/freertos_entry; do
    if [ ! -f "$elf" ]; then
        echo "PROBE FAIL: nros build --all left no $elf (in $PWD)" >&2
        exit 1
    fi
    echo "  built: $elf"
done

if ! nros setup --check; then
    echo "PROBE FAIL: nros setup --check reports something missing after the one nros setup the chapter ran" >&2
    exit 1
fi

nros_store="$HOME/.nros"
marker_hits="$(find / -xdev \( -path /proc -o -path "$nros_store" \) -prune -o \
    -path '*/packages/core/nros-core/Cargo.toml' -print 2>/dev/null || true)"
if [ -n "$marker_hits" ]; then
    echo "PROBE FAIL: a nano-ros root appeared outside the store during the chapter: $marker_hits" >&2
    exit 1
fi

# phase-517 W17 — the index sources with no `location = "store"` and no
# chapter that reaches them, measured with no checkout: where each one lands.
# Their `dest` is SDK-root-relative, so they must land inside the installed
# SDK root (as W10 measured for the NuttX sources), never in a checkout.
echo '=== probe verify: checkout-located index sources provision into the SDK root ==='
sdk_root="$(nros sdk-root)"
for pair in \
    tonbandgeraet:third-party/tracing/Tonbandgeraet \
    px4-rs:third-party/px4/px4-rs \
    cyclonedds-src:third-party/dds/cyclonedds; do
    src="${pair%%:*}" dest="${pair#*:}"
    if ! nros setup --source "$src"; then
        echo "PROBE FAIL: nros setup --source $src failed with no checkout" >&2
        exit 1
    fi
    if [ -z "$(ls -A "$sdk_root/$dest" 2>/dev/null)" ]; then
        echo "PROBE FAIL: nros setup --source $src left $sdk_root/$dest empty" >&2
        exit 1
    fi
    echo "  $src -> $sdk_root/$dest"
done

echo "PROBE PASS: installed release -> nros new (linux + freertos images) -> one nros setup -> nros build --all, with no nano-ros checkout on the host"
