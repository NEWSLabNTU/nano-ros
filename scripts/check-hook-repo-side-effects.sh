#!/usr/bin/env bash
# A git hook, and everything it reaches, must leave the invoking repository
# byte-identical — issues 0986, 0988.
#
# THE RULE
#
#   A script reachable from a git hook must not modify the invoking repository —
#   not its config, not its index, not its object store, not the mtime of a
#   tracked file — whatever git puts in the environment.
#
# The rule says "a script", not "a shell script". This gate read only
# `scripts/**/*.sh` and `.githooks/*` until 2026-09-08, so a PYTHON gate that
# built a throwaway repository was outside its reach entirely — the 0196 shape
# (a gate narrower than the rule it enforces), noted in issue 1229's "Not done
# here" and closed here. Measured at the time of widening, over the 251 tracked
# Python files under `scripts/`: four build a repository, and all four were
# DIRTY under a hook environment, including the one that cleared by hand. Both
# languages are enumerated below, run through their own interpreter, and
# credited by the same identifier — `nros_clear_inherited_git_env`, which now
# has a Python spelling in `scripts/lib/git_hook_env.py` beside the shell one.
#
# The enumeration is still `scripts/**` + `.githooks/*`, and that boundary was
# measured rather than assumed: no `tests/*.sh` (0 of 17), no `just` recipe body
# and no workflow `run:` block anywhere in the tree builds a repository. A gate
# tier reaches those through a script, and the script is what is checked here.
#
# WHY IT NEEDS A GATE AND NOT JUST A FIX
#
# 0986 was three scripts building throwaway repositories in their selftests. An
# inherited `GIT_DIR` overrides BOTH a path argument and `git -C`, so the
# selftests ran against the CALLER's repo: `git init "$tmp/work"` wrote
# `core.bare = true` into it, and `update-index --add --cacheinfo 160000,...`
# staged a gitlink carrying a deliberately invalid sha. The hook whose job is
# refusing bad submodule pins was staging one.
#
# Three properties made it invisible to every check this repo had:
#
#   1. It does not reproduce by hand. An interactive shell has no GIT_DIR, so
#      `bash .githooks/pre-push` passes and only git running it does damage.
#   2. It is self-masking. Run 1 sets core.bare=true; from run 2 the hook dies
#      at `rev-parse --show-toplevel` BEFORE reaching the selftests, so the
#      symptom presents as a broken repository rather than a bad script.
#   3. A selftest cannot catch it, because the selftests ARE the thing that
#      misbehaves — and they redirect to /dev/null and return a status.
#
# So the only thing that can catch it is running the code under the environment
# git actually supplies, against a repository whose every byte is known. That is
# this gate.
#
# WHICH ENVIRONMENT, MEASURED
#
# On git 2.34.1, an ordinary `git push` from an ordinary checkout hands the hook
# only `GIT_PREFIX=` — which is why "git always sets GIT_DIR" is the wrong
# reason to believe this matters. What actually sets it:
#
#   push from a LINKED WORKTREE          GIT_DIR=<common>/.git/worktrees/<name>
#   git --git-dir=X --work-tree=Y push   GIT_DIR + GIT_WORK_TREE
#   a caller that already exported it    GIT_DIR inherited unchanged
#
# A linked worktree is the normal shape for the parallel agent sessions this
# repo runs, and `$GIT_DIR/index` there IS that worktree's live index. Both
# shapes are exercised below, because they do DIFFERENT damage: with
# GIT_WORK_TREE also set, `git init` writes `core.worktree`; without it, it
# writes `core.bare = true`, which is the one 0986 measured.
#
# WHAT IS ASSERTED, AND WHAT IS NOT
#
# The victim repository is built here, so it can be compared byte-for-byte
# including mtimes. The REAL repo is not the assertion subject for the
# end-to-end hook run — it is only the working directory, because the hook exits
# at line 1 of its body against anything that is not this tree. The real repo's
# `.git/config` is checked (cheap, and nothing else writes it), and the
# per-script runs below carry the full-strength assertion with both the working
# directory AND the environment under this script's control.
#
# The hook's `just check fast` arm is skipped: it is deliberately about the
# caller's repo, it is separately gated, and running it here would recurse into
# this gate forever.
set -uo pipefail
cd "$(dirname "$0")/.."

