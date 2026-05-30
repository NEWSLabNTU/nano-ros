#!/usr/bin/env bash
# Phase 191.3 — guard against drift between the two copies of the QEMU configure
# flags: the index `[tool.qemu.source].configure` (the source-build fallback
# `nros setup` runs) and `ci/nano-ros-sdk/scripts/build-qemu.sh` (the prebuilt
# build). They MUST match or prebuilt != source-built. Cross-repo at runtime
# (the build script lives in nano-ros-sdk), but both copies are present in this
# repo via the `ci/nano-ros-sdk/` seed, so we diff their flag sets here.
#
# Compares the set of `--flag` tokens (ignoring `--prefix=…`, which legitimately
# differs: `{prefix}` vs `"$prefix"`). Run from the repo root; CI runs it via
# sdk-index-gate.
set -euo pipefail

index="${1:-nros-sdk-index.toml}"
script="${2:-ci/nano-ros-sdk/scripts/build-qemu.sh}"

flags() {
    # Extract every `--word…` token from stdin, drop --prefix*, sort-unique.
    grep -oE -- '--[a-z0-9-]+(=[^ "\\]*)?' \
        | grep -v -- '--prefix' \
        | sort -u
}

# Phase 208 (post-audit) — scope to the [tool.qemu.source] section. The
# index has multiple `configure = …` keys across different tools/sources;
# the previous `grep -E '^configure\s*=' | head -1` picked the first one
# in the file (`git submodule update --init --recursive`, a source-package
# recipe), so the drift check fired against every commit even when the
# qemu configure flags hadn't changed.
index_flags="$(sed -n '/^\[tool\.qemu\.source\]/,/^\[/p' "$index" \
    | grep -E '^configure\s*=' | head -1 | flags)"
# The build script's ./configure invocation may span lines (trailing `\`); join.
script_flags="$(sed -n '/\.\/configure/,/make /p' "$script" | tr '\n' ' ' | flags)"

if [ "$index_flags" != "$script_flags" ]; then
    echo "qemu configure flags DRIFTED between the index and build-qemu.sh:" >&2
    diff <(echo "$index_flags") <(echo "$script_flags") >&2 || true
    echo "→ keep [tool.qemu.source].configure and build-qemu.sh in sync." >&2
    exit 1
fi
echo "qemu configure flags match ($(echo "$index_flags" | tr '\n' ' '))"
