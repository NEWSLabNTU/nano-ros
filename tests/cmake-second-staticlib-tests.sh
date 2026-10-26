#!/usr/bin/env bash
# Issue 2092 — a SECOND Rust staticlib in one image must define nothing but its
# own surface strongly (cmake/NanoRosSecondStaticlib.cmake).
#
# A C++ Zephyr image whole-archives its message-FFI staticlib beside the
# runtime's `libnros_cpp.a`. Both carry compiler_builtins' C-compiled
# intrinsics as STRONG globals, so the first one the image's C calls ahead of
# the runtime (`__popcountdi2`, from Zephyr's `bitarray.c`) was defined twice
# and every Zephyr C++ image failed to link.
#
# Three halves, all against the host compiler, no nano-ros build, ~1 s:
#
#   A  the MECHANISM, on a synthetic pair of archives shaped like the real
#      ones: the unweakened pair fails to link with `multiple definition` (the
#      negative control — without it a green B proves nothing), the helper's
#      command weakens every global but the kept surface, and the pair then
#      links and the surface is still strong.
#   B  idempotence: the command re-runs on every rebuild of the archive, and
#      cargo may hand back the archive it already weakened.
#   C  the CLASS: every cmake function that builds a cargo artifact AND
#      whole-archives something must route through the helper. Issue 1645 left
#      exactly one such function, and this is what keeps a second one from
#      arriving without it.
set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"
# shellcheck source=../scripts/lib/grep-q.sh
source "$PROJECT_ROOT/scripts/lib/grep-q.sh"

MODULE="$PROJECT_ROOT/cmake/NanoRosSecondStaticlib.cmake"
CC_BIN="${CC:-cc}"

FAILURES=0
CHECKS=0
fail() { log_error "$*"; FAILURES=$((FAILURES + 1)); CHECKS=$((CHECKS + 1)); }
pass() { CHECKS=$((CHECKS + 1)); log_success "$*"; }

for _t in cmake ar nm objcopy "$CC_BIN"; do
    if ! command -v "$_t" >/dev/null 2>&1; then
        fail "host tool missing: $_t -- this test cannot report a verdict without it"
        exit 1
    fi
done

init_test_tmpdir "nros-second-staticlib"
trap 'cleanup_test_tmpdir' EXIT
T="$TEST_TMPDIR"

# ---------------------------------------------------------------------------
# A — the mechanism
# ---------------------------------------------------------------------------
log_header "A -- a whole-archived second staticlib no longer redefines the runtime's intrinsics"

# The runtime: one member per intrinsic, as compiler_builtins ships them, plus
# an unrelated member that must stay unpulled.
printf '%s\n' 'int __popcountdi2(long long a) { return (int)(a & 1); }' \
    > "$T/rt_pop.c"
printf '%s\n' 'int nros_runtime_other(void) { return 0; }' > "$T/rt_other.c"
# The FFI glue: its own surface plus its own copy of the same intrinsic.
printf '%s\n' 'int __popcountdi2(long long a) { return (int)(a & 1); }' \
    > "$T/ffi_pop.c"
printf '%s\n' 'int nros_cpp_demo(void) { return 7; }' > "$T/ffi_api.c"
# The image's C: calls the intrinsic (as Zephyr's bitarray.c does) and the
# FFI surface (as the generated C++ headers do).
printf '%s\n' \
    'extern int __popcountdi2(long long);' \
    'extern int nros_cpp_demo(void);' \
    'int main(void) { return __popcountdi2(3) + nros_cpp_demo() - 9; }' \
    > "$T/main.c"

for _s in rt_pop rt_other ffi_pop ffi_api main; do
    if ! "$CC_BIN" -c -O0 "$T/$_s.c" -o "$T/$_s.o" 2>"$T/$_s.cc.err"; then
        fail "A: could not compile the $_s fixture: $(cat "$T/$_s.cc.err")"
        exit 1
    fi
done
ar rcs "$T/librt.a" "$T/rt_pop.o" "$T/rt_other.o"
ar rcs "$T/libffi.a" "$T/ffi_pop.o" "$T/ffi_api.o"

# The real link line's order: image objects, the runtime scanned lazily, then
# the FFI archive whole-archived.
link_pair() {
    "$CC_BIN" "$T/main.o" "$T/librt.a" \
        "-Wl,--whole-archive,$1,--no-whole-archive" -o "$T/image" 2>"$T/link.err"
}

if link_pair "$T/libffi.a"; then
    fail "A: the UNWEAKENED pair linked -- the negative control cannot fail, so \
nothing below proves anything"
else
    if nros_grep_q -q 'multiple definition of .__popcountdi2' "$T/link.err"; then
        pass "A: unweakened pair fails with multiple definition of __popcountdi2 (control)"
    else
        fail "A: unweakened pair failed for another reason: $(cat "$T/link.err")"
    fi
fi

cp "$T/libffi.a" "$T/libffi-w.a"
weaken() {
    cmake -DMOD="$MODULE" -DARCHIVE="$1" -DOBJCOPY="$(command -v objcopy)" \
        -P "$T/weaken.cmake" >"$T/weaken.log" 2>&1
}
printf '%s\n' \
    'include("${MOD}")' \
    'nros_second_staticlib_weaken_command(_cmd ARCHIVE "${ARCHIVE}" KEEP "nros_cpp_*" OBJCOPY "${OBJCOPY}")' \
    'execute_process(COMMAND ${_cmd} RESULT_VARIABLE _rc)' \
    'if(NOT _rc EQUAL 0)' \
    '  message(FATAL_ERROR "weaken command failed: ${_rc}")' \
    'endif()' \
    > "$T/weaken.cmake"
