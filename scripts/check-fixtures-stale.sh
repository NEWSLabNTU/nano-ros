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

workspace_stale=()
if command -v parallel >/dev/null 2>&1; then
    mapfile -t workspace_stale < <(python3 scripts/build/fixtures-manifest.py list-workspaces --for-probe \
        | parallel --jobs "$(nproc)" bash scripts/test/workspace-fixture-stale.sh {} 2>/dev/null)
else
    while IFS= read -r line; do
        out="$(bash scripts/test/workspace-fixture-stale.sh "$line")"
        [ -n "$out" ] && workspace_stale+=("$out")
    done < <(python3 scripts/build/fixtures-manifest.py list-workspaces --for-probe)
fi
if [ ${#workspace_stale[@]} -gt 0 ]; then
    echo "ERROR: ${#workspace_stale[@]} workspace fixture(s) are missing or stale:" >&2
    printf '  %s\n' "${workspace_stale[@]}" >&2
    echo "  Run \`just native build-workspace-fixtures\` before test-all." >&2
    echo "  (bypass with  NROS_SKIP_FIXTURE_CHECK=1 )" >&2
    exit 1
fi
