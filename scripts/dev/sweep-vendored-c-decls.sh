#!/usr/bin/env bash
# Issue 0383 class sweep, generalised beyond zenoh-pico.
#
# THE CLASS: a call whose declaring header is never included. C99 dropped
# implicit function declarations; gcc <= 13 warned, gcc >= 14 and clang >= 15
# reject. Two instances shipped in vendored zenoh-pico (`common/serial.h`,
# `config/custom.h`), both latent for years and both fatal the moment a
# contributor used a current compiler. CI runs Ubuntu LTS (gcc 11/13), so only a
# rolling-distro host sees them — this box is gcc 16.
#
# `-fsyntax-only`, so it is milliseconds per TU and links nothing.
#
# SCOPE, HONESTLY: this catches sites reachable WITHOUT a build context, which is
# where zenoh-pico's two instances lived. For trees whose TUs need a generated
# config or a port header (micro-xrce-dds-client, ThreadX) it compiles almost
# nothing, and the complementary check is to build them the way the build does:
#
#     CFLAGS="-Werror=implicit-function-declaration -Werror=int-conversion" \
#         cargo build -p zpico-sys -p cyclonedds-sys -p nros-rmw-xrce-cffi
#
# On gcc >= 14 both diagnostics are errors by DEFAULT, so a successful build of
# those crates on a current compiler already proves the class absent for whatever
# configuration that build selects. Run both; neither subsumes the other.
#
# CLASSIFICATION IS THE WHOLE TRICK. Most vendored TUs cannot compile standalone
# here: they want a port header, a generated config, or a platform include this
# sweep has no business synthesising. Treating "cannot compile" as a hit would
# bury the real signal, and treating it as a pass would be a lie. So each file
# lands in exactly one bucket:
#
#   HIT     — an implicit-declaration / int-conversion diagnostic. The class.
#   SKIP    — a missing header, i.e. the TU needs build context we do not have.
#   CLEAN   — compiled, no diagnostics of the class.
set -uo pipefail

cd "$(dirname "$0")/../.."
repo="$PWD"

sweep_tree() {
    local name="$1" root="$2"; shift 2
    local incs=("$@")
    [ -d "$root" ] || { printf '%-26s SKIPPED (not provisioned)\n' "$name"; return 0; }

    local args=()
    for i in "${incs[@]}"; do
        [ -d "$i" ] && args+=("-I" "$i")
    done

    local hits=0 skips=0 clean=0
    local hitlist=""
    while IFS= read -r f; do
        local err
        err="$(gcc -fsyntax-only -w \
            -Werror=implicit-function-declaration -Werror=int-conversion \
            "${args[@]}" "$f" 2>&1)"
        if [ -z "$err" ]; then
            clean=$((clean + 1)); continue
        fi
        if printf '%s' "$err" | grep -qE 'error:.*(implicit declaration|makes pointer from integer|int-conversion)'; then
            hits=$((hits + 1))
            hitlist+="    ${f#"$repo/"}"$'\n'
            printf '%s' "$err" | grep -E 'error:.*(implicit declaration|makes pointer from integer)' |
                head -2 | sed 's/^/      /' >>"$repo/tmp/sweep-hits.txt"
        elif printf '%s' "$err" | grep -qE 'fatal error:.*No such file'; then
            skips=$((skips + 1))
        else
            # compiled far enough to diagnose something else — not our class
            clean=$((clean + 1))
        fi
    done < <(find "$root" -name '*.c' -not -path '*/test*' -not -path '*/example*' | sort)

    printf '%-26s HIT=%-4s SKIP(no ctx)=%-5s CLEAN=%s\n' "$name" "$hits" "$skips" "$clean"
    [ -n "$hitlist" ] && printf '%s' "$hitlist"
    return 0
}

: >"$repo/tmp/sweep-hits.txt"

sweep_tree "micro-xrce-dds-client" "packages/rmw/xrce/xrce-sys/micro-xrce-dds-client" \
    "packages/rmw/xrce/xrce-sys/micro-xrce-dds-client/include" \
    "packages/rmw/xrce/xrce-sys/micro-xrce-dds-client/src/c" \
    "packages/rmw/xrce/xrce-sys/micro-cdr/include"

sweep_tree "mbedtls" "packages/rmw/zenoh/zpico-sys/mbedtls" \
    "packages/rmw/zenoh/zpico-sys/mbedtls/include" \
    "packages/rmw/zenoh/zpico-sys/mbedtls/library"

sweep_tree "cyclonedds" "third-party/dds/cyclonedds" \
    "third-party/dds/cyclonedds/src/core/ddsc/include" \
    "third-party/dds/cyclonedds/src/ddsrt/include" \
    "third-party/dds/cyclonedds/src/core/ddsi/include"

sweep_tree "threadx" "third-party/threadx" \
    "third-party/threadx/common/inc" \
    "third-party/threadx/ports/linux/gnu/inc"

echo
echo "diagnostics (if any) -> tmp/sweep-hits.txt"
