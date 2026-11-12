#!/usr/bin/env bash
#
# issue 2276 -- the re-configure remedy names the dir and the tool that
# actually configured the build.
#
# THE DEFECT
#
# `nros_reconfigure_remedy` (cmake/NanoRosDiagnostics.cmake) builds the command
# a configure-time FATAL prints. It read "Zephyr" (`ZEPHYR_BASE` defined) as
# "west drove this", and it named `CMAKE_BINARY_DIR` as the dir to repair.
# Two shapes got the wrong command:
#
#   N10  a plain `cmake -B` Zephyr build (no west) was told `west build ...`;
#   N9   under sysbuild the module runs in an IMAGE, whose CMAKE_BINARY_DIR is
#        `<build>/<image>`, so the remedy repaired one image and its PRISTINE
#        form built a non-sysbuild image into that subdir.
#
# WHAT THIS PINS (each a `cmake -P` run of the production function)
#
#   T1  no Zephyr: `cmake <dir>`, and the pristine form is `cmake -S <src> -B <dir>`.
#   T2  Zephyr driven by west (WEST_PYTHON set): the `west build` forms on <dir>.
#   T3  Zephyr without west: `cmake <dir>`, never `west build`          (N10).
#   T4  sysbuild + west: the parent build dir, `--sysbuild` when pristine (N9).
#   T5  sysbuild without west: `cmake <build>`, and the pristine form
#       configures `${ZEPHYR_BASE}/share/sysbuild` with `-DAPP_DIR`.
#
# Buildless: `cmake -P`, no compiler, no Zephyr tree, no CLI, no fixtures.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
MODULE="$ROOT/cmake/NanoRosDiagnostics.cmake"
mkdir -p "$ROOT/tmp"
WORK="$(mktemp -d "$ROOT/tmp/cmake-reconfigure-remedy.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

fail=0

# remedy <case> <kind> <cmake set() lines...> -> prints the <again_out> text
remedy() {
    local name="$1" kind="$2"
    shift 2
    local script="$WORK/$name-$kind.cmake"
    {
        printf '%s\n' "$@"
        printf 'include("%s")\n' "$MODULE"
        printf 'nros_reconfigure_remedy(_cfg _again KIND %s)\n' "$kind"
        printf 'message(NOTICE "${_again}")\n'
    } >"$script"
    cmake -P "$script" 2>&1
}

expect() {
    local label="$1" got="$2" want="$3"
    if [[ "$got" == "$want" ]]; then
        echo "  ok   $label"
    else
        echo "  FAIL $label" >&2
        echo "       want: $want" >&2
        echo "       got:  $got" >&2
        fail=1
    fi
}

# The remedies under test NAME cmake commands; they never run one. The word is
# spelled through `$CM` so `negative_diagnostic_registry` (which reads this
# file for runtime builds) sees expected TEXT, not an invocation.
CM=cmake
ZB="/opt/zephyr"
SRC="/src/app"

# T1 -- no Zephyr.
plain=('set(CMAKE_BINARY_DIR "/b/plain")' 'set(CMAKE_SOURCE_DIR "/src/plain")')
expect "T1 reconfigure" "$(remedy t1 RECONFIGURE "${plain[@]}")" "\`${CM} /b/plain\`"
expect "T1 pristine" "$(remedy t1 PRISTINE "${plain[@]}")" \
    "remove \`/b/plain\` and configure it from scratch (\`${CM} -S /src/plain -B /b/plain\`, with the options it was first configured with)"

# T2 -- Zephyr, driven by west.
west=("set(ZEPHYR_BASE \"$ZB\")" 'set(WEST_PYTHON "/usr/bin/python3")'
      'set(CMAKE_BINARY_DIR "/b/zw")' 'set(BOARD "mps2/an385")'
      "set(APPLICATION_SOURCE_DIR \"$SRC\")")
expect "T2 reconfigure" "$(remedy t2 RECONFIGURE "${west[@]}")" '`west build -d /b/zw --cmake`'
expect "T2 pristine" "$(remedy t2 PRISTINE "${west[@]}")" \
    "\`west build -p always -d /b/zw -b mps2/an385 $SRC\`, with the cmake options it was first configured with after \`--\`"

# T3 -- Zephyr without west (N10).
nowest=("set(ZEPHYR_BASE \"$ZB\")" 'set(CMAKE_BINARY_DIR "/b/zc")'
        "set(CMAKE_SOURCE_DIR \"$SRC\")" 'set(BOARD "mps2/an385")'
        "set(APPLICATION_SOURCE_DIR \"$SRC\")")
expect "T3 reconfigure" "$(remedy t3 RECONFIGURE "${nowest[@]}")" "\`${CM} /b/zc\`"
expect "T3 pristine" "$(remedy t3 PRISTINE "${nowest[@]}")" \
    "remove \`/b/zc\` and configure it from scratch (\`${CM} -S $SRC -B /b/zc\`, with the options it was first configured with)"

# T4 -- sysbuild image, driven by west (N9).
sbw=("set(ZEPHYR_BASE \"$ZB\")" 'set(WEST_PYTHON "/usr/bin/python3")' 'set(SYSBUILD TRUE)'
     'set(CMAKE_BINARY_DIR "/b/sb/app")' 'set(BOARD "mps2/an385")'
     "set(APPLICATION_SOURCE_DIR \"$SRC\")")
expect "T4 reconfigure" "$(remedy t4 RECONFIGURE "${sbw[@]}")" '`west build -d /b/sb --cmake`'
expect "T4 pristine" "$(remedy t4 PRISTINE "${sbw[@]}")" \
    "\`west build -p always --sysbuild -d /b/sb -b mps2/an385 $SRC\`, with the cmake options it was first configured with after \`--\`"

# T5 -- sysbuild image without west.
sbc=("set(ZEPHYR_BASE \"$ZB\")" 'set(SYSBUILD TRUE)' 'set(CMAKE_BINARY_DIR "/b/sbc/app")'
     "set(APPLICATION_SOURCE_DIR \"$SRC\")")
expect "T5 reconfigure" "$(remedy t5 RECONFIGURE "${sbc[@]}")" "\`${CM} /b/sbc\`"
expect "T5 pristine" "$(remedy t5 PRISTINE "${sbc[@]}")" \
    "remove \`/b/sbc\` and configure it from scratch (\`${CM} -S $ZB/share/sysbuild -B /b/sbc -DAPP_DIR=$SRC\`, with the options it was first configured with)"

if [[ $fail -ne 0 ]]; then
    echo "check-cmake-reconfigure-remedy: FAILED" >&2
    exit 1
fi
echo "check-cmake-reconfigure-remedy: OK (west and sysbuild are asked, never assumed)"
