# shellcheck shell=bash
# PROBE-OWNED verification for installation.md's "Pinning a version" run on
# the `installed` track (phase-517 W11). Appended after the book-extracted
# steps by run-bootstrap-probe.sh; runs in the same shell, in the pinned
# workspace the last step left.
#
# The chapter already asserted that `nros toolchain manifest` answered from
# the pinned release's prefix, and the build step had to exit 0. What is left:
# the build ran under the pin (the pin file is unchanged and still names the
# older release, and the toolchain that answers in the workspace is still the
# pinned one), and no nano-ros checkout reached the host.

echo '=== probe verify: a project pinned to an older release builds with it ==='
pin_file="$HOME/pinned_app/nros-toolchain.toml"
rc=0
nros_grep_q '^version = "0.6.0-nros4"$' "$pin_file" || rc=$?
case "$rc" in
    0) ;;
    1)
        echo "PROBE FAIL: $pin_file no longer pins 0.6.0-nros4:" >&2
        cat "$pin_file" >&2
        exit 1
        ;;
    *)
        echo "PROBE FAIL: could not read $pin_file (grep exited $rc)" >&2
        exit 1
        ;;
esac
answered="$(nros toolchain manifest 2>&1 | head -1)"
case "$answered" in
    *sdk/nros/0.6.0-nros4/share/nros/manifest.toml) ;;
    *)
        echo "PROBE FAIL: inside the pinned workspace the toolchain that answers is not 0.6.0-nros4: $answered" >&2
        exit 1
        ;;
esac
# Fetching the pinned release runs its install.sh, which fronts
# `~/.nros/bin/nros` at the NEWEST installed version, never at the fetched
# one for being fetched. Both versions here are known, so the expected front
# is stated rather than found by listing the store: the probe's own asset is
# 0.0.0-probe, which sorts below 0.6.0-nros4.
installed="$(readlink -f "$HOME/.nros/bin/nros")"
case "$installed" in
    */sdk/nros/0.6.0-nros4/*) ;;
    *)
        echo "PROBE FAIL: ~/.nros/bin/nros fronts $installed, not the newest installed release (0.6.0-nros4 above 0.0.0-probe)" >&2
        exit 1
        ;;
esac

nros_store="$HOME/.nros"
marker_hits="$(find / -xdev \( -path /proc -o -path "$nros_store" \) -prune -o \
    -path '*/packages/core/nros-core/Cargo.toml' -print 2>/dev/null || true)"
if [ -n "$marker_hits" ]; then
    echo "PROBE FAIL: a nano-ros root appeared outside the store during the run: $marker_hits" >&2
    exit 1
fi

echo "PROBE PASS: installed $(basename "$(dirname "$(dirname "$installed")")") -> project pinned to 0.6.0-nros4 -> fetched -> nros new + setup + build under the pin, with no checkout on the host"