# This gate runs inside `check fast`, which the hook itself runs — so it can be
# reached with the very environment it is testing for. Clear ours; the hostile
# environments below are handed to CHILDREN explicitly.
# shellcheck source=scripts/lib/git-hook-env.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")/lib" && pwd)/git-hook-env.sh"
nros_clear_inherited_git_env
# shellcheck source=scripts/lib/grep-q.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")/lib" && pwd)/grep-q.sh"

REPO="$PWD"
HOOK=".githooks/pre-push"
# The two spellings of the clearing helper. Both name the hazard in their own
# prose, so both match the marker below and both are excluded from it — the
# helper is what a match is told to CALL.
LIB="scripts/lib/git-hook-env.sh"
LIB_PY="scripts/lib/git_hook_env.py"
CLEAR_FN="nros_clear_inherited_git_env"

fail=0
bad() { echo "  FAIL  $1" >&2; fail=1; }
ok()  { echo "  ok    $1"; }

# ---------------------------------------------------------------------------
# The static half: a script that builds a throwaway repo must clear first.
# ---------------------------------------------------------------------------
#
# `git init` is the marker, not `git add`/`git commit`: plenty of scripts here
# operate on the real repo on purpose, and `git init` is the one invocation that
# is only ever about a repository the script is CREATING. Every 0986 offender
# has one. Text in, verdict out, so the selftest can feed it synthetic input.
#
# TWO spellings, because a language that does not write command lines as text
# does not write `git init` either. Python spells it `["git", "init", …]` or
# `git(tmp, "init", …)`, and the shell pattern alone found 0 of the 4 real
# Python offenders — a naive port of this check would have reported a clean
# sweep over the very files that measured DIRTY. The second alternative is
# "a line naming git that also carries `init` as a QUOTED ARGUMENT", which is
# narrow enough to leave prose alone: measured over the 251 tracked Python
# files it matches exactly those 4, and over the 240 shell files it adds
# nothing to what the first alternative already found. It also does not fire on
# `git submodule update --init`, which appears in four gates' user-facing
# advice — `--init` is not a quoted `init` token.
#
# What it deliberately does NOT flag: a script that runs git READ-ONLY against
# the repository (106 of the 251 Python files invoke git; 4 create one). Reading
# the wrong repository under an inherited GIT_DIR is a correctness bug and worth
# clearing for, but it is not a repository side effect, and a marker that fired
# on all 106 would be a gate nobody could keep green.
#
# needs_clear: 0 = builds a throwaway repo, 1 = does not.
needs_clear() {
    nros_grep_q -E -- \
        '(^|[^-[:alnum:]])git[[:space:]]+init([[:space:]]|$)|git.*['"'"'"]init['"'"'"]'
}
# has_clear: 0 = calls the shared clearing helper.
has_clear() { nros_grep_q -- "$CLEAR_FN"; }

# how_to_clear <file> — the fix, in the language of the file that needs it.
#
# ONE identifier in both, deliberately. Crediting "the file pops some GIT_
# variables" is what let `check-box-sync-covers-tracked-source.py` clear four of
# git's sixteen names and still leak `GIT_OBJECT_DIRECTORY` into a victim's
# object store; the helper asks `git rev-parse --local-env-vars`, so the list
# cannot be a subset and cannot drift.
how_to_clear() {
    case "$1" in
        *.py) printf '%s' "            sys.path.insert(0, os.path.join(ROOT, \"scripts\", \"lib\"))
            from git_hook_env import $CLEAR_FN
            $CLEAR_FN()" ;;
        *) printf '%s' "            . \"\$(cd \"\$(dirname \"\${BASH_SOURCE[0]}\")/…/lib\" && pwd)/git-hook-env.sh\"
            $CLEAR_FN" ;;
    esac
}

