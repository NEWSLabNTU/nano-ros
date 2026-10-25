# shellcheck shell=bash
# PROBE-OWNED verification for a QEMU platform chapter on the `installed` track
# (phase-517 W10). Appended after the book-extracted steps by
# run-bootstrap-probe.sh; runs in the same shell.
#
# The chapter has already asserted itself: its Run step is a `probe-run` block
# whose documented output (`probe-expect`) the run had to print, so reaching
# this file means the image booted and said what the chapter says it says.
# What is left is the track's own claim — that nothing between the install and
# the run planted a nano-ros checkout the build could have resolved instead
# (check-installed-sdk-root.sh asserted it right after the install; a chapter
# step could have cloned one since).

echo '=== probe verify: the platform chapter ran with no checkout ==='
nros_store="$HOME/.nros"
marker_hits="$(find / -xdev \( -path /proc -o -path "$nros_store" \) -prune -o \
    -path '*/packages/core/nros-core/Cargo.toml' -print 2>/dev/null || true)"
if [ -n "$marker_hits" ]; then
    echo "PROBE FAIL: a nano-ros root appeared outside the store during the chapter: $marker_hits" >&2
    exit 1
fi

echo "PROBE PASS: installed release -> ${PROBE_PLATFORM:-?} chapter -> setup -> build -> run, with no nano-ros checkout on the host"
