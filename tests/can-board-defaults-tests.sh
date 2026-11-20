#!/usr/bin/env bash
# tests/can-board-defaults-tests.sh -- RFC-0132 D3, the `board` backend's
# weak fail-loud defaults (`nros-board-common/c/nros_board_can_unsupported.c`).
#
# WHAT THIS MEASURES
#
# On a "board"-stack board the CAN frame driver is the board package's strong
# symbols over a vendor driver. An image whose board supplies none must fail
# at `open` with NROS_PLATFORM_CAN_ENOTSUP and one line naming the board, not
# at link and not silently. Compiled for the host with the host C compiler,
# through the board's own TU (`nros-board-s32z270-freertos/c/can_unsupported.c`,
# which names the board):
#
#   A  no strong symbols: open(NULL) returns ENOTSUP (-6) and the line names
#      `s32z270-freertos`; send and recv return ENOTSUP, fd_active is 0
#   B  a strong `nros_platform_can_open` in another object of the image wins:
#      open returns its value and the default's line is not printed
#   C  every in-tree board that compiles the defaults is a "board"-stack board
#      (a host or os stack has a real backend the weak symbols would shadow)
#
# Seconds. PRECONDITIONS ARE HARD FAILURES (issue 1758): no cc, no verdict.
#
# Usage: ./tests/can-board-defaults-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
# shellcheck source=../scripts/lib/grep-q.sh
source scripts/lib/grep-q.sh

CC="${CC:-cc}"
if ! command -v "$CC" >/dev/null 2>&1; then
    echo "can-board-defaults: FAIL: no C compiler ($CC)" >&2
    exit 1
fi

FAILURES=0
fail() { echo "  FAIL $*"; FAILURES=$((FAILURES + 1)); }
ok() { echo "  ok   $*"; }

T="$(mktemp -d)"
trap 'rm -rf "$T"' EXIT
INC="$ROOT/packages/platform/nros-platform-api/include"
BOARD_TU="$ROOT/packages/boards/nros-board-s32z270-freertos/c/can_unsupported.c"

cat >"$T/app.c" <<'C'
#include <stdio.h>
#include <nros/platform_can.h>
int main(void) {
    static nros_platform_can_t can;
    nros_platform_can_frame_t f = {0};
    int8_t o = nros_platform_can_open(&can, NULL, 0u);
    int8_t s = nros_platform_can_send(&can, &f, 0u);
    int8_t r = nros_platform_can_recv(&can, &f, 0u);
    printf("open=%d send=%d recv=%d fd=%u\n", o, s, r, nros_platform_can_fd_active(&can));
    return 0;
}
C
cat >"$T/strong.c" <<'C'
#include <nros/platform_can.h>
int8_t nros_platform_can_open(nros_platform_can_t* can, const char* device, uint8_t want_fd) {
    (void)can; (void)device; (void)want_fd;
    return NROS_PLATFORM_CAN_OK;
}
C
FLAGS=(-std=c11 -Wall -Wextra -Werror -I"$INC")

echo "A  no strong symbols"
if "$CC" "${FLAGS[@]}" "$BOARD_TU" "$T/app.c" -o "$T/a" 2>"$T/a.err"; then
    out="$("$T/a")"
    if nros_grep_q "board \`s32z270-freertos\` defines no nros_platform_can_" <<<"$out"; then
        ok "the line names the board"
    else
        fail "the line does not name the board: $out"
    fi
    if nros_grep_q "^open=-6 send=-6 recv=-6 fd=0\$" <<<"$out"; then
        ok "open, send and recv return ENOTSUP; fd_active is 0"
    else
        fail "unexpected codes: $out"
    fi
else
    fail "the board's defaults do not compile: $(cat "$T/a.err")"
fi

echo "B  a strong open wins"
if "$CC" "${FLAGS[@]}" "$BOARD_TU" "$T/strong.c" "$T/app.c" -o "$T/b" 2>"$T/b.err"; then
    out="$("$T/b")"
    rc_open=0
    nros_grep_q "^open=0 " <<<"$out" || rc_open=$?
    rc_line=0
    nros_grep_q "defines no" <<<"$out" || rc_line=$?
    if [ "$rc_open" -eq 0 ] && [ "$rc_line" -eq 1 ]; then
        ok "the strong definition is the one called"
    else
        fail "the weak default shadowed the strong one: $out"
    fi
else
    fail "the image with a strong open does not link: $(cat "$T/b.err")"
fi

echo "C  only board-stack boards compile the defaults"
nros_git_grep tus -l 'nros_board_can_unsupported.c' -- 'packages/boards/*/c/*.c'
[ -n "$tus" ] || fail "no board compiles the defaults"
while IFS= read -r tu; do
    [ -n "$tu" ] || continue
    dir="${tu%/c/*}"
    stack="$(sed -n 's/^stack *= *"\([a-z]*\)".*/\1/p' "$dir/nros-board.toml" | sort -u)"
    if [ "$stack" = board ]; then
        ok "${dir#"$ROOT"/}: stack = \"board\""
    else
        fail "${dir#"$ROOT"/} compiles the CAN defaults but its stack is \"$stack\""
    fi
done <<<"$tus"

if [ "$FAILURES" -ne 0 ]; then
    echo "can-board-defaults: $FAILURES failure(s)"
    exit 1
fi
echo "can-board-defaults: all assertions held"
