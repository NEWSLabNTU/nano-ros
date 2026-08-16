# Phase 366 — The panic platform API, and one fatal path per image

**Status (2026-08-16).** IN PROGRESS. Implements RFC-0077's first half: the
platform ABI gains a fatal entry point, every port implements it, the four
hardcoded panic handlers migrate onto it, and the old paths retire. W1 is the
gate on everything else — the C header is the ABI SSoT (RFC-0054), so the
generated Rust mirror and the export macro must move with it in one commit or
`check-abi-bindings` goes red.

Implements: RFC-0077. Tracked issues: 0618 (the design defect), 0617 (the two
failure modes it produces).

## Why

`<nros/platform.h>` exposes clock, **allocation**, atomics, sleep, yield,
random, wall clock, tasks, mutexes, condvars, wake, critical section and
logging — and nothing for "the world ended". So the allocator is a platform fact
that C and Rust share through one funnel, and panic is not expressible at all,
which is why each language invented its own ending and four libraries each
hardcoded a different one:

| provider | behaviour |
| --- | --- |
| `nros-c` | `loop { spin_loop() }` — silent |
| `nros-board-nuttx` | `println!("nros: PANIC {info}")`, `exit(1)` |
| `nros-board-threadx-qemu-riscv64` | UART `"PANIC: "`, exit QEMU |
| `nros-board-mps2-an385-freertos` | semihosting, `bkpt #0`, spin |

An image is one artifact. One fatal path is the only coherent answer.

## Work items

### W1 — the ABI entry point (gates everything else)

Add to `packages/platform/nros-platform-api/include/nros/platform.h`, in the
shape the allocation section already uses:

```c
/* ---- Fatal error ---- */
_Noreturn void nros_platform_panic(const char *msg, size_t len);
```

`msg` is a diagnostic, NOT a C string: `len` bytes, no NUL required, possibly
empty. A port must tolerate being called from any context — interrupt,
scheduler-locked, or before the kernel starts.

Three things move together, because the header is the SSoT (RFC-0054):

- `scripts/gen-abi-bindings.sh` regenerated, `generated.rs` committed —
  `check-abi-bindings` fails on a diff.
- `PlatformPanic` trait in `nros-platform-api`, mirroring `PlatformAlloc`.
- `nros_platform_export_panic!` in `nros-platform-cffi`, so the three Rust ports
  can export it. RFC-0076 D4 records that the macro's symbol list is
  hand-maintained and drifted once already (W10 added task probes to the header
  and all five C ports but not the macro); this phase does not fix that, but it
  must not add a sixth instance of it.

**Acceptance:** `just check-abi-bindings` green; every port still links.

### W2 — the five C ports

`nros-platform-{posix,zephyr,threadx,freertos,esp-idf}/src/platform.c`, each
mapping to the native fatal path it already has:

| port | mapping |
| --- | --- |
| posix | `fwrite` to stderr, `abort()` |
| zephyr | message via the log ABI, then `k_panic()` |
| threadx | message, then `tx_thread_terminate` of self / spin |
| freertos | the existing `freertos_hooks.c` body — message, `bkpt`, halt |
| esp-idf | `esp_system_abort()`, which honours `CONFIG_ESP_SYSTEM_PANIC_*` |

Weak where the toolchain supports it, so a C/C++ image can override with a
strong definition. Note posix/threadx-linux are hosted: they get `std`, so the
Rust side never reaches the ABI there — the C surface still needs it.

**Acceptance:** each port compiles; `just build-test-fixtures lane=tier2` builds
all eight families.

### W3 — the Rust ports

`nros-platform-mps2-an385`, `nros-platform-esp32-qemu`, and `nros-board-cffi`
via `nros_platform_export_panic!`.

### W4 — Rust panics reach the ABI

The default `#[panic_handler]` formats `PanicInfo` into a stack buffer and calls
`nros_platform_panic`. It moves OUT of `nros-c` (a library) — RFC-0077's rule is
that the entry owns it — but that move is W5's, so W4 lands the forwarding body
where the handler currently lives and proves the funnel works end to end.

**Acceptance:** a panic in a fixture prints through the port's path, not
`nros-c`'s silent spin.

### W5 — migrate the four hardcoded handlers, then retire the old paths

In order, because each step must leave every image with exactly one provider:

1. Board handlers (`nuttx`, `threadx-qemu-riscv64`, `mps2-an385-freertos`)
   forward to `nros_platform_panic` instead of implementing a behaviour.
2. `nros-board-threadx-qemu-riscv64`'s handler is **ungated** today — it is the
   one that cannot currently be turned off, so it moves first.
3. The entry (`nros::main!` / `nano_ros_entry()`) supplies the handler; the
   `panic-spin` feature on `nros-c`/`nros-cpp` retires.
4. `ARCHITECTURE.md` §2 amended: panic's selector is the image, not
   `platform-<rtos>`. The allocator's sentence stays — its implementation IS
   platform-keyed, and the arena must remain shared.

### W6 — the gate

Extend `scripts/check-archive-lang-items.sh` to the panic lang item
(`rust_begin_unwind`), and count per image COORDINATE rather than per link line:
per-link-line catches duplicates but cannot catch ABSENCE, which is what 0617's
`#[panic_handler] function required` was.

**Acceptance:** the gate fails a deliberately-broken image both ways — two
providers, and none.

## Deliberately out of scope

- **The allocator.** Already per-image via 0616 and enforced by
  `check-archive-lang-items`; its arena must stay `nros_platform_alloc` and this
  phase must not offer a knob for it.
- **RFC-0076 D4's macro drift.** Real, adjacent, and its own work.
- **Per-RTOS fatal configuration** (`CONFIG_ESP_SYSTEM_PANIC_*`, Zephyr assert
  levels, NuttX crashdump). The ABI makes them reachable; choosing them is the
  image's and belongs with the `PANIC` entry option, not here.

## Risks

- **The ABI mirror.** A header edit without regenerating bindings is an
  immediate red (`check-abi-bindings`). Same commit, always.
- **Lang-item windows.** W5 can leave an image with zero or two providers
  between steps. W6's gate should land before W5's flips so the failure is a
  gate message, not a four-crates-away link error.
- **Host lanes cannot see any of this.** `std` supplies the handler, so every
  regression here surfaces only on embedded coordinates — tier 2 minimum.
