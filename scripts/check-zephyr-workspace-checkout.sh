#!/usr/bin/env bash
# Refuse a Zephyr workspace that belongs to a DIFFERENT nano-ros checkout —
# either because it lives inside one, or because its west manifest (the nano-ros
# Zephyr module every image compiles) IS one (issue 1253).
#
# phase-431 W1 gave the CLI an ownership guard: if the directory `nros` is
# operating on sits inside a nano-ros checkout, the running binary must be that
# checkout's own `packages/cli/target/**` build. A foreign binary emits with ITS
# OWN codegen, which can differ from this checkout's while carrying the same
# codegen version — the version catches an incompatible emitter, not one that
# merely moved.
#
# The guard is right. WHERE it fires is the problem. A shared west workspace
# nested inside a second checkout is a property of the HOST, decided once at
# provisioning time, but the refusal arrives ~15 minutes into a tier-2 fixture
# build, inside a cmake configure, as an error about a binary — naming neither
# the workspace, nor the second checkout, nor the fix. On the self-hosted runner
# that shape cost the tier-2 lane two consecutive nights (2026-09-06, -07):
#
#   == zephyr == FAILED (rc=2)
#   Error: this `nros` does not belong to the checkout it is being run against.
#   FATAL ERROR: ... -B/mnt/<disk>/<user>/nano-ros/zephyr-workspace/...
#
# (the path is written generically on purpose: `<user>/nano-ros` spelled out
# is a forbidden string here, since it reads as a GitHub org that is not ours)
#
# ...while the checkout under test was /home/aeon/actions-runner/_work/... .
# Nothing in this repository selects that path (it is the runner's own
# environment), so a reader of the tree could not discover the cause from it —
# which is why the check belongs here, where the resolution happens.
#
# The three conditions below MIRROR the guard's exactly, so this can never be
# stricter than the thing it front-runs: a workspace outside any checkout is
# fine (the guard's own first silent case), our own checkout is fine, and a
# checkout carrying no `packages/cli` sources is fine because nothing there
# could have built a CLI to be foreign to. `NROS_SKIP_STALE_CHECK=1` disables
# the guard, so it disables this too — otherwise this would fail a host that
# has deliberately opted out.
#
# A SECOND question, independent of the CLI guard (tier-2, 2026-09-09): which
# checkout's nano-ros Zephyr MODULE does this workspace compile? West lists the
# manifest repo (`[manifest] path` in `.west/config`, usually a `nano-ros`
# symlink back to the checkout that ran `west init -l`) as the `nros` module,
# and `zephyr/CMakeLists.txt` sets `NROS_REPO_DIR` to that module's parent. So
# every Zephyr image built against a workspace provisioned by ANOTHER checkout
# compiles that checkout's `zephyr/`, platform sources and nros-cpp headers,
# whatever commit it is at, beside this tree's generated entry code. Run
# 34319241943 compiled `<other>/packages/api/nros-cpp/include/nros/main.hpp`
# into a leaf of this tree and failed; the next night, same SHA, it built.
# A mixed tree is not a result about either tree. That question does not
# depend on the CLI guard, so `NROS_SKIP_STALE_CHECK=1` does NOT silence it.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"

