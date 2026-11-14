#!/usr/bin/env bash
# `scripts/ci/gate-tier.py` names the diff three ways (phase-489 W12): the
# local tree, a merge-queue batch (`--range <base> <head>`, two trees) and a
# pull request (`--pull-request`, the REST files list). Its self-test covers
# the classifier; this covers the two CI front ends that feed it, where every
# doubt must come out `full`. The negative control is the one the queue skip
# rests on: documentation plus ONE `.rs` file is `full` on a merge group.
#
# The range cases build a throwaway repository; the pull-request cases run
# against a stub `gh` on PATH that answers from canned text.
set -uo pipefail
cd "$(dirname "$0")/.."
# shellcheck source=scripts/lib/git-hook-env.sh
source scripts/lib/git-hook-env.sh
nros_clear_inherited_git_env
here="$PWD"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
fail=0

check() { # <name> <want> <got-output>
    local got="${3%%$'\n'*}"
    if [ "$got" = "$2" ]; then echo "  ok   $1"; else
        echo "  FAIL $1: want '$2', got '$3'"; fail=1; fi
}

# ---- merge-queue batch: a tree diff between base_sha and head_sha ----------
repo="$tmp/repo"
git init -q "$repo"
g() { git -C "$repo" -c user.name=t -c user.email=t@t -c commit.gpgsign=false "$@"; }
mkdir -p "$repo/docs/guides" "$repo/src"
printf 'a\n' > "$repo/docs/guides/a.md"
printf 'fn main() {}\n' > "$repo/src/lib.rs"
g add -A && g commit -qm base
base="$(g rev-parse HEAD)"
range() { python3 "$here/scripts/ci/gate-tier.py" --range "$1" "$2" --repo "$repo"; }

printf 'b\n' >> "$repo/docs/guides/a.md"; printf 'n\n' > "$repo/notes.md"
g add -A && g commit -qm docs
docs="$(g rev-parse HEAD)"
check "a batch of documentation only: docs" docs "$(range "$base" "$docs")"

printf '// x\n' >> "$repo/src/lib.rs"
g add -A && g commit -qm code
check "negative control: documentation plus one .rs: full" full "$(range "$base" "$(g rev-parse HEAD)")"

g mv src/lib.rs lib.md && g commit -qm rename
check "a .rs renamed to .md counts the deleted .rs: full" full "$(range "$docs" "$(g rev-parse HEAD)")"

check "a base that is not here (unfetched): full" full \
    "$(range 0123456789abcdef0123456789abcdef01234567 "$docs")"
check "an empty batch: full" full "$(range "$docs" "$docs")"

# `--fetch` brings an absent base in. It must NEVER cut a full clone's history
# (`git fetch --depth=1` there writes `.git/shallow`, which a persistent
# workspace keeps for every later run — 2026-11-15 took a local clone's main to
# 2 commits), and on a clone that is shallow already it may pass depth.
g config uploadpack.allowAnySHA1InWant true
g checkout -q -b side "$base"
printf 'side\n' > "$repo/side.md"; g add -A && g commit -qm side
side="$(g rev-parse HEAD)"; g checkout -q -
git clone -q --single-branch --branch "$(g rev-parse --abbrev-ref HEAD)" "file://$repo" "$tmp/full"
git clone -q --depth=1 "file://$repo" "$tmp/shallow"
fetched() { python3 "$here/scripts/ci/gate-tier.py" --range "$1" "$2" --repo "$3" --fetch; }
out="$(fetched "$side" "$(git -C "$tmp/full" rev-parse HEAD)" "$tmp/full")"
if [[ "$out" == *"could not"* ]]; then check "a full clone fetches the absent base" "a verdict" "$out"
else check "a full clone fetches the absent base" "${out%%$'\n'*}" "$out"; fi
check "negative control: --fetch leaves a FULL clone unshallow" false \
    "$(git -C "$tmp/full" rev-parse --is-shallow-repository)"
check "a shallow clone (depth 1) fetches the base: full" full \
    "$(fetched "$base" "$(git -C "$tmp/shallow" rev-parse HEAD)" "$tmp/shallow")"
check "and the verdict is measured, not a fetch failure" 0 \
    "$(fetched "$base" "$(git -C "$tmp/shallow" rev-parse HEAD)" "$tmp/shallow" | grep -c 'could not' || true)"

# ---- pull request: the REST files list, counted ---------------------------
mkdir -p "$tmp/bin"
cat > "$tmp/bin/gh" <<'STUB'
#!/usr/bin/env bash
[ "${STUB_FAIL:-0}" = 1 ] && { echo "HTTP 502" >&2; exit 1; }
case "$*" in
  */files*) printf '%b' "$STUB_FILES" ;;
  *) printf '%s\n' "$STUB_COUNT" ;;
esac
STUB
chmod +x "$tmp/bin/gh"
pr() { env PATH="$tmp/bin:$PATH" "$@" python3 "$here/scripts/ci/gate-tier.py" --pull-request o/r 1; }

check "a pull request of documentation only: docs" docs \
    "$(pr STUB_COUNT=2 STUB_FILES='docs/guides/cpp-api.md\t\nbook/src/a.md\t\n')"
check "negative control: documentation plus one .rs: full" full \
    "$(pr STUB_COUNT=2 STUB_FILES='docs/guides/cpp-api.md\t\nsrc/lib.rs\t\n')"
check "a rename whose OLD name is .rs: full" full \
    "$(pr STUB_COUNT=1 STUB_FILES='lib.md\tsrc/lib.rs\n')"
check "a files list shorter than changed_files (truncated): full" full \
    "$(pr STUB_COUNT=3001 STUB_FILES='docs/guides/cpp-api.md\t\n')"
check "the API fails: full" full "$(pr STUB_FAIL=1 STUB_COUNT=1 STUB_FILES='x.md\t\n')"

if [ "$fail" -ne 0 ]; then
    echo "gate-tier-diff tests: FAILED" >&2
    exit 1
fi
echo "gate-tier-diff tests: OK (14 cases)"
