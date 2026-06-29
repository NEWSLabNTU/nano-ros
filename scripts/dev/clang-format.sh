#!/usr/bin/env bash
# Resolve the clang-format binary at the version the repo was formatted with.
#
# clang-format output drifts between major versions (e.g. v17 vs v22 reformat
# `reinterpret_cast<T(*)[N]>` differently), so an unpinned PATH `clang-format`
# produces spurious `just format` diffs / `check-*-fmt` failures across machines.
# SSoT version = `.clang-format-version`. Provision the pinned binary with
# `just setup-clang-format` (PyPI `clang-format` wheel into build/clang-format).
#
# `nros_clang_format` echoes the resolved binary path (pinned if present, else the
# PATH one with a loud version-skew warning) or errors with a setup hint.
nros_clang_format() {
    local root want pinned have
    root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
    want="$(cat "$root/.clang-format-version" 2>/dev/null || echo 17.0.6)"
    pinned="$root/build/clang-format/bin/clang-format"
    if [ -x "$pinned" ]; then
        printf '%s\n' "$pinned"
        return 0
    fi
    if command -v clang-format >/dev/null 2>&1; then
        have="$(clang-format --version 2>/dev/null | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1)"
        if [ "$have" != "$want" ]; then
            printf 'WARN: clang-format %s on PATH != pinned %s — run `just setup-clang-format` for a consistent version (different versions reformat differently → spurious fmt diffs).\n' "${have:-unknown}" "$want" >&2
        fi
        command -v clang-format
        return 0
    fi
    printf 'ERROR: clang-format not found. Run `just setup-clang-format` (installs the pinned %s).\n' "$want" >&2
    return 1
}
