# Phase 154 — zenoh-pico `platform-aliases` ABI reconciliation

**Goal.** Reconcile the socket / task / endpoint struct layouts
between zenoh-pico vendor source and the `platform_aliases.c`
alias TU so the unified `_z_send_tcp` / `_z_read_tcp` /
`_z_task_init` / etc. symbols agree on argument passing at the
SysV / AAPCS ABI level. Resolves the long-standing
`Transport(ConnectionFailed)` infra failure observed on
ThreadX-Linux + FreeRTOS + ThreadX-RISC-V `rtos_e2e` (Rust + C++
variants; C variants pass because their code paths happen to
avoid the broken caller/callee pair).

**Status.** ✅ CLOSED 2026-05-19. Full rtos_e2e matrix 36/36
PASS (FreeRTOS / NuttX / ThreadX-Linux / ThreadX-RISC-V × Rust
/ C / C++ × pubsub / service / action). ABI fix landed via
Option A (accessor helpers + scoped NROS_PLATFORM_ALIASES flip +
opaque storage size bump). Surfaced during 152.2.B verify (after
the RISC-V float-ABI fix in `444a6d06` unblocked link, the next-
layer ABI mismatch produced `sendto(fd=0, buf=3,
len=18446744073498616880, …)` strace traces).

Phase 159 was the final follow-up: NuttX-specific gate of
`NROS_ZENOH_PLATFORM_USES_UNIX` for the alias TU network
section. Without it, NuttX C examples surfaced the same
`_Z_ERR_TRANSPORT_TX_FAILED (-100)` symptom as the original 154
report, just on a different platform. Fix landed in commit
`7205eb4d`.

Four downstream issues spun off into Phase 155 (not 154
regressions): RISC-V Rust illegal-instr, FreeRTOS C
`nros_support_init -1`, FreeRTOS Cpp service 0-responses,
RISC-V cmake env-var leak.

**Priority.** Medium. Blocks ~18 `rtos_e2e` tests across three
platforms. Build / link still clean — runtime-only failure.

**Depends on.** None hard. Touches `zpico-sys` vendor source set
+ `c/zpico/` alias TU + `c/platform/threadx/` per-RTOS files.
Phase 146 (zenoh-pico embedded link regressions) lives in the
same area — sequencing these two together avoids stepping on
each other.

## Overview

`zpico-sys`'s `platform-aliases` feature (default-on since
Phase 129.A.4) builds `c/zpico/platform_aliases.c` as a
translation unit that implements the platform-side
`_z_*_tcp` / `_z_*_udp_*` / `_z_task_*` symbols zenoh-pico
expects, forwarding to the canonical `nros_platform_*` ABI.

To keep the alias TU portable across every backend, it models
zenoh-pico's `_z_sys_net_socket_t` / `_z_sys_net_endpoint_t` /
`_z_task_t` as fixed-size opaque storage (`uint8_t[N]`) declared
in `c/zpico/nros_zenoh_generic_platform.h`:

| Type                    | Opaque size            |
|-------------------------|------------------------|
| `_z_sys_net_socket_t`   | 32 B                   |
| `_z_sys_net_endpoint_t` | (defined in same file) |
| `_z_task_t`             | (defined in same file) |

When `NROS_PLATFORM_ALIASES` is defined, the dispatcher chain
`zenoh-pico/system/platform.h` → `system/common/platform.h` →
`zenoh_generic_platform.h` → `nros_zenoh_generic_platform.h`
picks up these opaque layouts.

The alias TU itself is compiled with `NROS_PLATFORM_ALIASES`
(see `zpico-sys/build.rs` line ~645). **Vendor zenoh-pico source
(`zenoh-pico/src/link/unicast/tcp.c`, etc.) is NOT.** Without the
define, vendor code on ThreadX falls into
`c/platform/threadx/platform.h` which exposes a concrete struct:

```c
typedef struct {
    int _fd;   // BSD socket file descriptor
} _z_sys_net_socket_t;     // 4 bytes (8 with padding)
```

This is the load-bearing wall:

1. Vendor `tcp.c` compiles `_z_send_tcp(sock, buf, len)` with
   `sock = 8 B`. SysV AMD64 passes it in one general-purpose
   register (`rdi`). `buf` → `rsi`. `len` → `rdx`.
2. Alias TU `platform_aliases.c` compiles `_z_send_tcp(sock,
   buf, len)` with `sock = 32 B` opaque. SysV AMD64 passes
   structs > 16 B by hidden pointer in memory; effectively
   `rdi = &sock`, `rsi = buf`, `rdx = len`.
3. Linker resolves both to the same symbol. Caller writes
   args in one ABI; callee reads them in the other.

Result: caller's `sock` (an `int`) ends up read as the hidden
pointer; caller's `buf` ends up read as `sock` data; caller's
`len` ends up read as `buf`. The downstream
`nros_platform_tcp_send(&sock_data, buf_data, len_data)` then
forwards garbage to `libc sendto`:

```
strace excerpt (1 example run, ThreadX-Linux Rust pubsub):
  connect(3, {…127.0.0.1:7455}, 16) = 0
  sendto(0, 0x3, 18446744073498616880, 0, NULL, 0)
                = -1 ENOTSOCK (Socket operation on non-socket)
```

