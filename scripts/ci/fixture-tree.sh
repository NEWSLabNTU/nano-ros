#!/usr/bin/env bash
#
# A PERSISTENT checkout for the self-hosted fixture lanes. phase-489 W6.
#
# usage: scripts/ci/fixture-tree.sh sync        bring the tree to this job's commit
#        scripts/ci/fixture-tree.sh path        print where the tree lives
#        scripts/ci/fixture-tree.sh --self-test
#
# WHAT THIS IS FOR
#
# The fixture lanes (run-matrix tier 1 / tier 2, the tier-2 nightly) spend
# 30-45 min in `just build <lane>`, and every run of them started cold:
# `actions/checkout` runs `git clean -ffdx` in `_work/<repo>/<repo>`, which
# deletes `build/`, every `generated/`, every leaf build dir and every target
# dir. sccache kept the rustc half warm and nothing else.
#
# A user's checkout is not like that. They pull and run `just build <lane>`
# again, and the build system decides what is stale. That is the chain this
# lane exists to exercise (the user's rule: CI runs the commands the way the
# user would — prereq -> build -> test), so the fix is to give the lane a tree
# that behaves like a user's, not to cache build outputs on the side.
#
# WHY NOT JUST `clean: false`
#
# The `_work/<repo>/<repo>` checkout is SHARED by every job on the runner, and
# queue L3 (merge-gating, several times a day) and the W2 gate siblings MUST
# start clean (issue 0196 / 1476: a gate may not pass on an earlier job's
# leftovers). Their `git clean -ffdx` would wipe a warm tree between two nightly
# runs, and `-ff` also removes a nested repository, so the tree cannot live
# inside that checkout either. It lives BESIDE it:
#
#     _work/<repo>/<repo>             actions/checkout, cleaned every job
#     _work/<repo>/nros-fixture-tree  this tree, kept (the `nros-runner-work`
#                                     volume outlives the runner container)
#
# The workflow names the same place as the job's run default,
# `${{ github.workspace }}/../nros-fixture-tree`;
# `check-fixture-tree-persistence` holds the two spellings together.
#
# WHAT `sync` DOES — exactly a user's `git pull` + `git submodule update`
#
#   * fetch this job's commit FROM the job's own checkout (local transport, no
#     network, no credentials) and check it out detached, `--force`: a tracked
#     file an earlier job modified or deleted is restored. Git rewrites ONLY the
#     paths whose content differs, so an unchanged source keeps its mtime and
#     the mtime-keyed freshness of ninja and cargo stays meaningful — the same
#     property a user's pull has. (A rewritten file always gets "now", newer
#     than any output, so a pull backwards in history rebuilds too.)
#   * `git submodule update --recursive --force` for the submodules an earlier
#     job initialised, so none sits at an old pin while a provisioning verb
#     reports it `[already present (skip)]` (the trap self-hosted-clean-tree.sh
#     describes). Which submodules exist is still `just setup <scope>`'s call.
#   * KEEP every untracked and ignored file. That is the point: `build/`,
#     `generated/`, leaf build dirs, the fixture stamp. None of it is trusted —
#     `just build <lane>` re-derives freshness for every row (cargo/ninja own
#     edges, the codegen signatures, `probe_inputs_key`), clears the fixture
#     stamp before building and rewrites it only after a full build, and the
#     test half's `_require-fixtures` + `_check-fixtures-stale` re-probe every
#     fixture it runs. A stale artifact rebuilds; a missing one builds.
#
# A PERIODIC COLD RUN. The incremental path is what users take; the cold path
# is what a fresh clone takes, and both must work. When the tree is older than
# NROS_FIXTURE_TREE_MAX_AGE_DAYS (default 7) every untracked and ignored file
# in it and in its submodules is removed — object stores are kept — and the
# age restarts. That also bounds what the tree can accumulate.
#
# One job runs on a runner at a time, so the tree has one writer.
set -euo pipefail

say() { printf 'fixture-tree: %s\n' "$*"; }
die() { printf 'fixture-tree: FAIL — %s\n' "$*" >&2; exit 1; }

TREE_NAME=nros-fixture-tree
BORN_MARK=nros-fixture-tree-born   # under .git/, so no clean removes it

# The tree for a checkout at $1: a SIBLING of it, never inside it.
tree_for() { printf '%s/%s' "$(dirname "$1")" "$TREE_NAME"; }

