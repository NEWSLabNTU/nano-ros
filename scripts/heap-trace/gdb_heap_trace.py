# Issue 1370 -- record every call through the platform allocation funnel
# (RFC-0034 D6: `nros_platform_alloc` / `_realloc` / `_dealloc`) of a RUNNING
# image, without rebuilding it.
#
# The funnel is the one ABI every port shares, so the same script traces a
# native process, a Zephyr native_sim `zephyr.exe` and a QEMU guest through its
# gdb stub. What reaches the funnel differs by port, and a reading must say
# which it is: on Zephyr and the bare-metal boards BOTH zenoh-pico's `z_malloc`
# and Rust's `alloc` arrive here; on a native (std) process and on FreeRTOS
# only the C side does (Rust `alloc` goes to libstd / is not funnelled).
#
# Output is one event per line, replayable by
#     cargo run -p zpico-alloc --features stats --example heap_replay -- <trace>
#
#     A <size> <ptr>          nros_platform_alloc(<size>) returned <ptr>
#     R <old> <size> <ptr>    nros_platform_realloc(<old>, <size>) returned <ptr>
#     F <ptr>                 nros_platform_dealloc(<ptr>)
#
# Usage: export NROS_HEAP_TRACE_OUT=<path/to/trace.txt> (else ./heap-trace.txt),
# then inside gdb, before `run`/`continue`:
#     source scripts/heap-trace/gdb_heap_trace.py
#
# An environment variable rather than a gdb convenience variable: setting a
# STRING convenience variable needs a live inferior to allocate it in, which a
# script sourced before `run` does not have.
#
# Every stop costs a round-trip through gdb, so a traced image runs slower than
# an untraced one. That changes TIMING, not the request sizes or the order of
# one thread's calls, which is what a replay consumes.

import os

import gdb

_out_path = os.environ.get("NROS_HEAP_TRACE_OUT", "heap-trace.txt")

_out = open(_out_path, "w", buffering=1)


# Argument registers by architecture, for the entry stop where an optimised
# frame reports a parameter as `<optimized out>`: at the first instruction of
# the function the ABI's argument registers still hold the arguments. The last
# column is the RETURN register, read at the finish stop.
_ARG_REGS = {
    "i386:x86-64": ("$rdi", "$rsi", "$rax"),
    "arm": ("$r0", "$r1", "$r0"),
    "riscv": ("$a0", "$a1", "$a0"),
}
_RET = 2


def _arg(name, index):
    try:
        v = gdb.parse_and_eval(name)
        if not v.is_optimized_out:
            return int(v)
    except gdb.error:
        pass
    return _reg(index)


def _reg(index):
    arch = gdb.selected_frame().architecture().name()
    for key, regs in _ARG_REGS.items():
        if arch.startswith(key):
            return int(gdb.parse_and_eval(regs[index])) & ((1 << 64) - 1)
    raise gdb.GdbError("heap-trace: no argument-register table for %s" % arch)


class _Ret(gdb.FinishBreakpoint):
    def __init__(self, frame, fmt):
        super().__init__(frame, internal=True)
        self.silent = True
        self._fmt = fmt

    def stop(self):
        rv = self.return_value
        if rv is not None:
            ptr = int(rv)
        else:
            # No debug type for the return (a Rust export in an image built
            # without its debuginfo): the ABI's first return register holds it
            # at the finish stop.
            ptr = _reg(_RET)
        _out.write(self._fmt.format(ptr=ptr))
        return False

    def out_of_scope(self):
        # The frame unwound without returning (longjmp, thread exit): the
        # event is lost, and saying so beats a trace that silently omits it.
        _out.write("# lost-return\n")


class _Entry(gdb.Breakpoint):
    def __init__(self, spec, kind):
        super().__init__(spec, internal=True)
        self.silent = True
        self._kind = kind

    def stop(self):
        frame = gdb.newest_frame()
        if self._kind == "A":
            _Ret(frame, "A %d {ptr:#x}\n" % _arg("size", 0))
        elif self._kind == "R":
            _Ret(frame, "R %#x %d {ptr:#x}\n" % (_arg("ptr", 0), _arg("size", 1)))
        else:
            _out.write("F %#x\n" % _arg("ptr", 0))
        return False


# WHICH entry points. The rlsf arena's own exports come first when the image
# has them (Zephyr: `nros_zephyr_heap_*`, behind the C funnel's spinlock):
# they are the allocator whose fragmentation is the question, and on Zephyr the
# C funnel `nros_platform_alloc` is a LOCAL symbol that LTO inlines into some
# callers, so a breakpoint on it sees the frees and misses most allocations
# (measured: 1,116 `F` lines and zero `A` lines on the native_sim C talker).
# Otherwise the platform funnel, which is a real out-of-line call on the ports
# that have no arena of their own (POSIX `malloc`, FreeRTOS `pvPortMalloc`).
#
# A port may not define `realloc` (FreeRTOS and native Rust images link none);
# a missing symbol is reported, never fatal, and the trace says which entries
# it watched so a reader knows what an absent `R` line means.
_FUNNELS = (
    ("nros_zephyr_heap_alloc", "nros_zephyr_heap_realloc", "nros_zephyr_heap_free"),
    ("nros_platform_alloc", "nros_platform_realloc", "nros_platform_dealloc"),
)


def _present(sym):
    try:
        gdb.parse_and_eval("&" + sym)
        return True
    except gdb.error:
        return False


_funnel = next((f for f in _FUNNELS if _present(f[0])), None)
if _funnel is None:
    raise gdb.GdbError("heap-trace: no allocation funnel symbol in this image")
_watched = []
for _sym, _kind in zip(_funnel, ("A", "R", "F")):
    if not _present(_sym):
        gdb.write("heap-trace: %s is not in this image -- not watched\n" % _sym)
        continue
    _Entry(_sym, _kind)
    _watched.append(_sym)
_out.write("# watched: %s\n" % " ".join(_watched))
gdb.write("heap-trace: recording the allocation funnel to %s\n" % _out_path)
