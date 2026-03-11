# Platform Porting Pitfalls

Hard-won lessons from porting nros to bare-metal ARM (QEMU MPS2-AN385), ESP32-C3,
STM32F4, FreeRTOS, NuttX, ThreadX, and Zephyr. Read this before adding a new
platform or debugging mysterious failures on an existing one.

## Memory Layout

### DMA buffer placement

On MCUs with multiple SRAM regions, DMA descriptors and buffers **must** be in
DMA-accessible memory. The STM32F4 has 64 KB of CCM RAM that is NOT
DMA-accessible — placing Ethernet descriptors there causes silent data
corruption with no error interrupt.

**Fix:** Use linker script sections to force placement:

```ld
/* stm32f4.x — place DMA descriptors in SRAM1, not CCM */
.eth_descriptors (NOLOAD) : ALIGN(4) {
    *(.eth_descriptors .eth_descriptors.*)
} > RAM
```

In Rust, annotate the buffer:

```rust
#[link_section = ".eth_descriptors"]
static mut ETH_DMA_DESC: MaybeUninit<[u32; 256]> = MaybeUninit::uninit();
```

### Stack sizing

Bare-metal and RTIC applications share a single stack. FreeRTOS/Zephyr give each
task its own stack. In both cases, zenoh-pico's internal processing needs
significant stack space:

| Context | Minimum stack |
|---------|---------------|
| Bare-metal / RTIC main stack | 8 KB |
| FreeRTOS zenoh read task | 2 KB |
| FreeRTOS zenoh lease task | 1 KB |
| Zephyr zpico work queue thread | 2 KB |

Stack overflow on embedded targets causes **silent corruption** — there is no
guard page. FreeRTOS can detect overflow with `configCHECK_FOR_STACK_OVERFLOW=2`
and the high-water-mark API (`uxTaskGetStackHighWaterMark`), but only after the
fact.

### Heap sizing

zenoh-pico allocates heap memory during session open, publisher/subscriber
creation, and message processing. Typical heap consumption:

| Platform | Minimum heap |
|----------|-------------|
| MPS2-AN385 (bare-metal) | 64 KB |
| ESP32-C3 | 64 KB (~16 KB static + dynamic) |
| STM32F4 | 64 KB |
| FreeRTOS (lwIP + zenoh) | 256 KB (`configTOTAL_HEAP_SIZE`) |

Undersized heaps cause `_Z_ERR_SYSTEM_OUT_OF_MEMORY` (-78) during
`z_open()` or entity creation, not during message I/O.

### Rust struct move invalidation

When Rust returns a struct from a function, it **moves** the value to a new
address. If C code stored a pointer to the original location (e.g., during
`init()`), that pointer is now dangling.

**Symptom:** Callback crashes or garbage field values after struct is returned
from an init function.

