#!/usr/bin/env bash
# phase-363 W3 — the ONE way this tree turns a set of source paths into a
# content signature.
#
# WHY A MANIFEST RATHER THAN A CONTENT STREAM
#
# Both signature scripts used to `cat` every file into one `sha256sum`. That
# answers "did anything change?" and nothing else: a mismatch names no file, so
# a false-stale verdict cannot be investigated without bisecting by hand. Go's
# module `dirhash` (the `h1:` algorithm behind every `go.sum` line) solved this
# by hashing a MANIFEST instead — one `<hash>  <path>` line per file, sorted,
# then hash that. Same strength, and two manifests diff.
#
# `sha256sum` already emits exactly that format, so this costs one process for
# the whole set rather than one per file.
#
# WHY NO EXTENSION ALLOWLIST
#
# The old `case "$rel" in *.c|*.rs|*.toml|…)` filter was a denylist by omission,
# and it silently dropped real build inputs. Measured 2026-08-15 over the dirs
# each signature actually covers:
#
#   workspace lane (14 dirs)      37 `.conf`, 9 `.yaml`, 1 `.msg`, 1 `.json`
#   compile-check lane (24 dirs)   8 `.conf`, 3 `.msg`
#
# `.conf` is Zephyr Kconfig overlay — the SAME input class as issues 0167 and
# 0466, each of which cost a kernel-dump investigation. `.msg` is codegen input.
# The single `.json` is `realtime-rust/riscv32imac-unknown-nuttx-elf.json`, the
# custom RISC-V TARGET SPEC: edit it and the ABI moves, and no signature saw it.
# That one is the argument against allowlists in miniature — nobody sat down and
# decided a target spec was not a build input; it simply was not on the list.
#
# The two copies of the filter had also drifted apart: `*.yaml` was in one and
# not the other.
#
# No mature implementation filters source inputs by type. Go's dirhash hashes
# every file in the tree. Nix's `lib.fileset` exists because hand-filtering was
# too error-prone and offers git-tracked-ness as the source of truth. Bazel
# requires inputs to be DECLARED and then verifies the declaration against the
# compiler's own dependency output.
#
# So: enumerate through the git index — which is what makes the filter redundant,
# since git's ignore rules already exclude every build tree — and hash all of it.
# The only exclusions are files that cannot affect a build, and each costs
# nothing but a false-stale if this judgement is ever wrong.
#
# ERRORS ARE FATAL, NOT SILENT
#
# The previous code ran `git ls-files … 2>/dev/null` and `cat` inside a pipeline
# subshell, so an unreadable file or a failed enumeration produced a SHORTER
# stream and a perfectly valid-looking hash. That is issue 0466's finding (c) in
# a different script: a probe that breaks reports "fresh". Go's dirhash refuses
# rather than guesses (it rejects filenames containing newlines outright), and so
# does this.
#
# Usage:  source source-manifest.sh
#         nros_source_manifest <repo_root> <path>...   # prints the manifest
#         nros_source_signature <repo_root> <path>...  # prints its sha256

# Files that cannot change a build product. Everything else is hashed —
# including types nobody thought of, which is the entire point.
_NROS_MANIFEST_SKIP_GLOBS=(
    '*.md'          # documentation
    '*.gitignore'   # vcs metadata (build trees are excluded by git, not by us)
    '*.gitattributes'
)