sync_tree() {
    local src="$1" tree="$2" max_age_days="$3"
    [ -e "$src/.git" ] || die "no checkout at $src"
    src="$(cd "$src" && pwd -P)"
    mkdir -p "$tree"
    tree="$(cd "$tree" && pwd -P)"
    case "$tree/" in
        "$src"/*) die "the tree ($tree) is inside the job checkout ($src): its \`git clean -ffdx\` would remove it" ;;
    esac
    local want
    want="$(git -C "$src" rev-parse HEAD)"

    local fresh=0
    if [ ! -e "$tree/.git" ]; then
        say "creating $tree (first run on this runner: the build is cold)"
        git init -q "$tree"
        fresh=1
    elif ! git -C "$tree" rev-parse --git-dir >/dev/null 2>&1 \
        || ! git -C "$tree" rev-parse --is-shallow-repository >/dev/null 2>&1; then
        # Unreadable git METADATA (a killed job, a corrupt store). The build
        # output is not affected: re-create the git dir alone; the checkout
        # below restores tracked content over the worktree.
        say "the tree's git dir is unreadable — re-initialising it (build output kept)"
        rm -rf "$tree/.git"
        git init -q "$tree"
        fresh=1
    fi
    # Paths inside the git dir are ASKED of git, never modelled (issue 1336).
    local born_mark lock
    born_mark="$(git -C "$tree" rev-parse --path-format=absolute --git-path "$BORN_MARK")"
    lock="$(git -C "$tree" rev-parse --path-format=absolute --git-path index.lock)"
    [ "$fresh" -eq 0 ] || date +%s > "$born_mark"
    # A lock at job start is a killed job's: one job per runner, one writer.
    if [ -f "$lock" ]; then
        say "removing a stale index.lock (an earlier job was killed mid-checkout)"
        rm -f "$lock"
    fi

    local born now age_days
    born="$(cat "$born_mark" 2>/dev/null || echo 0)"
    now="$(date +%s)"
    age_days=$(( (now - born) / 86400 ))
    if [ "$age_days" -ge "$max_age_days" ]; then
        say "tree is ${age_days}d old (limit ${max_age_days}d): periodic COLD run —"
        say "  removing every untracked and ignored file; object stores kept"
        git -C "$tree" clean -ffdxq
        if [ -f "$tree/.gitmodules" ]; then
            git -C "$tree" submodule foreach --quiet --recursive 'git clean -ffdxq' || true
        fi
        born="$now"
        echo "$born" > "$born_mark"
    else
        say "tree is ${age_days}d old (cold reset at ${max_age_days}d): incremental run"
    fi

    local before=none
    before="$(git -C "$tree" rev-parse -q --verify HEAD 2>/dev/null || echo none)"
    # The tree is shaped like the checkout actions/checkout made: the same
    # `origin` URL and the same `refs/remotes/origin/*` (gates read
    # `origin/main` as their baseline — submodule-pins does), and SHALLOW
    # wherever that checkout is (`--update-shallow`), so an ancestry question
    # gets the same NOT VERIFIED answer it gets there (issue 1476), never a
    # verdict from history the tree happens to remember.
    local url
    url="$(git -C "$src" remote get-url origin 2>/dev/null || true)"
    if [ -n "$url" ]; then
        git -C "$tree" remote get-url origin >/dev/null 2>&1 \
            && git -C "$tree" remote set-url origin "$url" \
            || git -C "$tree" remote add origin "$url"
    fi
    # `--depth=1`, like actions/checkout's own fetch: the tree decides its
    # shallow boundary itself rather than inheriting whatever the job
    # checkout's object store remembers. (Without it, run 38336863204 died in
    # the fetch: "Failed to traverse parents of commit …".)
    fetch_head() {
        git -C "$tree" fetch -q --depth=1 --no-tags --prune "$src" \
            "+HEAD:refs/nros/fixture-tree-head" "+refs/remotes/origin/*:refs/remotes/origin/*"
    }
    if ! fetch_head; then
        # Readable but unusable (a store missing objects its refs name).
        say "the tree's git state is unusable — re-initialising its git dir (build output kept)"
        rm -rf "$tree/.git"
        git init -q "$tree"
        born_mark="$(git -C "$tree" rev-parse --path-format=absolute --git-path "$BORN_MARK")"
        echo "$born" > "$born_mark"
        [ -z "$url" ] || git -C "$tree" remote add origin "$url"
        fetch_head
    fi
    [ "$(git -C "$tree" rev-parse refs/nros/fixture-tree-head)" = "$want" ] \
        || die "fetched $(git -C "$tree" rev-parse refs/nros/fixture-tree-head), want $want"
    # Refresh the index FIRST. A file whose stat moved but whose content did not
    # (a touch, a restore) is otherwise "dirty" to `checkout --force`, which then
    # rewrites it with a new mtime and re-stales everything built from it —
    # the treadmill a user's pull does not have. Measured in --self-test.
    git -C "$tree" update-index -q --refresh >/dev/null 2>&1 || true
    git -C "$tree" checkout -q --force --detach "$want"
    local got
    got="$(git -C "$tree" rev-parse HEAD)"
    [ "$got" = "$want" ] || die "tree is at $got after checkout, want $want"
    # `checkout --force` restores tracked content; refuse to go on if it did not.
    if [ -n "$(git -C "$tree" status --porcelain --untracked-files=no --ignore-submodules=all)" ]; then
        git -C "$tree" status --short --untracked-files=no --ignore-submodules=all >&2
        die "tracked files still differ from $want after the checkout"
    fi
    if [ "$before" = none ]; then
        say "checked out $want"
    elif [ "$before" = "$want" ]; then
        say "already at $want (a re-run of the same commit)"
    else
        say "moved $before -> $want ($(git -C "$tree" diff --name-only "$before" "$want" 2>/dev/null | wc -l) path(s) rewritten; the rest keep their mtimes)"
    fi

    # Initialised submodules follow the new pins, like a user's
    # `git submodule update` after a pull. Uninitialised ones stay that way.
    if [ -f "$tree/.gitmodules" ] && [ -n "$(git -C "$tree" submodule status 2>/dev/null | grep -v '^-' || true)" ]; then
        git -C "$tree" submodule sync -q --recursive
        git -C "$tree" submodule update --recursive --force
    fi

    if [ -n "${GITHUB_ENV:-}" ]; then
        echo "NROS_FIXTURE_TREE=$tree" >> "$GITHUB_ENV"
    fi
    say "ready: $tree"
}

self_test() {
    local t
    t="$(mktemp -d)"
    trap 'rm -rf "$t"' RETURN
    export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t
    local ok=0 bad=0
    pass() { echo "  ok   $*"; ok=$((ok + 1)); }
    fail() { echo "  FAIL $*"; bad=$((bad + 1)); }

    local ws="$t/_work/repo/repo" tree
    mkdir -p "$ws"
    git init -q "$ws"
    printf 'a\n' > "$ws/a.c"; printf 'b\n' > "$ws/b.c"
    git -C "$ws" add a.c b.c && git -C "$ws" commit -qm one
    tree="$(tree_for "$ws")"
    [ "$tree" = "$t/_work/repo/$TREE_NAME" ] && pass "tree is a sibling of the checkout" \
        || fail "tree_for gave $tree"

    sync_tree "$ws" "$tree" 7 >/dev/null
    [ "$(git -C "$tree" rev-parse HEAD)" = "$(git -C "$ws" rev-parse HEAD)" ] \
        && pass "first sync checks out the job's commit" || fail "first sync HEAD"

    # Build output survives; a tracked edit an earlier job left is undone.
    mkdir -p "$tree/build"; echo out > "$tree/build/obj"
    echo junk >> "$tree/a.c"; rm "$tree/b.c"
    touch -d '2001-01-01' "$tree/build/obj"
    sync_tree "$ws" "$tree" 7 >/dev/null
    [ -f "$tree/build/obj" ] && pass "untracked build output is kept" || fail "build output removed"
    [ "$(cat "$tree/a.c")" = a ] && [ -f "$tree/b.c" ] \
        && pass "a modified/deleted tracked file is restored" || fail "tracked state not restored"

    # A new commit rewrites only what changed: the unchanged file keeps its mtime.
    touch -d '2002-02-02' "$tree/a.c" "$tree/b.c"
    printf 'b2\n' > "$ws/b.c"; git -C "$ws" commit -qam two
    sync_tree "$ws" "$tree" 7 >/dev/null
    [ "$(stat -c %Y "$tree/a.c")" = "$(date -d '2002-02-02' +%s)" ] \
        && pass "an unchanged source keeps its mtime across a pull" || fail "unchanged a.c rewritten"
    [ "$(stat -c %Y "$tree/b.c")" -gt "$(date -d '2002-02-02' +%s)" ] && [ "$(cat "$tree/b.c")" = b2 ] \
        && pass "a changed source is rewritten with a newer mtime" || fail "changed b.c not rewritten"

    # Inside the checkout is refused: the checkout's clean would remove it.
    if (sync_tree "$ws" "$ws/inner" 7) >/dev/null 2>&1; then
        fail "a tree inside the checkout was accepted"
    else
        pass "a tree inside the checkout is refused"
    fi

    # Past the age limit the next sync is a cold run.
    local mark
    mark="$(git -C "$tree" rev-parse --path-format=absolute --git-path "$BORN_MARK")"
    echo 0 > "$mark"
    sync_tree "$ws" "$tree" 7 >/dev/null
    [ ! -e "$tree/build/obj" ] && [ -f "$tree/a.c" ] \
        && pass "an over-age tree is reset to tracked content (cold run)" || fail "age reset"
    [ "$(( $(date +%s) - $(cat "$mark") ))" -lt 60 ] \
        && pass "the reset restarts the age" || fail "age not restarted"

    # Shaped like the job's checkout: shallow where it is, origin/* mirrored.
    local deep="$t/deep" sh="$t/_work/sh/sh" shtree
    git init -q "$deep"
    for i in 1 2 3; do echo "$i" > "$deep/f"; git -C "$deep" add f; git -C "$deep" commit -qm "c$i"; done
    mkdir -p "$(dirname "$sh")"
    git clone -q --depth=1 "file://$deep" "$sh"
    shtree="$(tree_for "$sh")"
    sync_tree "$sh" "$shtree" 7 >/dev/null
    [ "$(git -C "$shtree" rev-parse --is-shallow-repository)" = true ] \
        && pass "a shallow job checkout gives a shallow tree (issue 1476)" || fail "tree not shallow"
    [ "$(git -C "$shtree" rev-parse -q --verify refs/remotes/origin/HEAD 2>/dev/null || true)" \
        = "$(git -C "$sh" rev-parse HEAD)" ] && [ "$(git -C "$shtree" remote get-url origin)" = "file://$deep" ] \
        && pass "origin URL and refs/remotes/origin/* mirror the job checkout" || fail "origin not mirrored"

    # An inherited GIT_DIR must not redirect the sync into another repository.
    local victim="$t/victim" vhead
    git init -q "$victim"; echo v > "$victim/v"; git -C "$victim" add v; git -C "$victim" commit -qm v
    vhead="$(git -C "$victim" rev-parse HEAD)"
    printf 'b3\n' > "$ws/b.c"; git -C "$ws" commit -qam three
    if GIT_DIR="$victim/.git" GITHUB_WORKSPACE="$ws" bash "$0" sync >/dev/null 2>&1 \
        && [ "$(git -C "$victim" rev-parse HEAD)" = "$vhead" ] && [ -f "$victim/v" ] \
        && [ "$(git -C "$tree" rev-parse HEAD)" = "$(git -C "$ws" rev-parse HEAD)" ] && [ -f "$tree/a.c" ]; then
        pass "an inherited GIT_DIR does not redirect the sync (issues 0986/0988)"
    else
        fail "an inherited GIT_DIR redirected the sync"
    fi
    if (bash "$0") >/dev/null 2>&1; then fail "a bare invocation did something"; else pass "a bare invocation refuses"; fi

    # A tree whose git metadata is broken (a killed job) recovers; its build
    # output survives the recovery.
    mkdir -p "$tree/build"; echo out > "$tree/build/kept"
    rm -rf "$(git -C "$tree" rev-parse --path-format=absolute --git-path objects)"
    (sync_tree "$ws" "$tree" 7) >/dev/null 2>&1 || true
    [ "$(git -C "$tree" rev-parse HEAD 2>/dev/null)" = "$(git -C "$ws" rev-parse HEAD)" ] \
        && [ -f "$tree/build/kept" ] && [ "$(cat "$tree/b.c")" = b3 ] \
        && pass "a tree with a broken git store recovers, build output kept" \
        || fail "broken git store not recovered"

    echo "fixture-tree self-test: $ok ok, $bad failed"
    [ "$bad" -eq 0 ]
}

# An inherited GIT_DIR (a hook, a linked worktree, a probe) overrides
# `git -C <tree>`: `checkout --force` would then write ANOTHER repository's
# commit into the tree's worktree — measured on the first CI run of this
# script, where check-hook-repo-side-effects ran it with a victim repo's
# GIT_DIR and the tracked files of the real tree were replaced mid-build.
# Cleared for every command (issues 0986/0988).
# shellcheck source=scripts/lib/git-hook-env.sh
. "$(dirname "$0")/../lib/git-hook-env.sh"
nros_clear_inherited_git_env

# No default command: a bare invocation (a probe, a typo) must do nothing.
cmd="${1:-}"
case "$cmd" in
    sync)
        [ -n "${GITHUB_WORKSPACE:-}" ] || die "GITHUB_WORKSPACE is unset (this is a CI step)"
        sync_tree "$GITHUB_WORKSPACE" "$(tree_for "$GITHUB_WORKSPACE")" \
            "${NROS_FIXTURE_TREE_MAX_AGE_DAYS:-7}"
        ;;
    path)
        [ -n "${GITHUB_WORKSPACE:-}" ] || die "GITHUB_WORKSPACE is unset"
        tree_for "$GITHUB_WORKSPACE"; echo
        ;;
    --self-test) self_test ;;
    *) die "usage: fixture-tree.sh sync | path | --self-test (got '${cmd}')" ;;
esac
