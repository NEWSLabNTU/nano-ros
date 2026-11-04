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

# 9. REAL west, end to end — the runner's exact shape. Rows 1-8 stub west, so
# they prove the script CALLS `west update`; they cannot prove that an update
# against the refreshed manifest fetches a module the old one lacked. That is
# west's behaviour, and the fix rests on it, so it is measured here: a
# workspace provisioned when the imported manifest's `name-allowlist` held only
# `modA`, then the checkout's manifest adds `modB` (phase-524 W2 adding
# `hal_stm32`). A bare `west update` over the OLD copy must leave `modB` absent
# (the defect, asserted so a pass means something), and the sync must fetch it.
# Local `file://` repos, no network. Needs `west`; without it this row is
# reported NOT VERIFIED through the skip ledger, never passed.
source "$repo_root/scripts/build/zephyr-python.sh"
real_py="$(NROS_PYTHON= nros_zephyr_python)"
if [ -n "$real_py" ] && PATH="$(dirname "$real_py"):$PATH" command -v west >/dev/null 2>&1; then
    real_bin="$(dirname "$real_py")"
    up="$tmp/up"
    gitq() {
        env -u GIT_DIR -u GIT_WORK_TREE -u GIT_INDEX_FILE \
            GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t \
            GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_NOSYSTEM=1 git "$@"
    }
    mkrepo() {   # mkrepo <name> <file> <content>
        gitq init -q -b main "$up/$1"
        printf '%s\n' "$3" > "$up/$1/$2"
        gitq -C "$up/$1" add "$2"
        gitq -C "$up/$1" commit -qm init
    }
    mkrepo modA f modA
    mkrepo modB f modB
    mkrepo zephyr west.yml "manifest:
  projects:
    - name: modA
      url: file://$up/modA
      revision: main
    - name: modB
      url: file://$up/modB
      revision: main"
    manifest() {   # manifest <allowlisted module>...
        printf 'manifest:\n  projects:\n    - name: zephyr\n      url: file://%s/zephyr\n' "$up"
        printf '      revision: main\n      import:\n        name-allowlist:\n'
        for m in "$@"; do printf '          - %s\n' "$m"; done
        printf '  self:\n    path: nano-ros\n'
    }
    rm -rf "$tmp/ws" "$tmp/co"
    mkdir -p "$tmp/ws/nano-ros" "$tmp/co"
    manifest modA > "$tmp/ws/nano-ros/west.yml"
    west_real() { (cd "$tmp/ws" && env -u GIT_DIR -u GIT_WORK_TREE PATH="$real_bin:$PATH" west "$@"); }
    if west_real init -l --mf west.yml nano-ros >/dev/null 2>&1 && west_real update >/dev/null 2>&1 \
        && [ -d "$tmp/ws/modA" ]; then
        check "real west: a workspace provisioned on the old allowlist has modA" ok
        manifest modA modB > "$tmp/co/west.yml"
        west_real update >/dev/null 2>&1 || true
        [ ! -e "$tmp/ws/modB" ] && r=ok || r=bad
        check "...a bare west update over its OWN copy never fetches modB (the defect)" "$r"
        rc=0
        out="$(NROS_PYTHON="$real_py" PATH="$real_bin:$PATH" env -u GIT_DIR -u GIT_WORK_TREE \
            bash "$sync" "$tmp/ws" "$tmp/co" 2>&1)" || rc=$?
        [ "$rc" -eq 0 ] && [ -f "$tmp/ws/modB/f" ] && r=ok || r=bad
        check "...and the sync fetches the module the checkout's manifest added (rc=$rc)" "$r"
        [ "$r" = ok ] || printf '%s\n' "$out" | sed 's/^/        /'
    else
        check "real west: could not provision the synthetic workspace" bad
    fi
else
    # shellcheck source=scripts/build/check-skip.sh
    . "$repo_root/scripts/build/check-skip.sh"
    nros_check_skip zephyr-sync-workspace-manifest \
        "real-west row NOT VERIFIED: no interpreter that imports west (rows 1-8 ran with west stubbed)"
fi

echo
echo "$pass passed, $fail failed"
[ "$fail" -eq 0 ]