# The checkout a west workspace's manifest resolves to, or empty when the
# manifest is not a nano-ros checkout (a user workspace whose manifest is
# their own app repo pulls nano-ros in some other way; not this check's case).
manifest_checkout_of() {
    local ws="$1" cfg path dir
    cfg="$ws/.west/config"
    [ -f "$cfg" ] || return 0
    path="$(awk '
        /^\[/ { in_manifest = ($0 ~ /^\[manifest\][[:space:]]*$/); next }
        in_manifest && $1 == "path" { sub(/^[^=]*=[[:space:]]*/, ""); print; exit }
    ' "$cfg")"
    [ -n "$path" ] || return 0
    dir="$(cd "$ws/$path" 2>/dev/null && pwd -P)" || return 0
    [ -f "$dir/zephyr/module.yml" ] && [ -f "$dir/packages/core/nros-core/Cargo.toml" ] || return 0
    printf '%s' "$dir"
}

module_problems=0
# The ONE resolver (phase-440 W1) — the same tree `just zephyr` and the fixture
# builders compile against, never a fourth ladder.
# shellcheck source=scripts/lib/zephyr-workspace.sh
. "$repo_root/scripts/lib/zephyr-workspace.sh"
zephyr_ws="$(nros_zephyr_ws_resolve_abs "" "$repo_root" 2>/dev/null || true)"
if [ -n "$zephyr_ws" ] && [ -d "$zephyr_ws" ]; then
    module_checkout="$(manifest_checkout_of "$zephyr_ws")"
    if [ -n "$module_checkout" ] && [ "$module_checkout" != "$repo_root" ]; then
        module_problems=1
        cat >&2 <<EOF
  zephyr workspace: $(cd "$zephyr_ws" && pwd -P)
      its west manifest (the nano-ros Zephyr module) is $module_checkout
      this tree:                                         $repo_root

Every Zephyr image built against that workspace compiles $module_checkout's
\`zephyr/\` module, platform sources and nros-cpp headers — not this tree's —
next to entry code THIS tree generates. Whatever that checkout's commit is
decides the result, so a failure (or a pass) is not a fact about this tree.

Build against a workspace whose manifest is this checkout: the in-tree
\`zephyr-workspace/\` from \`just zephyr setup\`, or, on a runner, the contained
runner (\`just runner-up <labels>\`), which provisions it into its own checkout.
EOF
    fi
fi

if [ "${NROS_SKIP_STALE_CHECK:-}" = "1" ]; then
    exit "$module_problems"
fi

# EVERY provisioned root, not just Zephyr (phase-440 W5). The ownership guard
# does not care which tree it is handed: any path inside a FOREIGN checkout is
# refused, so a check that asks about one root reports a property of that root
# rather than of the host. esp-idf reached this shape too and nobody would have
# heard about it until a fixture build fifteen minutes in.
#
# Each entry is `NAME:ENV_VAR:candidate[:candidate...]`, and a root with no env
# override leaves that field empty. `RFC-0095 D2` folds these into one
# store-resolved root, at which point this list collapses to that root and the
# per-tree spellings go with it — until then the list is the honest shape,
# because the tree really does carry four of them.
PROVISIONED_ROOTS="
zephyr:NROS_ZEPHYR_WORKSPACE:zephyr-workspace:../nano-ros-workspace:../nano-ros-workspace-4.4
esp-idf:NROS_ESP_IDF_WORKSPACE:esp-idf-workspace
external:${NROS_EXTERNAL_DIR_UNSET:-}:external
"

# The ownership guard's own marker and its lexical walk upward. Kept identical to
# `stale_guard.rs` on purpose: a check that front-runs a guard must never be
# STRICTER than the guard, or it refuses a configuration that would have worked.
owner_of() {
    local dir="$1" owner=""
    while [ -n "$dir" ] && [ "$dir" != "/" ]; do
        if [ -f "$dir/packages/core/nros-core/Cargo.toml" ]; then owner="$dir"; break; fi
        dir="$(dirname "$dir")"
    done
    printf '%s' "$owner"
}

problems=0
while IFS= read -r entry; do
    [ -n "$entry" ] || continue
    name="${entry%%:*}"
    rest="${entry#*:}"
    var="${rest%%:*}"
    cands="${rest#*:}"

    root=""
    [ -n "$var" ] && root="$(eval "printf '%s' \"\${$var:-}\"")"
    if [ -z "$root" ]; then
        IFS=':' read -ra _c <<< "$cands"
        for cand in "${_c[@]}"; do
            [ -d "$repo_root/$cand" ] && root="$repo_root/$cand" && break
        done
    fi
    # Absent is not this check's business — the caller already warns about that.
    [ -n "$root" ] && [ -d "$root" ] || continue

    # Resolve symlinks: a root symlinked onto a big disk reaches the same second
    # checkout as an absolute override, and the build tools hand cmake the
    # resolved path either way.
    root_real="$(cd "$root" 2>/dev/null && pwd -P)" || continue

    owner="$(owner_of "$root_real")"
    [ -n "$owner" ] || continue                          # outside any checkout — fine
    [ "$owner" != "$repo_root" ] || continue             # our own checkout — fine
    [ -f "$owner/packages/cli/Cargo.toml" ] || continue  # no CLI sources — nothing to be foreign to

    problems=$((problems + 1))
    printf '%s\n' "  $name: $root_real" >&2
    printf '%s\n' "      owned by $owner" >&2
done <<< "$PROVISIONED_ROOTS"

[ "$problems" -eq 0 ] && exit "$module_problems"

cat >&2 <<EOF
The provisioned root(s) listed above sit inside a DIFFERENT nano-ros
checkout, so every \`nros\` invocation against them will be refused by the
phase-431 W1 ownership guard — not here, but deep inside the fixture build.

  this tree: $repo_root

That second checkout has \`packages/cli\`, so the guard resolves ownership to
IT and expects $owner/packages/cli/target/**/nros — never this tree's build.

WHERE THIS COMES FROM, AND THE FIX THAT IS NOT ABOUT DIRECTORIES

On a self-hosted runner this shape means the job is running DIRECTLY ON THE
HOST, where the layout is whatever that machine happens to carry. The
sanctioned way to run one is contained — \`just runner-up <labels>\` /
\`scripts/ci/runner-container.sh\` — and there the problem cannot arise: the
work tree is the named volume \`nros-runner-work\` at /home/runner/_work with
no host bind mounts, and the Zephyr workspace is provisioned by
\`runner-provision.sh\` (the \`nros-sdk-zephyr\` label runs \`just setup
zephyr\`) INTO THAT CHECKOUT. A workspace the checkout owns is the guard's
"our own checkout" case, so it is silent by construction rather than by
anyone remembering where to put a directory.

The fix is to move the JOB, not the workspace:

  just runner-up nros-qemu,nros-sdk-zephyr,nros-big

This check deliberately suggests NO path. A bare-host runner is not a
supported configuration to repair — it is the configuration that produced
this — and it does not know that machine's disks, where a wrong suggestion
is worse than none.

Do NOT reach for NROS_SKIP_STALE_CHECK=1. It silences the guard whose whole
job is to stop a foreign CLI emitting different codegen under the same
version — a loud failure traded for a quiet wrong answer.
EOF
exit 1
