#!/usr/bin/env bash
# phase-360 W3/W4 — self-test for the ONE source-signature helper.
#
# The properties below are the ones whose absence cost real investigations, so
# they are asserted rather than trusted:
#
#   1. NO TYPE FILTER. The predecessor hashed an extension allowlist and dropped
#      `.conf` (Zephyr Kconfig — issues 0167 and 0466, a kernel dump each),
#      `.msg` (codegen input) and a `.json` that is a custom Rust TARGET SPEC.
#      Nobody decided those were not inputs; they were simply not on the list.
#   2. Build output cannot leak in. Enumeration goes through the git index, so
#      an ignored tree is invisible — this is what makes (1) safe.
#   3. Order is content-determined, not filesystem-determined.
#   4. A dep-info closure is parsed as Make syntax, escaped spaces included. A
#      naive `.split()` truncates a path with a space and silently SHRINKS the
#      closure, which hashes to a perfectly valid-looking signature.
#   5. Failures are fatal. The predecessor swallowed errors into a shorter
#      stream (issue 0466 finding (c): a probe that breaks reports "fresh").
#
# Hermetic: everything happens in a throwaway git repo under $TMPDIR, so a
# failure here can never leave the real worktree dirty.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/build/source-manifest.sh
source "$repo_root/scripts/build/source-manifest.sh"

fail=0
ok() { echo "  ok    $1"; }
bad() {
    echo "  FAIL  $1" >&2
    fail=1
}

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

sandbox="$tmp/repo"
mkdir -p "$sandbox/leaf/sub" "$sandbox/leaf/target/debug"
(
    cd "$sandbox"
    git init -q .
    git config user.email t@t.t
    git config user.name t
    printf 'target/\n' > .gitignore
)
printf 'CONFIG_A=y\n' > "$sandbox/leaf/prj.conf"
printf 'int32 v\n' > "$sandbox/leaf/sub/Echo.msg"
printf '{"arch":"riscv32"}\n' > "$sandbox/leaf/spec.json"
printf 'MEMORY {}\n' > "$sandbox/leaf/memory.x"
printf '# docs\n' > "$sandbox/leaf/README.md"
printf 'BUILD OUTPUT\n' > "$sandbox/leaf/target/debug/artifact.o"
(cd "$sandbox" && git add -A >/dev/null 2>&1 && git commit -qm init)

echo "check-source-manifest: no type filter"
manifest="$(nros_source_manifest "$sandbox" leaf)" || bad "manifest failed"
for f in prj.conf sub/Echo.msg spec.json memory.x; do
    if grep -q " leaf/$f\$" <<< "$manifest"; then ok "$f is hashed"; else bad "$f was DROPPED"; fi
done

echo "check-source-manifest: exclusions"
grep -q "README.md" <<< "$manifest" && bad ".md should be skipped" || ok ".md skipped"
grep -q "artifact.o" <<< "$manifest" && bad "gitignored build output LEAKED" || ok "build output excluded"

echo "check-source-manifest: content, not mtime"
before="$(nros_source_signature "$sandbox" leaf)"
touch "$sandbox/leaf/prj.conf"
[ "$(nros_source_signature "$sandbox" leaf)" = "$before" ] \
    && ok "an mtime bump with identical bytes is NOT a change" \
    || bad "mtime bump changed the signature (the treadmill returns)"
printf 'CONFIG_A=n\n' > "$sandbox/leaf/prj.conf"
[ "$(nros_source_signature "$sandbox" leaf)" != "$before" ] \
    && ok "a content edit IS a change" \
    || bad "a .conf content edit did not move the signature"

echo "check-source-manifest: deterministic order"
a="$(nros_source_manifest "$sandbox" leaf)"
b="$(nros_source_manifest "$sandbox" leaf)"
[ "$a" = "$b" ] && ok "stable across runs" || bad "manifest is not deterministic"

echo "check-source-manifest: dep-info closure"
dep_src="$sandbox/leaf/sub/with space.rs"
printf 'fn a() {}\n' > "$dep_src"
(cd "$sandbox" && git add -A >/dev/null 2>&1 && git commit -qm sp)
# Cargo escapes an embedded space as `\ ` — the case a naive split truncates.
printf '%s: %s %s\n' \
    "$sandbox/leaf/target/debug/x" \
    "$sandbox/leaf/prj.conf" \
    "${dep_src// /\\ }" > "$sandbox/leaf/target/debug/x.d"
closure="$(nros_dep_closure_manifest "$sandbox" "$sandbox/leaf/target/debug")" || bad "closure failed"
grep -q "leaf/prj.conf\$" <<< "$closure" && ok "closure picks up a listed dep" || bad "closure missed a dep"
grep -q "with space.rs\$" <<< "$closure" \
    && ok "an escaped space is one path, not two" \
    || bad "escaped space truncated the closure"

echo "check-source-manifest: errors are fatal"
nros_source_manifest "$sandbox" >/dev/null 2>&1 && bad "no-path call should fail" || ok "no-path call fails"
nros_source_manifest "$tmp/not-a-repo" leaf >/dev/null 2>&1 \
    && bad "enumeration outside a repo should fail" \
    || ok "enumeration failure is fatal"

if [ "$fail" -ne 0 ]; then
    echo "check-source-manifest: FAILED" >&2
    exit 1
fi
echo "check-source-manifest: OK"