echo "check-hook-repo-side-effects: every throwaway-repo builder clears first"
hazardous=()
while IFS= read -r f; do
    [ -f "$f" ] || continue
    [ "$f" = "$LIB" ] && continue
    [ "$f" = "$LIB_PY" ] && continue
    needs_clear < "$f" || continue
    hazardous+=("$f")
    if has_clear < "$f"; then
        ok "$f clears the inherited git environment"
    else
        bad "$f builds a repository with \`git init\` and never calls $CLEAR_FN.
        An inherited GIT_DIR makes that \`git init\` rewrite the CALLER's
        repository instead of building a temp one (issue 0986). Add:
$(how_to_clear "$f")"
    fi
done < <(git ls-files 'scripts/*.sh' 'scripts/**/*.sh' 'scripts/*.py' 'scripts/**/*.py' '.githooks/*')

if [ "${#hazardous[@]}" -eq 0 ]; then
    bad "no script matched \`git init\` at all — the enumeration is broken,
        and a check that examines nothing reports OK over nothing."
fi

# ---------------------------------------------------------------------------
# The dynamic half: run them, and compare the victim byte for byte.
# ---------------------------------------------------------------------------

# snapshot <dir> — every path, type, size, mtime and content hash under <dir>.
#
# `find` over a temp tree we built; there is no git index to consult here, which
# is the case `check-no-tracked-file-find` exempts by construction.
snapshot() {
    (
        cd "$1" || return 1
        find . -printf '%P\t%y\t%s\t%T@\n' 2>/dev/null | LC_ALL=C sort
        find . -type f -print0 2>/dev/null | LC_ALL=C sort -z \
            | xargs -0 -r sha1sum 2>/dev/null | LC_ALL=C sort
    )
}

# build_victim <dir> — a repo shaped like the one 0986 measured on: a separate
# gitdir with `core.worktree` set and NO `bare` key, plus a linked worktree.
build_victim() {
    local d="$1"
    mkdir -p "$d/tree" "$d/gitdir" || return 1
    (
        set -e
        export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t
        export GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t
        git init -q --separate-git-dir="$d/gitdir" "$d/tree"
        git -C "$d/tree" config user.name t
        git -C "$d/tree" config user.email t@t
        printf 'victim\n' > "$d/tree/file.txt"
        git -C "$d/tree" add file.txt
        git -C "$d/tree" commit -qm init
        git -C "$d/tree" worktree add -q -b linked "$d/linked"
    ) >/dev/null 2>&1
}

# run_probe <shape> <cwd> <cmd…>
#
# Runs <cmd> with the hook environment of <shape> pointed at a freshly built
# victim, and prints CLEAN/DIRTY plus the diff. Exit status of <cmd> is
# deliberately IGNORED: the assertion is about side effects, and a script run
# outside the tree it expects will legitimately refuse.
#
# stdout: `CLEAN` or `DIRTY\n<diff>`. The probed command's own output goes to
# $PROBE_LOG and its status to the first line of $PROBE_RC_FILE — through files
# rather than variables because callers read the verdict with `$(…)`, and a
# variable set inside a command substitution dies with its subshell.
PROBE_LOG="$(mktemp)"
PROBE_RC_FILE="$(mktemp)"
trap 'rm -f "$PROBE_LOG" "$PROBE_RC_FILE"' EXIT
run_probe() {
    local shape="$1" cwd="$2"; shift 2
    local v before after rc
    v="$(mktemp -d)" || { echo "DIRTY"; echo "  mktemp failed"; return 0; }
    if ! build_victim "$v"; then
        echo "DIRTY"; echo "  could not build the victim repo"; rm -rf "$v"; return 0
    fi

    before="$(snapshot "$v")"
    local -a env_args
    case "$shape" in
        # Exactly what git hands a hook on a push from a linked worktree — the
        # shape that actually occurs. GIT_WORK_TREE is deliberately ABSENT.
        worktree) env_args=(
            "GIT_DIR=$v/gitdir/worktrees/linked"
            "GIT_PREFIX=" ) ;;
        # The superset a `git --git-dir=… --work-tree=…` invocation and the
        # commit-family hooks produce.
        explicit) env_args=(
            "GIT_DIR=$v/gitdir"
            "GIT_WORK_TREE=$v/tree"
            "GIT_INDEX_FILE=$v/gitdir/index"
            "GIT_OBJECT_DIRECTORY=$v/gitdir/objects"
            "GIT_PREFIX=" ) ;;
        *) echo "DIRTY"; echo "  unknown shape $shape"; rm -rf "$v"; return 0 ;;
    esac

    ( cd "$cwd" && env "${env_args[@]}" "$@" ) > "$PROBE_LOG" 2>&1
    rc=$?
    printf '%s\n' "$rc" > "$PROBE_RC_FILE"
    after="$(snapshot "$v")"

    if [ "$before" = "$after" ]; then
        echo "CLEAN"
    else
        echo "DIRTY"
        diff <(printf '%s\n' "$before") <(printf '%s\n' "$after") | sed 's/^/      /' | head -40
    fi
    rm -rf "$v"
}

# ---------------------------------------------------------------------------
# Selftest — the negative control, on the normal path (check-gate-selftests).
# ---------------------------------------------------------------------------
#
# Both halves, both directions. Without this the gate could pass by examining
# nothing, which is the failure mode it exists to prevent one level up.
self_test() {
    local t verdict errs=0
    t="$(mktemp -d)" || return 1

    # --- static half, both directions -----------------------------------
    printf '#!/usr/bin/env bash\ngit init -q "$tmp/x"\n' > "$t/dirty.sh"
    printf '#!/usr/bin/env bash\n%s\ngit init -q "$tmp/x"\n' "$CLEAR_FN" > "$t/clean.sh"
    printf '#!/usr/bin/env bash\ngit status\n' > "$t/none.sh"
    needs_clear < "$t/dirty.sh" || { echo "  selftest: missed a \`git init\`" >&2; errs=1; }
    needs_clear < "$t/none.sh"  && { echo "  selftest: flagged a script with no \`git init\`" >&2; errs=1; }
    has_clear   < "$t/dirty.sh" && { echo "  selftest: saw a clear that is not there" >&2; errs=1; }
    has_clear   < "$t/clean.sh" || { echo "  selftest: missed the clearing call" >&2; errs=1; }

    # The same four questions in Python, because the shell spelling answered
    # `no` to all four for every Python file in the tree. `prose.py` is the
    # false positive that would make this widening unaffordable: four gates
    # print `git submodule update --init` as advice, and none of them builds a
    # repository.
    printf 'import subprocess\nsubprocess.run(["git", "init", "-q", tmp])\n' > "$t/dirty.py"
    printf 'from git_hook_env import %s\n%s()\nsubprocess.run(["git", "init", tmp])\n' \
        "$CLEAR_FN" "$CLEAR_FN" > "$t/clean.py"
    printf 'print("run: git submodule update --init packages/x")\n' > "$t/prose.py"
    needs_clear < "$t/dirty.py" || {
        echo "  selftest: missed a Python \`git init\` — this is the widening" >&2; errs=1; }
    needs_clear < "$t/prose.py" && {
        echo "  selftest: flagged \`git submodule update --init\` in prose" >&2; errs=1; }
    has_clear   < "$t/dirty.py" && { echo "  selftest: saw a clear that is not there" >&2; errs=1; }
    has_clear   < "$t/clean.py" || { echo "  selftest: missed the Python clearing call" >&2; errs=1; }

    # The Python helper the credit above is granted for. It has its own
    # negative control (does its fallback list still cover what this git calls
    # repository-local?), and a helper that clears a SUBSET is the defect this
    # widening measured, so it is run here rather than trusted.
    if ! python3 "$REPO/$LIB_PY" >/dev/null 2>&1; then
        echo "  selftest: $LIB_PY --self-test FAILED — the Python clearing helper" >&2
        echo "       does not hold, so every Python credit above is unearned:" >&2
        python3 "$REPO/$LIB_PY" 2>&1 | sed 's/^/         /' >&2
        errs=1
    fi

    # --- dynamic half, both directions ----------------------------------
    # An offender in miniature: 0986's two damage sites, verbatim in shape.
    cat > "$t/offender.sh" <<'EOF'
#!/usr/bin/env bash
tmp="$(mktemp -d)"
git init -q "$tmp/work" >/dev/null 2>&1
git -C "$tmp/work" update-index --add --cacheinfo \
    "160000,0123456789abcdef0123456789abcdef01234567,dep" >/dev/null 2>&1
rm -rf "$tmp"
exit 0
EOF
    verdict="$(run_probe worktree "$t" bash "$t/offender.sh")"
    case "$verdict" in
        DIRTY*) ;;
        *) echo "  selftest: an offender was reported CLEAN. This gate cannot" >&2
           echo "       fail, so it is reporting nothing." >&2; errs=1 ;;
    esac
    verdict="$(run_probe explicit "$t" bash "$t/offender.sh")"
    case "$verdict" in
        DIRTY*) ;;
        *) echo "  selftest: an offender was reported CLEAN under \`explicit\`." >&2; errs=1 ;;
    esac

    # The same script, cleared: the probe must NOT cry wolf, or every real run
    # is noise and the gate gets disabled.
    { printf '#!/usr/bin/env bash\n. %q\n%s\n' "$REPO/$LIB" "$CLEAR_FN"
      tail -n +2 "$t/offender.sh"; } > "$t/fixed.sh"
    verdict="$(run_probe worktree "$t" bash "$t/fixed.sh")"
    case "$verdict" in
        CLEAN) ;;
        *) echo "  selftest: a CLEARED script was reported DIRTY — false positive:" >&2
           printf '%s\n' "$verdict" >&2; errs=1 ;;
    esac

    # --- the same, in Python --------------------------------------------
    #
    # Not a formality: `subprocess.run(["git", "init", …])` is a different
    # spelling of the same syscall, and a gate that runs every hazardous file
    # through `bash` would report a Python offender as a shell SYNTAX error
    # with a clean victim — i.e. CLEAN, loudly, for the wrong reason.
    cat > "$t/offender.py" <<'EOF'
import subprocess
import tempfile

tmp = tempfile.mkdtemp()
subprocess.run(["git", "init", "-q", tmp], capture_output=True)
open(tmp + "/f", "w").write("x\n")
subprocess.run(["git", "-C", tmp, "add", "f"], capture_output=True)
EOF
    for shape in worktree explicit; do
        verdict="$(run_probe "$shape" "$t" python3 "$t/offender.py")"
        case "$verdict" in
            DIRTY*) ;;
            *) echo "  selftest: a PYTHON offender was reported CLEAN under" >&2
               echo "       \`$shape\`. The widening to Python catches nothing." >&2
               errs=1 ;;
        esac
    done

    { printf 'import sys\nsys.path.insert(0, %q)\nfrom git_hook_env import %s\n%s()\n' \
        "$REPO/$(dirname "$LIB_PY")" "$CLEAR_FN" "$CLEAR_FN"
      cat "$t/offender.py"; } > "$t/fixed.py"
    verdict="$(run_probe worktree "$t" python3 "$t/fixed.py")"
    case "$verdict" in
        CLEAN) ;;
        *) echo "  selftest: a CLEARED Python script was reported DIRTY — false" >&2
           echo "       positive, which would make the widening unaffordable:" >&2
           printf '%s\n' "$verdict" >&2; errs=1 ;;
    esac

    rm -rf "$t"
    [ "$errs" -eq 0 ] || { echo "check-hook-repo-side-effects: SELFTEST FAILED" >&2; return 1; }
    echo "  ok    selftest: a shell AND a Python offender are caught in both shapes,
        their cleared twins are not, and the Python helper's own control holds"
    return 0
}

echo "check-hook-repo-side-effects: negative control"
self_test || fail=1

# ---------------------------------------------------------------------------
# Every hazardous script, under both hook environments.
# ---------------------------------------------------------------------------
#
# Argument sets are needed because two of them do their repo-building work only
# in a selftest. They are named here rather than derived: a script's own flag
# for "run your selftest" is not something the tree records anywhere else.
# `--changed HEAD` on the reachability script is not decoration: without a
# baseline it probes all 20 submodule pins over the NETWORK, which a fast gate
# cannot afford. Against HEAD nothing has moved, so it takes the
# skipped-unchanged path — after running its selftest, which is the 0986 site
# this is here to exercise.
# `gen-issue-index.py` takes `--self-test` for a second reason: its normal path
# WRITES `docs/issues/open.md` into the real tree, and a gate has no business
# regenerating a build artifact as a side effect of asking a question about
# it. `--self-test` is the arm that builds the throwaway repo, which is the
# 0986 site, and it is what the `issue-index` recipe runs first anyway.
probe_args() {
    case "$1" in
        scripts/reserve-claim.sh) echo "--selftest" ;;
        scripts/ci/submodule-commits-reachable.sh) echo "--changed HEAD" ;;
        scripts/gen-issue-index.py) echo "--self-test" ;;
        *) echo "" ;;
    esac
}

# interp <file> — the interpreter that file's callers use.
#
# The scripts are run rather than sourced, so the shebang would do; naming it
# keeps the probe's command line explicit and keeps a non-executable checkout
# (a fresh clone, a CI tarball) from changing the answer.
interp() {
    case "$1" in
        *.py) echo "python3" ;;
        *) echo "bash" ;;
    esac
}

echo "check-hook-repo-side-effects: the victim repo survives every hazardous script"
for f in "${hazardous[@]}"; do
    [ "$f" = "$HOOK" ] && continue                     # run end-to-end below
    [ "$f" = "scripts/check-hook-repo-side-effects.sh" ] && continue  # this file
    for shape in worktree explicit; do
        # The real repo is the working directory because that is where these run
        # from; the VICTIM is what the environment names, and it is the thing
        # compared. A script that ignored the environment entirely and wrote to
        # its cwd is out of this gate's reach and in `check fast`'s: all but
        # `gen-issue-index.py` are gates themselves and run there every push,
        # and that one runs from the `issue-index` gate beside them.
        # shellcheck disable=SC2046  # deliberate: an empty arg set must vanish
        verdict="$(run_probe "$shape" "$REPO" "$(interp "$f")" "$REPO/$f" $(probe_args "$f"))"
        case "$verdict" in
            CLEAN) ok "$f [$shape]" ;;
            *) bad "$f [$shape] MODIFIED the repository its environment named:
$(printf '%s\n' "$verdict" | tail -n +2)" ;;
        esac
    done
done

# ---------------------------------------------------------------------------
# The hook itself, end to end, with git-shaped stdin.
# ---------------------------------------------------------------------------
#
# stdin is `<local-ref> <local-sha> <remote-ref> <remote-sha>`, with local ==
# remote: that gets past the hook's branch guard and reaches every stage, while
# leaving nothing for the pin checks to diff — so no network, no fetch, and the
# throwaway-repo selftests (the 0986 sites) still run.
#
# ONE shape here, not two. A full hook run costs ~3.7 s and this gate sits on
# the fan-out's critical path, where the floor is a single gate. `worktree` is
# the shape that actually occurs (measured above); the other is covered where it
# is cheap — the per-script probes, which exercise the same clearing helper the
# hook calls.
echo "check-hook-repo-side-effects: the hook itself, under a hook environment"
head_sha="$(git rev-parse HEAD 2>/dev/null)"
if [ -z "$head_sha" ]; then
    bad "cannot resolve HEAD — refusing to report on a hook run that did not happen"
else
    cfg_before="$(sha1sum "$(git rev-parse --git-dir)/config" 2>/dev/null)"
    verdict="$(printf 'refs/heads/probe %s refs/heads/probe %s\n' "$head_sha" "$head_sha" \
        | run_probe worktree "$REPO" env NROS_SKIP_PREPUSH_CHECKS=1 \
              timeout 120 bash "$REPO/$HOOK" origin "$REPO")"
    case "$verdict" in
        CLEAN) ok "$HOOK [worktree] left the environment's repository untouched" ;;
        *) bad "$HOOK [worktree] MODIFIED the repository its environment named:
$(printf '%s\n' "$verdict" | tail -n +2)" ;;
    esac
    # The hook must have reached its LAST stage, or the run above proves less
    # than it appears to — a hook that exits at line 1 leaves nothing behind and
    # would report CLEAN forever. If this fires, an earlier stage refused:
    # `just check issue-ids` and the submodule pin checks are where to look, and
    # they are fast gates too, so they are red alongside this.
    if ! nros_grep_q -- 'fast gates SKIPPED' "$PROBE_LOG"; then
        bad "$HOOK exited before its final stage (rc=$(cat "$PROBE_RC_FILE")), so
        the stages after that point were not exercised. Its output:
$(sed 's/^/          /' "$PROBE_LOG" | head -20)"
    fi
    cfg_after="$(sha1sum "$(git rev-parse --git-dir)/config" 2>/dev/null)"
    if [ "$cfg_before" = "$cfg_after" ]; then
        ok "this repo's own .git/config is byte-identical across the hook runs"
    else
        bad "this repo's own .git/config CHANGED across a hook run — that is
        0986's exact symptom (core.bare=true), and it is live right now."
    fi
fi

if [ "$fail" -ne 0 ]; then
    echo "check-hook-repo-side-effects: FAILED" >&2
    exit 1
fi
echo "check-hook-repo-side-effects: OK"
