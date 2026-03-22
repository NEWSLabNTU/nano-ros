# RISC-V 64 ThreadX C/C++ Porting Notes

Cross-compiling C/C++ examples for ThreadX on RISC-V 64-bit QEMU virt
requires several workarounds for toolchain incompatibilities between
Rust's `compiler_builtins`, picolibc, and the ThreadX kernel.

## Toolchain

- Cross-compiler: `riscv64-unknown-elf-gcc` (GCC 10.x)
- C library: picolibc (headers only for compilation; `libc.a` for linking)
- Linker: `rust-lld` (GNU ld cannot handle TLS/non-TLS `errno` mismatch)
- Rust target: `riscv64gc-unknown-none-elf` (double-float ABI, lp64d)

## Key Issues and Solutions

### 1. TLS errno mismatch (GNU ld → rust-lld)

picolibc's `libc.a` defines `errno` as a TLS (thread-local storage)
variable. ThreadX's `app_define.c` defines `errno` as a regular `.sbss`
global. GNU ld refuses to link objects with mixed TLS/non-TLS references
to the same symbol.

**Fix:** Use `rust-lld` as the linker. The Rust linker handles this
mismatch (the Rust build uses it natively). Since GCC 10.x doesn't
support `-fuse-ld=lld` for cross targets, the toolchain file overrides
`CMAKE_C_LINK_EXECUTABLE` with a wrapper script that calls `rust-lld`
directly.

**Files:** `cmake/toolchain/riscv64-threadx.cmake`,
`cmake/toolchain/riscv64-lld-wrapper.sh`

### 2. compiler_builtins soft-float ABI (strip script)

Rust's pre-compiled `compiler_builtins` for `riscv64gc-unknown-none-elf`
contains objects (e.g., `bswapsi2.o`) compiled with soft-float ABI,
despite the target being hard-float (`lp64d`). This is a known upstream
issue: [rust-lang/rust#83229](https://github.com/rust-lang/rust/issues/83229).

**Fix:** `cmake/strip-compiler-builtins.sh` runs at install time and
removes all objects with `soft-float` ELF flag from the Rust staticlib.
The `libgcc.a` from the cross-compiler provides these symbols instead.

### 3. Rust memset crashes on RISC-V (GCC -fno-builtin)

Rust's `compiler_builtins` embeds `memset`/`memcpy`/`memmove` as weak
symbols in the main crate object. These implementations can crash on
RISC-V in QEMU (observed as an illegal instruction in a recursive
`memset` that never terminates).

The startup code provides simple byte-loop replacements, but GCC's loop
idiom recognition transforms `while(n--) *p++ = c` back into
`call memset` — creating infinite recursion.

**Fix:** Add `-fno-builtin` to `CMAKE_C_FLAGS_INIT` in the toolchain
file. This prevents GCC from replacing explicit loops with built-in
function calls. The strip script also localizes Rust's mem symbols
(`llvm-objcopy --localize-symbol`) so they don't shadow the startup
versions.

**Files:** `cmake/toolchain/riscv64-threadx.cmake`,
`examples/qemu-riscv64-threadx/cmake/startup.c`,
`cmake/strip-compiler-builtins.sh`

### 4. picolibc TLS requires tp register initialization

picolibc uses the RISC-V `tp` (thread pointer) register for
thread-local storage. Our `entry.s` (from the ThreadX port) leaves
`tp = 0` after boot. Any picolibc function that accesses TLS variables
(e.g., `errno` from `strtol`, or the `rand()` seed) will dereference
`NULL + offset` and crash.

**Fix:** `startup.c` initializes `tp` to point to a static 512-byte
TLS block before calling `uart_init()` or any other picolibc function.

### 5. No C entry point in RISC-V app_define.c

The ThreadX RISC-V board crate's `app_define.c` originally only
supported Rust entry via `rust_app_entry` callback. C/C++ examples
define `app_main()` which must be called when no Rust callback is set.

**Fix:** Added `extern void app_main(void) __attribute__((weak))` and
a fallback `else if (app_main) app_main()` branch in `app_thread_entry`.

**File:** `packages/boards/nros-threadx-qemu-riscv64/c/app_define.c`

### 6. No UART output (picolibc stdout)

picolibc declares `stdout` as an undefined `FILE *const` global. On
bare-metal, no default stdout is provided. `printf()` silently discards
output unless the application defines a `stdout` FILE stream.

**Fix:** `startup.c` provides:
- `uart_init()` call before any output
- `stdout` definition as a picolibc `FDEV_SETUP_STREAM` pointing to
  `uart_putc` (QEMU virt PL011 UART)
- `_write()` syscall implementation for file descriptor output

### 7. C++ standard library headers (picolibc compat)

picolibc provides C headers (`stdio.h`, `stdint.h`) but not C++ wrapper
headers (`cstdio`, `cstdint`). The nros C++ API headers use C++ includes.

**Fix:** Minimal wrapper headers in `examples/qemu-riscv64-threadx/cmake/cxx-compat/`
that `#include` the corresponding C header. Added to the include path
via the support cmake module.

## Build Flow

```
riscv64-unknown-elf-gcc (compile)
    → picolibc headers (-isystem sysroot/include)
    → -fno-builtin (prevent memset loop→call optimization)
    → -DNROS_PLATFORM_BAREMETAL (skip POSIX platform headers)

rust-lld (link, via wrapper)
    → strip soft-float objects from .a archives
    → localize Rust memset/memcpy symbols
    → --allow-multiple-definition (TLS errno)
    → picolibc libc.a + libgcc.a
    → ThreadX linker script (link.lds)
```

## QEMU Launch

```bash
qemu-system-riscv64 -M virt -m 256M -bios none -nographic \
    -global virtio-mmio.force-legacy=false \
    -kernel <binary> \
    -netdev user,id=net0 \
    -device virtio-net-device,netdev=net0,bus=virtio-mmio-bus.0,mac=52:54:00:12:34:56
```

Uses slirp (user-mode) networking — no host TAP/veth setup needed.
Gateway: `10.0.2.2`, zenohd port: `7453`.