# Print `<sha256>  <path>` for every tracked-or-untracked-unignored file under
# the given paths, sorted by path. Non-zero on ANY failure.
nros_source_manifest() {
    local repo_root="${1:?nros_source_manifest: repo_root}"
    shift
    [ "$#" -gt 0 ] || {
        echo "nros_source_manifest: at least one path is required" >&2
        return 2
    }

    # Enumerate through a temp FILE, not a process substitution. `done < <(cmd)`
    # discards cmd's exit status — a failed `git ls-files` would yield an empty
    # list and a perfectly valid-looking signature, which is the exact silent
    # failure this helper exists to remove. (Caught by
    # `check-source-manifest.sh`, which is why it asserts this.) A temp file
    # also preserves the NUL separators that command substitution strips.
    local _listing
    _listing="$(mktemp)" || return 1
    if ! git -C "$repo_root" ls-files -z --cached --others --exclude-standard -- "$@" > "$_listing"; then
        rm -f "$_listing"
        echo "nros_source_manifest: git ls-files failed under $repo_root" >&2
        return 1
    fi

    local -a files=()
    local rel skip glob
    while IFS= read -r -d '' rel; do
        skip=0
        for glob in "${_NROS_MANIFEST_SKIP_GLOBS[@]}"; do
            # shellcheck disable=SC2053  # glob match is intended
            [[ $rel == $glob ]] && {
                skip=1
                break
            }
        done
        [ "$skip" -eq 1 ] && continue
        # `sha256sum` output is line-oriented, so a newline in a path would make
        # the manifest ambiguous. Go's dirhash rejects these for the same reason.
        case "$rel" in
        *$'\n'*)
            echo "nros_source_manifest: filenames with newlines are not supported: $rel" >&2
            rm -f "$_listing"
            return 1
            ;;
        esac
        files+=("$rel")
    done < "$_listing"
    rm -f "$_listing"

    # An empty set is legitimate (a row whose dir holds only skipped files), but
    # it must not silently look the same as a broken enumeration — the caller
    # mixes the record in, so identical-empty manifests still differ per row.
    [ "${#files[@]}" -eq 0 ] && return 0

    _nros_hash_relpaths "$repo_root" "${files[@]}"
}

# ONE `sha256sum` for a whole set; its native output IS the manifest line
# format. `--` guards a path that begins with a dash. Sorting is by the path
# field under LC_ALL=C so the manifest is locale-independent.
_nros_hash_relpaths() {
    local repo_root="$1"
    shift
    (cd "$repo_root" && sha256sum -- "$@") | LC_ALL=C sort -k2 || {
        echo "nros_source_manifest: hashing failed under $repo_root" >&2
        return 1
    }
}

# phase-363 W4 — the row's MEASURED dependency closure.
#
# A compile-check row exists to compile AGAINST workspace crates, and those are
# not under its own `dir`. So the source manifest above — however complete for
# what it covers — is blind to exactly the edit that matters: issue 0466 records
# `packages/boards/nros-board-common/src/platform_config.rs` changing while the
# gate stayed silent and the tests caught it on mtime.
#
# Rather than guess that closure (a `cargo metadata` walk cannot even run here:
# a row's `Cargo.toml` carries `@NANO_ROS_ROOT@` placeholders until the builder
# stages it), take the one the compiler already wrote. Cargo emits dep-info
# `.d` per unit; the union of their in-repo entries IS the set the build read.
# Same move ninja makes with `.ninja_deps` and ccache with its manifest.
#
# Limits, stated because they are the same ones ccache documents:
#   * only as good as the LAST build — a file that does not exist yet cannot
#     appear, so the `dir` manifest stays as the belt to this suspenders;
#   * a dep dropped from the graph lingers until a rebuild — over-watching,
#     which fails safe;
#   * rows built by `cxx-syntax`, `cmake-configure` and `west-*` write no cargo
#     dep-info, so they get an empty closure and today's coverage. Extending to
#     them means `-MD` for the C++ syntax rows and `ninja -t deps` for the cmake
#     ones; both are follow-ups, not blockers.
#
# Paths under the build root are skipped: they are this build's own output, and
# hashing them would arm a rebuild on what the build just produced.
nros_dep_closure_manifest() {
    local repo_root="${1:?nros_dep_closure_manifest: repo_root}"
    local build_dir="${2:?nros_dep_closure_manifest: build_dir}"
    [ -d "$build_dir" ] || return 0

    local _closure
    _closure="$(mktemp)" || return 1
    if ! python3 "$(dirname "${BASH_SOURCE[0]}")/dep-closure.py" \
        "$repo_root" "$build_dir" > "$_closure"; then
        rm -f "$_closure"
        echo "nros_dep_closure_manifest: dep-closure extraction failed for $build_dir" >&2
        return 1
    fi
    local -a rels=()
    while IFS= read -r -d '' rel; do
        rels+=("$rel")
    done < "$_closure"
    rm -f "$_closure"
    [ "${#rels[@]}" -eq 0 ] && return 0
    _nros_hash_relpaths "$repo_root" "${rels[@]}"
}

# The manifest's own sha256 — what a signature embeds.
nros_source_signature() {
    local out
    out="$(nros_source_manifest "$@")" || return 1
    printf '%s' "$out" | sha256sum | awk '{print $1}'
}