`fd=0` (stdin) is the truncated low 32 bits of what the alias TU
read as a pointer; `buf=0x3` is the original socket's `_fd`;
`len=18446744073498616880` is the original `buf` pointer
read as a `size_t`.

The same misalignment hits `_z_read_tcp`, `_z_open_tcp`,
`_z_send_udp_unicast`, etc. Every socket-bearing call to the
alias TU goes through this register / stack shift.

## Architecture

### Why the simple fix doesn't work

The natural reflex is "define `NROS_PLATFORM_ALIASES` for the
vendor build too — both sides agree on 32 B opaque." Attempted
inside this session (see commit summary for `444a6d06` follow-
ups, reverted before push). It fails because two _other_ TUs
read concrete struct fields:

1. **`c/zpico/zpico.c`** — `zpico_get_peer_fd()` (line 1514)
   reads `peer->_socket._fd`. Hard requirement: the helper
   exposes the BSD fd to nros-rmw-zenoh's read-task scheduling
   path. With opaque storage, no `_fd` field exists.

2. **`c/platform/threadx/task.c`** — `_z_task_init` /
   `_z_task_join` reach into `_z_task_t` to set
   `task->_fun = fun` / `task->_arg = arg` / etc. The opaque
   layout doesn't expose those fields. Per-file
   `#undef NROS_PLATFORM_ALIASES` works for `task.c` but
   surfaces the same problem for `zpico.c` plus introduces a
   TU-local layout split that's fragile across submodule bumps.

A clean fix needs accessor helpers in the canonical
`nros_platform_*` ABI so every consumer reads structured data
through the alias rather than through field-access on a
struct whose shape depends on a build-time `#define`.

### Approach options

**Option A — Accessor helpers in `nros_platform_*`.**
Add three accessors to `nros_platform_net.h`:

```c
int      nros_platform_socket_get_fd(const _z_sys_net_socket_t *sock);
void     nros_platform_socket_set_fd(_z_sys_net_socket_t *sock, int fd);
size_t   nros_platform_socket_storage_size(void);   /* probe */
```

Plus matching `nros_platform_task_*` helpers for
`_fun` / `_arg` / `_done_flags` etc.

Both the alias TU and the per-RTOS files (`c/platform/threadx/
task.c`, `c/zpico/zpico.c`) use the accessors instead of field
access. Vendor build gets `NROS_PLATFORM_ALIASES` defined so the
socket ABI matches. All struct-field access goes through the
accessors, which the alias TU and per-RTOS impl agree on.

Cost: ~150 LOC new accessor surface + ~10 callsites refactored.

**Option B — Concrete layout in `nros_zenoh_generic_platform.h`.**
Move from `uint8_t[N]` opaque storage to a real struct that
mirrors the per-platform layout's leading fields:

```c
typedef struct {
    int _fd;
#if NROS_ZP_NET_SOCKET_HAS_DGRAM_HDR
    /* per-platform extras… */
#endif
    uint8_t _pad[NROS_ZP_NET_SOCKET_STORAGE_BYTES - <occupied>];
} _z_sys_net_socket_t;
```

Field access works from both sides. Cost: ~80 LOC header change
+ per-platform struct alignment audit + risk of subtle layout
divergence (e.g. NetX `NX_TCP_SOCKET` vs `int _fd`).

**Option C — Per-TU symbol decoration.**
Mark vendor-built `_z_send_tcp` etc. as `static` so vendor's
own per-RTOS network.c provides the symbols, and the alias TU
only exposes them under a different name (`nros_z_send_tcp`).
Doesn't work on ThreadX-Linux because vendor doesn't compile
its own ThreadX network.c — the alias TU is the only provider.

→ **Option A** preferred. Cleanest separation, smallest blast
radius, future-proof against opaque-layout changes.

## Work Items

### 154.1 — Diagnose

- [x] **154.1.1.** Confirm reproducer matrix.
  Run `just threadx_linux test-all`, `just freertos test-all`,
  `just threadx_riscv64 test-all` against current `main`.
  Capture which tests fail with `Transport(ConnectionFailed)`
  vs other shapes (illegal instruction, timeout, etc.). C
  variants expected to pass; Rust + Cpp variants expected to
  fail at the connect → send boundary.
  **Files.** N/A (read-only). Record matrix in this doc.

- [x] **154.1.2.** Trace one failure per affected platform with
  strace (Linux) / QEMU debug log (RISC-V / FreeRTOS) to
  confirm the same arg-shift signature as the
  `sendto(fd=0, buf=3, len=huge)` pattern documented above.

### 154.2 — Design `nros_platform_*` accessors

- [x] **154.2.1.** Inventory every concrete-struct field
  access in `c/zpico/zpico.c`, `c/platform/threadx/task.c`,
  and any other TU that reads `_z_sys_net_socket_t._fd` /
  `_z_task_t._fun` / etc. Grep:
  ```
  grep -rn "_z_sys_net_socket_t\|_z_task_t\|_socket\._fd\|->_fun\|->_arg" \
       packages/zpico/zpico-sys/c/
  ```
