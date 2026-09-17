#!/usr/bin/env bash
# Make sure an unpacked Zephyr SDK is REGISTERED with cmake — issue 1279 /
# phase-449 W8.
#
# `find_package(Zephyr-sdk)` reads `~/.cmake/packages/Zephyr-sdk/`, which is
# per-USER state. Every gate in front of the registration asked about something
# else:
#
#   * `just zephyr setup` skips the whole of `scripts/zephyr/setup.sh` when the
#     WORKSPACE directory exists;
#   * `setup.sh` itself skips `install_sdk` — which is what runs the SDK's own
#     `setup.sh -c` — when the SDK DIRECTORY exists.
#
# So a host holding a complete, unpacked, unregistered SDK is one neither verb
# could repair: both print a cheerful "already present" and return 0, and the
# failure surfaces much later inside `FindZephyr-sdk.cmake` during a build. As
# `runner-doctor` puts it: "an unregistered SDK fails at configure, not at
# download."
#
# Two ways to reach that state, and the second is ordinary: `~/.cmake` is lost
# or cleared; or the workspace lives in a persistent store while `~/.cmake` does
# not — exactly a container whose writable layer `--ephemeral` destroys after
# one job, so EVERY container starts with an unpacked, unregistered SDK.
#
# Registration is a few milliseconds, safe to repeat, and about the user's
# environment rather than the workspace — so it is gated on ITSELF here and
# called unconditionally from both verbs.
#
# `-c` ONLY, never `-h`. `-c` writes the cmake package registry entry; `-h`
# installs host tools, is the expensive half, and is reported to FAIL in a
# container (issue 1279). Keeping them apart is the point: a step that is cheap
# and always safe must not inherit the gating of one that is neither.
set -uo pipefail

sdk="${1:?usage: ensure-sdk-registered.sh <zephyr-sdk-dir>}"

reg_dir="${HOME}/.cmake/packages/Zephyr-sdk"

# The registry entry is a file whose CONTENT is `<sdk>/cmake`. Keying on the
# content rather than on "the directory is non-empty" is what makes this
# per-ARTIFACT: a host registered for a DIFFERENT SDK version has a non-empty
# directory and still cannot build this line.
registered() {
    local f
    for f in "$reg_dir"/*; do
        [ -f "$f" ] || continue
        case "$(cat "$f" 2>/dev/null)" in
            "$sdk"/cmake|"$sdk"/cmake/) return 0 ;;
        esac
    done
    return 1
}

if [ ! -d "$sdk" ]; then
    # Not an error: provisioning may legitimately not have run yet, and the
    # caller that skipped the SDK entirely (`--skip-sdk`) still calls this.
    exit 0
fi

if registered; then
    exit 0
fi

if [ ! -x "$sdk/setup.sh" ]; then
    echo "ensure-sdk-registered: $sdk has no executable setup.sh — cannot register." >&2
    exit 1
fi

echo "ensure-sdk-registered: $sdk is unpacked but NOT registered with cmake; registering." >&2
rc=0
( cd "$sdk" && ./setup.sh -c ) >/dev/null 2>&1 || rc=$?

if ! registered; then
    echo "ensure-sdk-registered: registration did not take (SDK setup.sh -c exited $rc)." >&2
    echo "  find_package(Zephyr-sdk) reads $reg_dir; without an entry there a" >&2
    echo "  build fails at CONFIGURE, far from this step." >&2
    echo "  Try by hand:  (cd $sdk && ./setup.sh -c)" >&2
    exit 1
fi

echo "ensure-sdk-registered: registered $sdk with cmake." >&2