if ! weaken "$T/libffi-w.a"; then
    fail "A: the helper's command failed: $(cat "$T/weaken.log")"
    exit 1
fi

syms="$(nm -g --defined-only "$T/libffi-w.a" 2>/dev/null)"
if nros_grep_q -qE '^[0-9a-f]+ W __popcountdi2$' <<<"$syms"; then
    pass "A: the intrinsic is WEAK in the second staticlib"
else
    fail "A: __popcountdi2 is not weak after the helper ran:"$'\n'"$syms"
fi
if nros_grep_q -qE '^[0-9a-f]+ T nros_cpp_demo$' <<<"$syms"; then
    pass "A: the kept surface (nros_cpp_*) stays STRONG"
else
    fail "A: nros_cpp_demo is not a strong T after the helper ran:"$'\n'"$syms"
fi

if link_pair "$T/libffi-w.a"; then
    pass "A: the weakened pair links"
else
    fail "A: the weakened pair still fails to link: $(cat "$T/link.err")"
fi

# ---------------------------------------------------------------------------
# B — idempotence
# ---------------------------------------------------------------------------
log_header "B -- re-running the command on a weakened archive changes nothing"
cp "$T/libffi-w.a" "$T/libffi-w1.a"
if ! weaken "$T/libffi-w.a"; then
    fail "B: second run of the helper's command failed: $(cat "$T/weaken.log")"
elif [ "$(nm -g --defined-only "$T/libffi-w.a" 2>/dev/null)" = \
       "$(nm -g --defined-only "$T/libffi-w1.a" 2>/dev/null)" ] && link_pair "$T/libffi-w.a"; then
    pass "B: idempotent"
else
    fail "B: a second run changed the symbol table or broke the link"
fi

# ---------------------------------------------------------------------------
# C — the class
# ---------------------------------------------------------------------------
log_header "C -- every cmake function that builds a cargo artifact and whole-archives routes through the helper"
# A function is in the class when its body both resolves cargo
# (`nros_rust_tool(<var> cargo)`) and names `--whole-archive`. Such a function
# links a Rust staticlib it built itself, and only the runtime archive may be
# a Rust staticlib without the helper.
scan_out="$(python3 - "$PROJECT_ROOT" <<'PY_EOF'
import pathlib, re, subprocess, sys
root = pathlib.Path(sys.argv[1])
files = subprocess.run(
    ["git", "-C", str(root), "ls-files", "--", "*.cmake", "*CMakeLists.txt"],
    capture_output=True, text=True, check=True).stdout.split()
start = re.compile(r"^\s*(function|macro)\s*\(\s*([A-Za-z0-9_]+)", re.I)
end = re.compile(r"^\s*end(function|macro)\s*\(", re.I)
members = 0
for rel in files:
    if rel.startswith(("third-party/", "packages/cli/third-party/")) or "/third-party/" in rel:
        continue
    p = root / rel
    try:
        lines = p.read_text(errors="replace").splitlines()
    except OSError:
        continue
    name, body, line0 = None, [], 0
    for i, line in enumerate(lines, 1):
        code = line.split("#", 1)[0]
        m = start.match(code)
        if m and name is None:
            name, body, line0 = m.group(2), [], i
            continue
        if name is not None and end.match(code):
            text = "\n".join(body)
            if re.search(r"nros_rust_tool\s*\(\s*\S+\s+cargo\s*\)", text) and "--whole-archive" in text:
                members += 1
                # Computing the command is not running it: the helper's out
                # variable must be a COMMAND of a custom command in the same
                # function (comments are already stripped).
                m = re.search(r"nros_second_staticlib_weaken_command\s*\(\s*(\w+)", text)
                if not m:
                    print(f"MISSING {rel}:{line0} {name}")
                elif not re.search(r"COMMAND\s+\$\{" + re.escape(m.group(1)) + r"\}", text):
                    print(f"UNRUN {rel}:{line0} {name}")
                else:
                    print(f"OK {rel}:{line0} {name}")
            name = None
            continue
        if name is not None:
            body.append(code)
print(f"MEMBERS {members}")
PY_EOF
)" || { fail "C: the scan itself failed"; scan_out=""; }
log_info "C: ${scan_out//$'\n'/; }"
if nros_grep_q -qE '^(MISSING|UNRUN) ' <<<"$scan_out"; then
    fail "C: a function builds a cargo staticlib and whole-archives it WITHOUT \
running nros_second_staticlib_weaken_command's command (issue 2092):"$'\n'"$(grep -E '^(MISSING|UNRUN) ' <<<"$scan_out")"
else
    pass "C: no function whole-archives a cargo artifact without the helper"
fi
# A floor, so a scan that silently stopped matching cannot read green: the
# Zephyr message-FFI image is a member today.
if nros_grep_q -q '^OK zephyr/cmake/nros_generate_interfaces.cmake:[0-9]* _nros_zephyr_cpp_ffi_image$' <<<"$scan_out"; then
    pass "C: the Zephyr message-FFI image function is in the class and uses the helper"
else
    fail "C: the scan did not find _nros_zephyr_cpp_ffi_image as a member using the helper"
fi

log_header "Summary"
if [ "$FAILURES" -eq 0 ]; then
    log_success "$CHECKS assertions held"
    exit 0
fi
log_error "$FAILURES of $CHECKS assertions failed"
exit 1
