# Phase 366 — The panic platform API, and one fatal path per image

**Status (2026-08-16).** IN PROGRESS — W1, W2, W3, W4 and W6 landed; W5 is
HALF done and is where the remaining risk is.

Landed: the ABI entry point with its regenerated mirror and `PlatformPanic`
trait (W1); five C ports and two bare-metal Rust ports mapped onto their native
fatal paths (W2, W3); `nros-c`'s `#[panic_handler]` forwarding to the ABI
instead of spinning silently (W4); and the lang-item gate extended to
`rust_begin_unwind`, mutation-tested both directions — two providers exit 1, one
exits 0 (W6). Verified across `lane=tier2`: zephyr, threadx-linux, nuttx, esp32,
qemu and freertos all build.

W5 half: the threadx-riscv64 and nuttx boards now EXPORT their behaviour as a
strong `nros_platform_panic` and their lang items only forward. The lang items
themselves are still in the boards, because removing them leaves a pure-Rust
image on those boards with no handler at all — the entry does not supply one
yet. That is the remaining work, and W6's gate now exists to catch it.

Two families are red for reasons outside this phase: threadx-riscv64 on #0629
(the lld wrapper passes no `-L`, so the CycloneDDS C++ link cannot find
`-lstdc++` — unmasked by this phase's header edit, not caused by it), and one
native run where `ar` died with "Bus error (core dumped)", which retested clean
SOLO and is the under-load flake.

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

### W5 — migrate the providers, then move ownership to the entry

The order is FORCED, and the reason is issue 0617 from the opposite direction:
delete a provider before something else supplies one and every pure-Rust image
on that board fails with `#[panic_handler] function required`. Each step must
leave every image with exactly one.

Providers as they stand (verified in tree, not recalled):

| provider | shape | state |
| --- | --- | --- |
| `nros-c/src/lib.rs:160` | `#[panic_handler]` | **forwards** to the ABI (W4) |
| `nros-board-nuttx` | `#[panic_handler]` | **forwards**; behaviour is its strong `nros_platform_panic` |
| `nros-board-threadx-qemu-riscv64` | `#[panic_handler]` | **forwards**; ditto |
| `nros-board-mps2-an385-freertos` | `use panic_semihosting as _` | **not migrated** |
| `nros-c` / `nros-cpp` `panic-halt` | `use panic_halt as _` | feature-gated, library-owned |

**W5.a (done)** — threadx-qemu-riscv64 and nuttx export their behaviour as a
strong `nros_platform_panic`; their lang items only forward. The ungated one
moved first because it was the provider no feature could turn off.

**W5.b — the third board. BLOCKED on W5.c, and the attempt proved why.**

`nros-board-mps2-an385-freertos` imports the lang item from `panic-semihosting`
rather than writing one. Replacing that with a forwarding handler — the same
migration the other two boards took — fails:

```
error[E0152]: duplicate lang item in crate `panic_semihosting`
  (which `logging_smoke_freertos_mps2` depends on): `panic_impl`
```

The old arrangement worked only because BOTH sides named the same crate, so
cargo unified them into one lang item. The moment the board writes its own, the
image's `panic-semihosting` is a second definition.

The images split, and that is the whole constraint:

- `packages/testing/nros-tests/bins/logging-smoke-freertos-mps2` declares its own
  provider with `features = ["exit"]`, so a panic EXITS QEMU instead of hanging
  the harness. That is a deliberate image-specific policy and exactly what
  RFC-0077 says should happen — this image already owns its ending, for a
  reason the board could not know.
- the six `examples/qemu-arm-freertos/rust/*` declare NOTHING and depend on the
  board for theirs.

So the board cannot stop providing until those six have their own, and it cannot
start writing one while the smoke bin has its own. Both directions are blocked
by the same missing piece: **W5.c**. Attempted, reverted, recorded — the
alternative was a suppression feature that W5.c would delete again.

Note `examples/qemu-arm-baremetal/rust/*` already write `use panic_semihosting
as _;` in their own `main.rs`, which is a second existence proof of the target
shape alongside ESP32's `esp-backtrace`.

**W5.c — the entry emits a provider.** `nros::main!` and `nano_ros_entry()`
today emit NONE — verified, zero matches in `main_macro.rs`. This is the
substantive step, and W5.b's failed attempt shows it is a HARD prerequisite
rather than merely the next item. Default forwards to
`nros_platform_panic`; an entry that declares its own (or `use panic_halt as _`)
must win, which means the emitted default has to be suppressible rather than
unconditional.

**W5.d — delete the forwarding handlers**, in `nros-c` and the three boards,
once W5.c supplies one. Only now is it safe.

**W5.e — retire the features.** `panic-spin` disappears from `nros-c`/`nros-cpp`;
`panic-halt` stops being a library feature and becomes what it always should
have been — a dependency the IMAGE names.

**W5.f — amend `ARCHITECTURE.md` §2.** Panic's selector is the image, not
`platform-<rtos>`. The allocator's sentence stays: its implementation IS
platform-keyed and its arena must remain shared. Last, so the text lands only
once the code makes it true.

**Prerequisite for W5.d, not a nicety.** W6's gate counts per LINK LINE, which
catches duplication and cannot catch ABSENCE — and absence is precisely what
W5.d risks. Extending it to count per image COORDINATE should land first.

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
