#!/usr/bin/env bash
#
# Issue 2878 -- `scripts/zephyr/tracing-custom-backend-patch.sh` against
# synthetic `subsys/tracing/` trees of both Zephyr lines' shapes:
#
#   3.7  the backend name is an `#elif` chain in tracing_core.c; the patch adds
#        the two Kconfig symbols and one `#elif`.
#   4.4  the name is the promptless Kconfig string TRACING_BACKEND_NAME; the
#        patch adds the two symbols and one `default` of that string, and
#        leaves tracing_core.c alone.
#
# Each line: check mode names the missing edit (exit 1, tree untouched), apply,
# second apply says "already applied" and changes nothing, check mode passes;
# a half-applied tree and a tree without the anchors are refused. The live
# witness, an image whose own backend receives the CTF stream, is
# `just zephyr test-tracing-custom-backend` (needs a provisioned workspace).
# ~1 s, tmpdir, no Zephyr checkout needed.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
patch="$root/scripts/zephyr/tracing-custom-backend-patch.sh"
# shellcheck source=scripts/lib/grep-q.sh
source "$root/scripts/lib/grep-q.sh"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

fail() {
    echo "[FAIL] $*" >&2
    exit 1
}

run() {
    rc=0
    out="$("$@" 2>&1)" || rc=$?
}

# The backend choice and its neighbours, as both lines have them.
choice() {
    cat <<'K'
choice
	prompt "Tracing Backend"
	default TRACING_BACKEND_UART

config TRACING_BACKEND_UART
	bool "UART backend"

config TRACING_BACKEND_POSIX
	bool "Posix architecture (native) backend"
	depends on TRACING_SYNC

config TRACING_BACKEND_RAM
	bool "RAM backend"

endchoice
K
}

mk37() {
    local d="$1/zephyr/subsys/tracing"
    mkdir -p "$d"
    { choice; printf '\nconfig RAM_TRACING_BUFFER_SIZE\n\tint "Ram Tracing buffer size"\n'; } > "$d/Kconfig"
    cat > "$d/tracing_core.c" <<'C'
#if defined CONFIG_TRACING_BACKEND_UART
#define TRACING_BACKEND_NAME "tracing_backend_uart"
#elif defined CONFIG_TRACING_BACKEND_RAM
#define TRACING_BACKEND_NAME "tracing_backend_ram"
#else
#define TRACING_BACKEND_NAME ""
#endif
C
}

mk44() {
    local d="$1/zephyr/subsys/tracing"
    mkdir -p "$d"
    {
        choice
        printf '\nconfig TRACING_BACKEND_NAME\n\tstring\n'
        printf '\tdefault "tracing_backend_uart" if TRACING_BACKEND_UART\n'
        printf '\tdefault "tracing_backend_ram" if TRACING_BACKEND_RAM\n'
        printf '\nconfig RAM_TRACING_BUFFER_SIZE\n\tint "Ram Tracing buffer size"\n'
    } > "$d/Kconfig"
    printf '\tworking_backend = tracing_backend_get(CONFIG_TRACING_BACKEND_NAME);\n' > "$d/tracing_core.c"
}

for line in 3.7 4.4; do
    ws="$tmp/ws-$line"
    "mk${line/./}" "$ws"
    t="$ws/zephyr/subsys/tracing"
    cp -r "$t" "$tmp/orig-$line"

    run env NROS_PATCH_CHECK=1 bash "$patch" "$ws"
    [ "$rc" -eq 1 ] || fail "$line: check mode on an unpatched tree exited $rc:
$out"
    [[ "$out" == *MISSING* ]] || fail "$line: check mode did not name the edit: $out"
    diff -r "$tmp/orig-$line" "$t" >/dev/null || fail "$line: check mode wrote to the tree"

    run bash "$patch" "$ws"
    [ "$rc" -eq 0 ] || fail "$line: apply exited $rc:
$out"
    kc="$t/Kconfig"
    core="$t/tracing_core.c"
    # The member sits INSIDE the choice, the name symbol after it.
    python3 - "$kc" "$line" <<'PY' || fail "$line: Kconfig shape wrong: $(cat "$kc")"
import sys
kc, line = open(sys.argv[1]).read(), sys.argv[2]
member = kc.index("config TRACING_BACKEND_CUSTOM\n\tbool")
end = kc.index("\nendchoice\n")
name = kc.index("config TRACING_BACKEND_CUSTOM_NAME\n\tstring")
assert member < end < name, (member, end, name)
assert "depends on TRACING_BACKEND_CUSTOM\n" in kc[name:]
want = "\tdefault TRACING_BACKEND_CUSTOM_NAME if TRACING_BACKEND_CUSTOM\n"
assert (kc.count(want) == 1) == (line == "4.4"), line
PY
    if [ "$line" = 3.7 ]; then
        python3 - "$core" <<'PY' || fail "3.7: tracing_core.c shape wrong: $(cat "$core")"
import sys
c = open(sys.argv[1]).read()
e = c.index("#elif defined CONFIG_TRACING_BACKEND_CUSTOM\n"
            "#define TRACING_BACKEND_NAME CONFIG_TRACING_BACKEND_CUSTOM_NAME\n")
assert e < c.index("#else\n"), "the #elif must precede the #else"
PY
    else
        cmp -s "$core" "$tmp/orig-$line/tracing_core.c" || fail "4.4: tracing_core.c was edited"
    fi

    cp -r "$t" "$tmp/once-$line"
    run bash "$patch" "$ws"
    [ "$rc" -eq 0 ] && [[ "$out" == *"already applied"* ]] ||
        fail "$line: second apply not reported as applied (rc $rc): $out"
    diff -r "$tmp/once-$line" "$t" >/dev/null || fail "$line: second apply changed the tree"
    run env NROS_PATCH_CHECK=1 bash "$patch" "$ws"
    [ "$rc" -eq 0 ] || fail "$line: check mode on a patched tree exited $rc: $out"
done

# Half-applied (3.7: Kconfig carries it, tracing_core.c does not): refused.
cp "$tmp/orig-3.7/tracing_core.c" "$tmp/ws-3.7/zephyr/subsys/tracing/tracing_core.c"
run bash "$patch" "$tmp/ws-3.7"
[ "$rc" -eq 1 ] && [[ "$out" == *"some of"* ]] || fail "half-applied tree not refused (rc $rc): $out"

# No anchors (a tree whose backend choice moved): refused, not mis-patched.
bad="$tmp/ws-bad/zephyr/subsys/tracing"
mkdir -p "$bad"
printf 'config TRACING\n\tbool "Tracing"\n' > "$bad/Kconfig"
: > "$bad/tracing_core.c"
run bash "$patch" "$tmp/ws-bad"
[ "$rc" -eq 1 ] && [[ "$out" == *"anchor not found"* ]] || fail "anchorless tree not refused (rc $rc): $out"

# Both patch sets run it.
for set in 3.7 4.4; do
    rc=0
    nros_grep_q -F 'tracing-custom-backend-patch.sh "$WORKSPACE"' "$root/scripts/zephyr/patches/$set.sh" || rc=$?
    case "$rc" in
        0) ;;
        1) fail "scripts/zephyr/patches/$set.sh does not apply the patch" ;;
        *) fail "could not read scripts/zephyr/patches/$set.sh (grep exit $rc)" ;;
    esac
done

echo "zephyr-tracing-custom-backend-patch: OK (3.7 #elif + 4.4 Kconfig name, check mode, idempotent, half-applied and anchorless refused, both sets run it)"
