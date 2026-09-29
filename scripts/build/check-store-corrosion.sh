# A gate that configures a `find_package(nano_ros)` probe project needs the
# pinned Corrosion IN THE SDK STORE — issue 1553.
#
# Without it `nros_resolve_corrosion` (cmake/NanoRosCorrosion.cmake) falls
# through to a `FetchContent` clone into ONE shared host fetch cache, and two
# such gates in one parallel `check-fast` lane clone into the same directory at
# once. Measured in CI on PR #1354, three runs in two days, two different
# losers' symptoms: `destination path 'corrosion-src' already exists` and a
# `Parse error` on a half-written `CMakeLists.txt`.
#
# A gate therefore never reaches the network for Corrosion. With the store
# empty it SKIPS through the ledger (the `no in-tree CLI` shape its sibling
# preconditions already have, so a pristine worktree stays green) and names the
# one command that fills it. Serialising the gates or locking the fetch was
# ruled out in the issue: both keep every probe gate depending on a clone at
# configure time, and would hide an empty store rather than fill it. CI fills
# it (`gate.yml`, "Provision Corrosion"), so there the gates RUN.
#
# The question is asked of the CLI (`nros setup --check --tool corrosion`), the
# one place that knows the pinned version and CONSTRUCTS its store prefix
# (issue 1546), rather than re-deriving that path here.
#
# Usage (inside a `set -euo pipefail` recipe, after the CLI precondition):
#   source scripts/build/check-store-corrosion.sh
#   nros_gate_require_store_corrosion <gate> <nros-cli> || exit 0
# Returns 0 = run the gate, 1 = skipped and recorded; EXITS 1 when the CLI
# cannot answer the question at all.

# shellcheck source=scripts/build/check-skip.sh
source "$(dirname "${BASH_SOURCE[0]}")/check-skip.sh"
# shellcheck source=scripts/lib/grep-q.sh
source "$(dirname "${BASH_SOURCE[0]}")/../lib/grep-q.sh"

nros_gate_require_store_corrosion() {
    local gate="${1:?nros_gate_require_store_corrosion: gate}"
    local cli="${2:?nros_gate_require_store_corrosion: nros cli}"
    local out rc=0
    out="$("$cli" setup --check --tool corrosion 2>&1)" || rc=$?
    # Three answers, and only two of them are about the store. The CLI can also
    # refuse to answer at all -- measured: a CLI left stale by a branch switch
    # exits non-zero with `in-tree nros CLI is STALE` before reading anything.
    # Reading every non-zero exit as "not provisioned" turned that into a SKIP
    # blaming an empty store that was in fact full. So skip only on the CLI's
    # positive MISSING verdict, and fail loudly on anything else.
    if [ "$rc" -eq 0 ]; then
        return 0
    fi
    local missing=0
    nros_grep_q '\[MISSING\] *tool *corrosion' <<<"$out" || missing=$?
    if [ "$missing" -eq 0 ]; then
        nros_check_skip "$gate" "the pinned Corrosion is not in the SDK store -- run \`just workspace install-corrosion\` (a gate never clones it: issue 1553)"
        return 1
    fi
    printf '%s\n' "$out" >&2
    echo "${gate}: \`nros setup --check --tool corrosion\` could not say whether the store holds the pinned Corrosion (rc=${rc}; output above)" >&2
    exit 1
}
