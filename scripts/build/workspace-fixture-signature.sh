#!/usr/bin/env bash
# Print the build-input signature for one workspace fixture manifest record.
#
#   workspace-fixture-signature.sh <manifest-record>
#   workspace-fixture-signature.sh --batch < records   # "<id>\t<signature>" per line
#   workspace-fixture-signature.sh --self-test         # batch == one-record, per record
#
# Issue B39 -- `--batch` hashes the same bytes per record as the one-record
# form, so a signature is identical either way; it only computes what records
# SHARE once: the tool fingerprints (one `nros` run each) and each workspace
# dir's source manifest. An audit that checks every in-lane row
# (`rmw_coordinate_truth`, tier2-nightly: 401 rows over ~90 dirs) ran this
# script once per row, ~0.2 s each, serially, past its test's 60 s.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
repo_root="$(cd "$script_dir/../.." && pwd)"

# shellcheck source=scripts/build/source-manifest.sh
source "$script_dir/source-manifest.sh"
# shellcheck source=scripts/build/codegen-fingerprint.sh
source "$script_dir/codegen-fingerprint.sh"
# shellcheck source=scripts/lib/grep-q.sh
source "$repo_root/scripts/lib/grep-q.sh"

# The shared parts, each written once into "$work" (bytes with NULs, so files,
# not shell variables), in the order the stream puts them.
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

tools_bytes() {
    {
    # #182 — the fixture is a function of the CODEGEN TOOL, not just the
    # workspace sources: `nros dev codegen entry` emits the entry TU, `nros ws
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
    # The ladder itself lives in `codegen-fingerprint.sh` (issue 1018) — it had
    # been written twice here and in the compile-check lane and had already
    # drifted. Byte-for-byte the same signature: the `binary:` fallback prefix
    # this lane spells is now the helper's second argument.
    if fp="$(nros_codegen_fingerprint "$repo_root" "binary:")"; then
        printf 'tool:nros\0%s\0' "$fp"
    else
        printf 'tool:nros-absent\0'
    fi

    # Issue 2441 — and the CLI's PLANNING, not only what it emits. A workspace
    # fixture is built by `nros configure` + `nros build`, so it is a function
    # of the seeds, the image statement and the configure options they hand the
    # build, none of which is codegen output: a planning change moved nothing
    # above and the fixture was REUSED over it. Not the source stamp (2439's
    # west key), which a `doctor.rs` edit moves for every row: the planning
    # set is the files the fixture road RUNS (`cli-planning-set.txt`, gated by
    # `check-cli-planning-set`), and the ruling (2026-11-12) keyed on that.
    if ps="$(nros_cli_planning_stamp "$repo_root")"; then
        printf 'tool:nros-planning\0%s\0' "$ps"
    else
        printf 'tool:nros-planning-absent\0'
    fi

    } > "$work/tools"
}

resolver_bytes() {
    {
    # phase-318 W1.b — the RESOLVER, for records that actually run it.
    #
    # `nros configure` shells out to `nros-launch-resolve` and the SystemModel it
    # emits is a committed fixture INPUT, so a signature blind to it repeats
    # #182 one layer down. Scoped to records with a bringup: a resolver rebuild
    # then invalidates the fixtures that consume a resolved model and nothing
    # else, which is much narrower than hashing it into everything.
    if true; then
        printf 'tool:resolve\0%s\0' "$(bash "$repo_root/scripts/build/resolve-fingerprint.sh" 2>/dev/null || echo resolver-error)"
    fi

    } > "$work/resolve"
}

# manifest_bytes <rel_dir>: the dir's source manifest, cached per dir.
manifest_bytes() {
    local rel_ws="$1" key
    key="$(printf '%s' "$rel_ws" | sha256sum | awk '{print $1}')"
    if [ ! -f "$work/m-$key" ]; then
        nros_source_manifest "$repo_root" "$rel_ws" > "$work/m-$key"
    fi
    printf '%s' "$work/m-$key"
}

