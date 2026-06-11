#!/usr/bin/env bash
set -e

if [ "${NROS_SKIP_FIXTURE_CHECK:-0}" != "0" ]; then
    exit 0
fi

cmake_records() {
    python3 scripts/build/fixtures-manifest.py list --for-probe --lang c
    python3 scripts/build/fixtures-manifest.py list --for-probe --lang cpp
}

cmake_stale=()
if command -v parallel >/dev/null 2>&1; then
    mapfile -t cmake_stale < <(cmake_records | parallel --jobs "$(nproc)" bash scripts/test/cmake-fixture-stale.sh {} 2>/dev/null)
else
    while IFS= read -r line; do
        out="$(bash scripts/test/cmake-fixture-stale.sh "$line")"
        [ -n "$out" ] && cmake_stale+=("$out")
    done < <(cmake_records)
fi
if [ ${#cmake_stale[@]} -gt 0 ]; then
    echo "WARNING: ${#cmake_stale[@]} C/C++ fixture cell(s) were STALE and have now been rebuilt (cmake):" >&2
    printf '  %s\n' "${cmake_stale[@]}" >&2
    echo "  (cmake/ninja incremental self-heal; bypass with  NROS_SKIP_FIXTURE_CHECK=1 )" >&2
fi

rust_stale=()
if command -v parallel >/dev/null 2>&1; then
    mapfile -t rust_stale < <(python3 scripts/build/fixtures-manifest.py list --for-probe --with-platform --lang rust \
        | parallel --jobs "$(nproc)" bash scripts/test/rust-fixture-stale.sh {} 2>/dev/null)
else
    while IFS= read -r line; do
        out="$(bash scripts/test/rust-fixture-stale.sh "$line")"
        [ -n "$out" ] && rust_stale+=("$out")
    done < <(python3 scripts/build/fixtures-manifest.py list --for-probe --with-platform --lang rust)
fi
if [ ${#rust_stale[@]} -gt 0 ]; then
    echo "WARNING: ${#rust_stale[@]} rust fixture(s) were STALE and have now been rebuilt by cargo:" >&2
    printf '  %s\n' "${rust_stale[@]}" >&2
    echo "  (cargo incremental self-heal; bypass with  NROS_SKIP_FIXTURE_CHECK=1 )" >&2
fi

# issue 0030 — gate the workspace-fixture preflight on cross-toolchain presence.
# build-test-fixtures builds each platform's workspace Entry via that platform's
# `build-examples` lane, which skips cleanly when its cross toolchain is absent.
# So on a lighter tier the fixture legitimately does not exist — requiring it
# would hard-fail the WHOLE `test-all` preflight, even though the matching e2e
# test already `skip!`s at runtime on the absent binary. Mirror the
# embedded-Cyclone gate in the `test-all` recipe (justfile): require a cross
# workspace fixture only when its toolchain is present; otherwise drop it from
# the required set with an info note. (esp32/nuttx are excluded entirely via
# `skip_probe = true` — they are not in the build-test-fixtures fan-out.)
# Only the cargo/cmake-lane workspace fixtures (freertos, threadx-linux, plus the
# always-host native/c/cpp/mixed rows) reach this probe and write the
# `.nros-workspace-fixture.*.inputsig` stamp the stale check demands.
# zephyr/esp32/nuttx are `skip_probe = true` own-lane artifacts (west / esp /
# nuttx machinery, each with its own sig) and never appear here.
workspace_toolchain_present() {
    case "$1" in
        workspace-rust-qemu-freertos)
            command -v arm-none-eabi-gcc >/dev/null 2>&1 ;;
        workspace-rust-threadx-linux)
            [ -n "${THREADX_DIR:-}" ] || [ -d third-party/threadx/kernel ] ;;
        *)
            return 0 ;;
    esac
}

workspace_records=()
while IFS= read -r line; do
    id="${line%%$'\x1f'*}"
    if workspace_toolchain_present "$id"; then
        workspace_records+=("$line")
    else
        echo "info: workspace fixture '$id' not required in preflight — cross toolchain absent (issue 0030)" >&2
    fi
done < <(python3 scripts/build/fixtures-manifest.py list-workspaces --for-probe)

workspace_stale=()
if [ ${#workspace_records[@]} -eq 0 ]; then
    :
elif command -v parallel >/dev/null 2>&1; then
    mapfile -t workspace_stale < <(printf '%s\n' "${workspace_records[@]}" \
        | parallel --jobs "$(nproc)" bash scripts/test/workspace-fixture-stale.sh {} 2>/dev/null)
else
    for line in "${workspace_records[@]}"; do
        out="$(bash scripts/test/workspace-fixture-stale.sh "$line")"
        [ -n "$out" ] && workspace_stale+=("$out")
    done
fi
if [ ${#workspace_stale[@]} -gt 0 ]; then
    echo "ERROR: ${#workspace_stale[@]} workspace fixture(s) are missing or stale:" >&2
    printf '  %s\n' "${workspace_stale[@]}" >&2
    echo "  Run \`just native build-workspace-fixtures\` before test-all." >&2
    echo "  (bypass with  NROS_SKIP_FIXTURE_CHECK=1 )" >&2
    exit 1
fi