**Fix:** Use static storage, `Box::pin()`, or index-based references instead of
raw pointers. See [Troubleshooting: Subscriber Callback Crashes](../guides/troubleshooting.md#subscriber-callback-crashes).

## Networking

### Ephemeral port conflicts

smoltcp's ephemeral port allocator starts at port 49152 with a static counter
that resets to zero on each boot. When QEMU instances are killed and restarted,
the new instance picks the **same** source port, creating a 4-tuple collision
with stale host-side TCP sockets.

**Symptom:** `ConnectionFailed` or `TransportOpenFailed` on the second test run.
`ss -tnap` shows `FIN-WAIT-1` sockets from the previous QEMU instance.

**Fix (test infrastructure):** Kill stale host TCP sockets between test runs:

```rust
// Clean stale TCP connections to QEMU IPs
for ip in &["192.0.3.10", "192.0.3.11"] {
    let _ = Command::new("ss")
        .args(["-K", "dst", ip])
        .status();
}
```

**Fix (Zephyr native_sim):** Pass different `--seed` values to each instance so
the entropy source produces different ephemeral ports:

```bash
./build-listener/zephyr/zephyr.exe --seed=12345
./build-talker/zephyr/zephyr.exe --seed=67890
```

### QEMU single-threaded I/O starvation

QEMU emulates the CPU and processes TAP network I/O in **one thread**. If the
guest never executes WFI (Wait For Interrupt), QEMU never yields to its I/O
event loop, and the TAP file descriptor is never serviced.

**Symptom:** First few packets work (buffered), then all networking stops.
Services and actions time out. Pub/sub may appear to work because subscriber
callbacks fire inline during `zp_read()`.

**Fix:** In RTIC, yield between `spin_once()` calls:

```rust
// CORRECT — yields to idle task → WFI → QEMU processes TAP
cx.local.executor.spin_once(0);
Mono::delay(10.millis()).await;

// WRONG — busy-loops, starving QEMU I/O
loop {
    cx.local.executor.spin_once(0);
}
```

In bare-metal (no RTIC), call `cortex_m::asm::wfi()` in your idle loop.

### TAP device assignment

Each QEMU peer **must** use a different TAP device. Two QEMU instances on the
same TAP cause packet collision and loss.

```
QEMU talker  → tap-qemu0 ─┐
                           ├── qemu-br (192.0.3.1/24)
QEMU listener → tap-qemu1 ─┘
```

### Subscriber startup ordering

Zenoh does not buffer messages for unknown subscribers. If the publisher starts
before the subscriber's declaration propagates through the router, early messages
are lost.

**Rule:** Start subscriber first, wait 5 seconds for stabilization, then start
publisher.

### TAP queue discipline

The default `fq_codel` qdisc drops packets when QEMU emulation is slow (CoDel
interprets emulation pauses as congestion). This causes spurious test failures.

**Fix:** Use `pfifo` instead:

```bash
sudo tc qdisc replace dev tap-qemu0 root pfifo limit 1000
sudo tc qdisc replace dev tap-qemu1 root pfifo limit 1000
```

## zenoh-pico

### Z_FEATURE_INTEREST causes multi-client failures

When multiple zenoh-pico clients connect to the same router, the interest
protocol's write filter creation can fail with `-78`
(`_Z_ERR_SYSTEM_OUT_OF_MEMORY`) even when memory is available.

**Fix:** Disable in all builds:

```rust
// build.rs
cmake::Config::new(&zenoh_pico_path)
    .define("Z_FEATURE_INTEREST", "0")
    .define("Z_FEATURE_MATCHING", "0")
    .build();
```

`Z_FEATURE_MATCHING` depends on `Z_FEATURE_INTEREST` — both must be disabled.

See [Troubleshooting: zenoh-pico Multiple Client Issues](../guides/troubleshooting.md#zenoh-pico-multiple-client-issues).

### Blocking TCP reads in cooperative schedulers

`_z_read_tcp()` can block for up to `SOCKET_TIMEOUT_MS` (default 10 seconds).
In cooperative schedulers (RTIC, bare-metal main loop), this blocks the entire
system since C FFI cannot yield.

**Impact:** Pub/sub works (callbacks fire inline during `zp_read()`), but
services and actions fail because `z_get()` query replies need multiple
`zp_read()` cycles, and the 5-second query timeout expires while `_z_read_tcp`
blocks.

**Current status:** `zpico_spin_once(0)` is non-blocking (`single_read=true`),
but the underlying `z_get()` timeout is still 5 seconds. RTIC service/action E2E
tests are `#[ignore]`d pending a fully non-blocking TCP read path.

### Version pinning

All zenoh components must be the **same version**. A version mismatch between
zenoh-pico and zenohd causes transport-level failures (`-100`,
`_Z_ERR_TRANSPORT_TX_FAILED`) that look like network issues.

nros pins zenohd and zenoh-pico to the **same version**. zenohd is built from
the `scripts/zenohd/zenoh/` submodule; zenoh-pico from `zpico-sys/zenoh-pico/`.

## ESP32-C3 (RISC-V)

### picolibc errno TLS crash

picolibc's `errno.h` uses `__thread` (thread-local storage), which crashes on
bare-metal RISC-V because TLS is not initialized. The crash is a hard fault
with no useful backtrace.

**Fix:** Shadow `errno.h` with a non-TLS version:

```c
// errno_override.h — no __thread, just a global
extern int errno;
#define EPERM  1
#define ENOENT 2
// ... subset of errno codes
```

Include this with `-include errno_override.h` in CFLAGS before picolibc headers.

### Flash image format

ESP32-C3 QEMU requires a merged flash image, not a raw binary:

```bash
espflash save-image --merge --chip esp32c3 target/riscv32imc-unknown-none-elf/release/app app.bin
```

The `--merge` flag creates a 4 MB image with bootloader, partition table, and
application combined.

## FreeRTOS

### Task priority inversion

The LAN9118 RX FIFO poll task and zenoh-pico read task must run at the **same
priority** (typically priority 4). If the read task has higher priority, it
monopolizes the CPU, preventing RX FIFO drain. TCP keep-alives are missed, and
zenoh sessions expire.

Similarly, lwIP's `tcpip_thread` must run at the same priority. Lower priority
stalls packet processing.

### Recursive mutexes required

zenoh-pico's FFI layer requires recursive mutexes. Enable in FreeRTOS config:

```c
#define configUSE_RECURSIVE_MUTEXES 1
```

### Interrupt priority constraints

On Cortex-M3 (3-bit priority, 8 levels): ISRs at priority >= 5
(`configLIBRARY_MAX_SYSCALL_INTERRUPT_PRIORITY`) cannot call FreeRTOS APIs.
Ethernet IRQ handlers that signal tasks must use a lower (numerically higher)
priority.

## Build System

### Parallel test build races

When nextest runs test files in parallel and multiple tests build the same
example with different features, cargo fingerprinting creates race conditions —
one test overwrites the binary another test is about to execute.

**Fix:** Use `--target-dir` for each feature variant:

```rust
Command::new("cargo")
    .args(["build", "--release", "--features", "safety-e2e"])
    .arg("--target-dir").arg("target-safety")
    .status()?;
```

Add each target dir to the example's `.gitignore`:

```gitignore
/target/
/target-safety/
/target-zero-copy/
```

### CMake cache invalidation

Changes to CMake defines (like `Z_FEATURE_INTEREST=0`) are cached. Changing the
value without cleaning the build has no effect.

**Fix:**

```bash
# Cargo (native)
cargo clean -p zpico-sys && cargo build

# Zephyr (west)
west build --pristine
```

### Feature flag mutual exclusivity

The three platform axes — RMW backend, platform, ROS edition — are mutually
exclusive within each axis. Enabling two platforms (e.g.,
`platform-posix,platform-zephyr`) causes compile errors with confusing messages
about duplicate symbol definitions, not a clear "pick one" error.

## Test Infrastructure

### Orphan process prevention

When nextest is killed (Ctrl-C, OOM, timeout), child processes (zenohd, QEMU)
can become orphans that hold ports and TAP devices.

**Fix:** Use `PR_SET_PDEATHSIG(SIGKILL)` in the pre-exec hook:

```rust
use std::os::unix::process::CommandExt;

let mut cmd = Command::new("zenohd");
unsafe {
    cmd.pre_exec(|| {
        // Kill this child when parent dies
        libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
        // New process group for tree cleanup
        libc::setpgid(0, 0);
        Ok(())
    });
}
```

**Important:** Do not use `pkill` to clean up zenohd — other agents or users
may have their own zenohd instances. Use process groups and death signals
instead.

### Sequential test execution for shared resources

Tests that use QEMU networking (TAP devices, zenohd on fixed ports) must run
sequentially. Use nextest test groups:

```toml
# .config/nextest.toml
[test-groups.qemu-network]
max-threads = 1
```

### Stale TCP connection cleanup

Between sequential QEMU tests, host-side TCP sockets may linger in `FIN-WAIT-1`
(the QEMU peer was killed before completing the TCP close handshake). These
cause 4-tuple collisions when the next QEMU instance reuses the same source port.

**Fix:** Call `ss -K dst <ip>` between tests to force-destroy stale sockets:

```rust
pub fn cleanup_tap_network() {
    for ip in &["192.0.3.10", "192.0.3.11"] {
        let _ = Command::new("ss")
            .args(["-K", "dst", ip])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    std::thread::sleep(Duration::from_secs(1));
}
```
