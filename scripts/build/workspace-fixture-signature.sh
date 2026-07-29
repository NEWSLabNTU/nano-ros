#!/usr/bin/env bash
# Print the build-input signature for one workspace fixture manifest record.
set -euo pipefail

record="${1:?usage: workspace-fixture-signature.sh <manifest-record>}"

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"

IFS=$'\x1f' read -r id _lang dir bringup _entry _build_subdir _target_dir _codegen_out _defs <<< "$record"
[ -n "$id" ] && [ -n "$dir" ] || {
    echo "workspace fixture record is missing id/dir" >&2
    exit 2
}

workspace="$repo_root/$dir"
[ -d "$workspace" ] || {
    echo "workspace fixture '$id' dir does not exist: $dir" >&2
    exit 2
}

{
    printf 'nros-workspace-fixture-v3\0%s\0' "$record"
    # #182 — the fixture is a function of the CODEGEN TOOL, not just the
    # workspace sources: `nros codegen entry` emits the entry TU, `nros ws
    # sync`/`generate-*` shape the msg crates. A signature blind to the tool
    # let a fixture built with a pre-fd32a0f75 emitter verify as "fresh"
    # (realtime tier lanes ran museum TUs with correct-looking sources).
    # Hash the CLI binary's content into the signature; absent binary hashes
    # as the literal marker (the build script builds it before stamping).
    # RFC-0061 / phase-318 W1 — key on what the tool EMITS, not on its binary.
    # Rust binaries are not reproducible across rebuilds, so the old
    # `sha256(nros)` moved on every `just setup-cli` and invalidated every
    # workspace fixture: measured 2026-07-28, a codegen change that only ADDED a
    # rejection path no fixture uses invalidated 40 fixtures / 35 build dirs, a
    # multi-hour ~100 GB rebuild whose correct answer was zero.
    #
    # `nros codegen-fingerprint` hashes the bytes this build's emitters produce
    # for a compiled-in corpus, so it moves iff generated code would move. Cached
    # per binary hash: one probe run per rebuild, a file read thereafter.
    #
    # Fallback order is deliberate — an OLDER nros without the verb falls back to
    # the binary hash (today's over-approximation), never to "assume fresh".
    nros_bin="$repo_root/packages/cli/target/release/nros"
    if [ -x "$nros_bin" ]; then
        bin_hash="$(sha256sum "$nros_bin" | awk '{print $1}')"
        cache="$repo_root/.nros-cache/codegen-fingerprint/$bin_hash"
        if [ -s "$cache" ]; then
            fp="$(cat "$cache")"
        elif fp="$("$nros_bin" codegen-fingerprint 2>/dev/null)" && [ -n "$fp" ]; then
            mkdir -p "$(dirname "$cache")" && printf '%s' "$fp" > "$cache" || true
        else
            fp="binary:$bin_hash"
        fi
        printf 'tool:nros\0%s\0' "$fp"
    else
        printf 'tool:nros-absent\0'
    fi

    # phase-318 W1.b — the RESOLVER, for records that actually run it.
    #
    # `nros sync` shells out to `nros-launch-resolve` and the SystemModel it
    # emits is a committed fixture INPUT, so a signature blind to it repeats
    # #182 one layer down. Scoped to records with a bringup: a resolver rebuild
    # then invalidates the fixtures that consume a resolved model and nothing
    # else, which is much narrower than hashing it into everything.
    if [ -n "${bringup:-}" ]; then
        printf 'tool:resolve\0%s\0' "$(bash "$repo_root/scripts/build/resolve-fingerprint.sh" 2>/dev/null || echo resolver-error)"
    fi
    # phase-300 W2.1 — enumerate via the git index instead of walking: the
    # find-prune list omitted _deps/install/log, so cmake byproducts were
    # content-hashed into the signature (slow + FALSE STALENESS when a
    # rebuilt _deps changed the hash with no source change). Tracked files
    # plus untracked-but-unignored ones (new sources not yet committed)
    # cover exactly the source set; gitignored build trees can never leak.
    rel_ws="${workspace#$repo_root/}"
    git -C "$repo_root" ls-files -z --cached --others --exclude-standard -- "$rel_ws" \
        | while IFS= read -r -d '' relf; do printf '%s/%s\0' "$repo_root" "$relf"; done \
        | sort -z \
        | while IFS= read -r -d '' file; do
            rel="${file#$repo_root/}"
            case "$rel" in
                *.c|*.cc|*.cpp|*.h|*.hpp|*.rs|*.toml|*.xml|*.lock|*/CMakeLists.txt|*/package.xml|*.cmake)
                    printf 'path:%s\0' "$rel"
                    cat "$file"
                    printf '\0'
                    ;;
            esac
        done
} | sha256sum | awk '{print $1}'