# signature_of <record>: the one-record signature, from the cached parts.
signature_of() {
    local record="$1" id _lang dir bringup _rest workspace mf
    IFS=$'\x1f' read -r id _lang dir bringup _rest <<< "$record"
    [ -n "$id" ] && [ -n "$dir" ] || {
        echo "workspace fixture record is missing id/dir" >&2
        return 2
    }
    workspace="$repo_root/$dir"
    [ -d "$workspace" ] || {
        echo "workspace fixture '$id' dir does not exist: $dir" >&2
        return 2
    }
    [ -f "$work/tools" ] || tools_bytes
    if [ -n "${bringup:-}" ] && [ ! -f "$work/resolve" ]; then
        resolver_bytes
    fi
    mf="$(manifest_bytes "${workspace#$repo_root/}")"
    {
    printf 'nros-workspace-fixture-v4\0%s\0' "$record"
        cat "$work/tools"
        if [ -n "${bringup:-}" ]; then
            cat "$work/resolve"
        fi
        cat "$mf"
    } | sha256sum | awk '{print $1}'
}

# --self-test: the batch form must give each record the bytes the one-record
# form gives it, run in its own process with nothing shared. Records come from
# the manifest: two that share a dir (the per-dir cache is reused), one from
# another dir, and that one with its bringup blanked (no resolver block; every
# manifest row has a bringup today). Negative controls: the four signatures
# differ, and a record whose dir is missing fails the batch without dropping
# the records around it.
self_test() {
    local all a1 a2 b nob bad recs fails=0 i id single batch_out rc
    all="$(python3 "$repo_root/scripts/build/fixtures-manifest.py" list-workspaces --for-probe)"
    a1="$(head -n1 <<< "$all")"
    local dir_a
    dir_a="$(printf '%s' "$a1" | cut -d$'\x1f' -f3)"
    a2="$(awk -F'\x1f' -v d="$dir_a" -v r="$a1" '$3==d && $0!=r && !n++' <<< "$all")"
    b="$(awk -F'\x1f' -v d="$dir_a" '$3!=d && $4!="" && !n++' <<< "$all")"
    nob="$(printf '%s' "$b" | awk -F'\x1f' -v OFS=$'\x1f' '{$1=$1"-nobringup"; $4=""; print}')"
    bad="$(printf '%s' "$b" | awk -F'\x1f' -v OFS=$'\x1f' '{$1="selftest-missing"; $3="no/such/dir"; print}')"
    for r in "$a1" "$a2" "$b"; do
        [ -n "$r" ] || { echo "self-test: the manifest lacks two rows in one dir and one in another" >&2; return 1; }
    done
    recs=("$a1" "$a2" "$b" "$nob")

    rc=0
    batch_out="$(printf '%s\n' "${recs[0]}" "${recs[1]}" "$bad" "${recs[2]}" "${recs[3]}" \
        | bash "$0" --batch 2>/dev/null)" || rc=$?
    if [ "$rc" -ne 2 ]; then
        echo "FAIL: a batch with a missing-dir record exits 2 (got $rc)" >&2; fails=$((fails + 1))
    fi
    # nros_grep_q exits 2 itself on a tool error, so the branch is match/miss.
    if nros_grep_q '^selftest-missing' <<< "$batch_out"; then
        echo "FAIL: a failed record prints no signature" >&2; fails=$((fails + 1))
    fi
    for i in "${!recs[@]}"; do
        id="${recs[$i]%%$'\x1f'*}"
        single="$(bash "$0" "${recs[$i]}")"
        if ! nros_grep_q -x -F -- "$id"$'\t'"$single" <<< "$batch_out"; then
            echo "FAIL: batch and single differ for $id" >&2; fails=$((fails + 1))
        fi
    done
    if [ "$(printf '%s\n' "$batch_out" | cut -f2 | sort -u | wc -l)" -ne ${#recs[@]} ]; then
        echo "FAIL: distinct records give distinct signatures" >&2; fails=$((fails + 1))
    fi
    echo "workspace-fixture-signature self-test: $(( ${#recs[@]} + 3 )) checks, $fails failed"
    [ "$fails" -eq 0 ]
}

if [ "${1:-}" = "--self-test" ]; then
    self_test
    exit $?
fi

if [ "${1:-}" = "--batch" ]; then
    rc=0
    while IFS= read -r record; do
        [ -n "$record" ] || continue
        if sig="$(signature_of "$record")"; then
            printf '%s\t%s\n' "${record%%$'\x1f'*}" "$sig"
        else
            rc=2
        fi
    done
    exit "$rc"
fi

record="${1:?usage: workspace-fixture-signature.sh <manifest-record> | --batch | --self-test}"
signature_of "$record"
