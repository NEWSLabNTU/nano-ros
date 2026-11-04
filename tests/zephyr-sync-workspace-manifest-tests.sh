#!/usr/bin/env bash
# Issue 2249 — an existing west workspace must follow the checkout's manifest.
#
# Drives `scripts/zephyr/sync-workspace-manifest.sh` against a synthetic
# workspace (a `.west/config` plus the manifest project's copy) with `west`
# stubbed on PATH, recording each call. The stub dir is also what
# `nros_zephyr_activate` prepends (via NROS_PYTHON), so no real venv is
# reached. Buildless, tmpdir, <1 s.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
sync="$repo_root/scripts/zephyr/sync-workspace-manifest.sh"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

pass=0
fail=0
check() {
    if [ "$2" = ok ]; then
        pass=$((pass + 1)); echo "  ok    $1"
    else
        fail=$((fail + 1)); echo "  FAIL  $1"
    fi
}

mkdir -p "$tmp/bin"
# Records "<cwd> <args>" per call; exits $STUB_WEST_RC (default 0).
cat > "$tmp/bin/west" <<'EOF'
#!/usr/bin/env bash
printf '%s %s\n' "$PWD" "$*" >> "$STUB_WEST_LOG"
exit "${STUB_WEST_RC:-0}"
EOF
chmod +x "$tmp/bin/west"

# fresh <copy-content|-> : a workspace whose manifest project is `nano-ros`
# and file `west.yml`; `-` means the copy is missing. Checkout holds "new".
fresh() {
    rm -rf "$tmp/ws" "$tmp/co"
    mkdir -p "$tmp/ws/.west" "$tmp/ws/nano-ros" "$tmp/co"
    printf '[manifest]\npath = nano-ros\nfile = west.yml\n' > "$tmp/ws/.west/config"
    [ "$1" = - ] || printf '%s\n' "$1" > "$tmp/ws/nano-ros/west.yml"
    printf 'new\n' > "$tmp/co/west.yml"
    : > "$tmp/west.log"
}
run() {   # -> rc, out
    rc=0
    out="$(STUB_WEST_LOG="$tmp/west.log" STUB_WEST_RC="${1:-0}" \
        NROS_PYTHON="$tmp/bin/python3" PATH="$tmp/bin:$PATH" \
        bash "$sync" "$tmp/ws" "$tmp/co" 2>&1)" || rc=$?
}
calls() { wc -l < "$tmp/west.log" | tr -d ' '; }

# 1. Current copy: no network, rc 0.
fresh new; run
[ "$rc" -eq 0 ] && [ "$(calls)" = 0 ] && check "a current manifest runs no west update" ok \
    || check "a current manifest runs no west update (rc=$rc calls=$(calls))" bad

# 2. Stale copy (the runner's case): copy replaced, one `west update` in the workspace.
fresh old; run
[ "$rc" -eq 0 ] && check "a stale manifest syncs (rc 0)" ok || check "a stale manifest syncs (rc=$rc: $out)" bad
[ "$(cat "$tmp/ws/nano-ros/west.yml")" = new ] && r=ok || r=bad
check "...the workspace copy is the checkout's now" "$r"
[ "$(cat "$tmp/west.log")" = "$tmp/ws update" ] && r=ok || r=bad
check "...and exactly one 'west update' ran, in the workspace" "$r"
run
[ "$(calls)" = 1 ] && r=ok || r=bad
check "...and a second run is a no-op" "$r"

# 3. Missing copy: written, then updated.
fresh -; run
[ "$rc" -eq 0 ] && [ "$(cat "$tmp/ws/nano-ros/west.yml")" = new ] && [ "$(calls)" = 1 ] && r=ok || r=bad
check "a missing copy is written and updated" "$r"

# 4. west update fails: old copy restored, failure propagates, next run retries.
fresh old; run 3
[ "$rc" -eq 3 ] && r=ok || r=bad
check "a failed west update fails the sync (rc=$rc)" "$r"
[ "$(cat "$tmp/ws/nano-ros/west.yml")" = old ] && r=ok || r=bad
check "...and restores the previous copy" "$r"
compgen -G "$tmp/ws/.west/*.prev.*" >/dev/null && r=bad || r=ok
check "...leaving no backup file behind" "$r"
run 0
[ "$(calls)" = 2 ] && [ "$(cat "$tmp/ws/nano-ros/west.yml")" = new ] && r=ok || r=bad
check "...so the next run retries instead of reading it as current" "$r"

# 5. A failed update on a MISSING copy leaves it missing (still stale next run).
fresh -; run 3
[ "$rc" -eq 3 ] && [ ! -e "$tmp/ws/nano-ros/west.yml" ] && r=ok || r=bad
check "a failed update over a missing copy leaves it missing" "$r"

# 6. No .west/config: not a workspace we repair.
rm -rf "$tmp/ws"; mkdir -p "$tmp/ws"; : > "$tmp/west.log"; run
[ "$rc" -eq 0 ] && [ "$(calls)" = 0 ] && r=ok || r=bad
check "a dir with no .west/config is left alone" "$r"

# 7. `file` absent from the config defaults to west.yml, like west itself.
fresh old; printf '[manifest]\npath = nano-ros\n' > "$tmp/ws/.west/config"; run
[ "$rc" -eq 0 ] && [ "$(cat "$tmp/ws/nano-ros/west.yml")" = new ] && r=ok || r=bad
check "an unset manifest file defaults to west.yml" "$r"

# 8. The 4.4 line: `file = west-4.4.yml` compares that file, not west.yml.
fresh new; printf '[manifest]\npath = nano-ros\nfile = west-4.4.yml\n' > "$tmp/ws/.west/config"
printf 'old44\n' > "$tmp/ws/nano-ros/west-4.4.yml"; printf 'new44\n' > "$tmp/co/west-4.4.yml"; run
[ "$rc" -eq 0 ] && [ "$(cat "$tmp/ws/nano-ros/west-4.4.yml")" = new44 ] && [ "$(calls)" = 1 ] && r=ok || r=bad
check "the config's manifest file is the one compared" "$r"

echo
echo "$pass passed, $fail failed"
[ "$fail" -eq 0 ]