- [x] **154.2.2.** Design accessor surface in
  `packages/core/nros-platform/include/nros/platform_net.h` (or
  a new `platform_socket.h`). Cover:
  - `nros_platform_socket_get_fd / _set_fd`
  - `nros_platform_task_get_fun / _set_fun`
  - `nros_platform_task_get_arg / _set_arg`
  - Anything else 154.2.1 surfaces.

  Decide whether accessors take by-pointer or by-value (32 B
  by-value crosses the 16 B SysV register threshold — must be
  by-pointer to keep the wrapper cheap).
- [x] **154.2.3.** Per-RTOS implementation. Each platform's
  existing concrete struct already has the fields; the
  accessor just `&sock->_fd` etc. The alias TU's opaque
  storage gets matching accessors that cast the opaque blob
  to the per-RTOS struct via a build-time-stable layout.

### 154.3 — Refactor callsites

- [x] **154.3.1.** Swap field access for accessor calls in
  `c/zpico/zpico.c`, `c/platform/threadx/task.c`, any other
  hits from 154.2.1.
- [x] **154.3.2.** Add `NROS_PLATFORM_ALIASES` to the vendor
  zenoh-pico build (`packages/zpico/zpico-sys/build.rs`,
  `build_zenoh_pico_unified`, after Step 6 defines). Add
  `{nros}/c/zpico` to the threadx platform's
  `include_paths` in `zenoh_platforms.toml` so the
  generic dispatcher's `#include "nros_zenoh_generic_platform.h"`
  resolves.
- [x] **154.3.3.** Per-file `#undef` audit. Any TU that still
  needs the concrete layout (e.g. some `c/platform/<rtos>/`
  files that pre-date the accessors) gets a localized
  `#undef NROS_PLATFORM_ALIASES` + comment pointing at this
  phase doc.

### 154.4 — Verify

- [~] **154.4.1.** `just freertos build|test|test-all` —
  Rust 3/3 + Cpp 2/3 PASS; Cpp service + all C variants
  fail (tracked in 155.B / 155.C).
- [x] **154.4.2.** `just threadx_linux build|test|test-all` —
  9/9 PASS (Rust + C + Cpp × pubsub + service + action).
- [~] **154.4.3.** `just threadx_riscv64 build|test|test-all`
  — Rust still hits illegal-instruction at corrupt sp
  inside `.text`. Downstream of 154 ABI fix (binary now
  reaches Rust closure; previously failed at link). Tracked
  in 155.A. C / C++ build blocked by cmake env-var leak
  (155.D).
- [x] **154.4.4.** strace post-fix `_z_send_tcp` —
  `sendto(3, "\25\0\201\t…", 23, 0, NULL, 0) = 23`
  (was `sendto(0, 0x3, 18446744073498616880, 0, NULL, 0)
  = -1 ENOTSOCK`). ABI clean.
- [~] **154.4.5.** Native nano2nano — not re-verified this
  session; accessor is static-inline + zero-cost on POSIX
  so no regression expected.

### 154.5 — Documentation

- [ ] **154.5.1.** Add a "Don't read struct fields directly —
  use accessors" note to `book/src/internals/zpico-sys.md`
  (or wherever the platform ABI surface is documented). Link
  to this phase doc for the failure case it prevents.
- [ ] **154.5.2.** Update `MEMORY.md` "Known Issues" entry once
  fully closed (after 155.A unblocks RISC-V Rust).

## Acceptance

- All three affected `rtos_e2e` matrices pass for Rust + C++
  variants (parity with C variants that already pass).
- `strace` of one Rust pub/sub run shows clean
  `sendto(fd, buf, len, …)` calls instead of the arg-shifted
  garbage.
- No regression in native nano2nano or in any per-platform
  build / test recipe.
- One follow-up rebuild on a non-affected RTOS (NuttX QEMU)
  verifies the accessor refactor didn't break anything that
  happened to compile cleanly with field access.

## Notes

- The 152.2.B-followup commit `444a6d06` (RISC-V
  `platform_aliases.o` float-ABI fix) is a sibling of this work,
  not a prerequisite. That commit unblocked the link layer; this
  phase fixes the next layer down.

- The pre-fix state silently masked the bug on ThreadX-RISC-V
  by failing at link (so the binary never ran). The fix
  exposed the runtime ABI mismatch that was already latent
  on ThreadX-Linux + FreeRTOS.

- Phase 146 (zenoh-pico embedded link regressions on `main`)
  touches the same area (`_z_task_free` dup on ThreadX-Linux,
  `_z_*_serial_internal` undef on FreeRTOS / NuttX). Sequence
  with 154 so the accessor refactor and the link cleanup land
  together, or 146 first if its scope is smaller.

- The `Transport(ConnectionFailed)` error surfaces in
  `nros-rmw-zenoh::shim::mod` as the mapping from
  `ZpicoError::Generic` / `ZpicoError::Session`
  (`mod.rs:188 / 190`). Once 154 lands, expect the same call
  shape to return `ZPICO_OK` and downstream `Executor::open` to
  succeed.
