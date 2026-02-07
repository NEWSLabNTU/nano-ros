# Phase 25: Sleep Reduction

**Status: Complete**

## Summary

Audit (`docs/sleep-audit.md`) identified sleep-based scheduling across
production code, test infrastructure, and examples. This phase eliminates
unnecessary sleeps, replacing them with event-driven mechanisms (condvars,
`poll(2)`, readiness markers) to reduce latency in production and wall-clock
time in tests.

## Progress

| Task | Status | Description |
|------|--------|-------------|
| 25.1 Executor spin wake mechanism | Done | Replace 10 ms poll sleep with condvar/eventfd wake |
| 25.2 Service call condvar wait | Done | Replace 10 ms poll loop in `zenoh_shim_get` with condvar |
| 25.3 Test output polling with `poll(2)` | Done | Replace `sleep(50ms)` loops with fd polling |
| 25.4 Remove zenohd fixture sleep | Done | Drop 500 ms post-port-check delay |
| 25.5 Event-driven test synchronization | Done | Replace fixed `sleep(N)` with `wait_for_pattern` |
| 25.6 BSP network readiness polling | Done | Replace 2 s sleep with `net_if_is_up` poll |

## 25.1 Executor Spin Wake Mechanism

**Priority: P0 — production latency**

**Files:**
- `crates/nano-ros-node/src/executor.rs` (BasicExecutor)
- `crates/nano-ros-transport/src/zenoh/` (subscription callback)

**Current behavior:**

`BasicExecutor::spin()` (line 1426) loops with a fixed 10 ms sleep:

```rust
loop {
    let result = self.spin_once(POLL_INTERVAL_MS);  // check buffers, process ready data
    // ...
    std::thread::sleep(Duration::from_millis(POLL_INTERVAL_MS));  // 10 ms
}
```

`spin_once()` (line 1354) checks subscription/service buffers and processes
any ready data. It does NOT block on I/O — the zenoh-pico background read
thread delivers data to buffers via callbacks. The 10 ms sleep is the only
rate limiter: data that arrives immediately after a check waits up to 10 ms
to be noticed.

**Fix:**

Add a wake mechanism so the background thread can notify the executor:

```rust
// In BasicExecutor
struct BasicExecutor {
    // ...
    wake: Arc<(Mutex<bool>, Condvar)>,
}

// In spin()
loop {
    let result = self.spin_once(delta_ms);
    if result.total() == 0 {
        // No work — wait for notification or timeout
        let (lock, cvar) = &*self.wake;
        let guard = lock.lock().unwrap();
        let _ = cvar.wait_timeout(guard, Duration::from_millis(POLL_INTERVAL_MS));
    }
    // If work was done, loop immediately without sleeping
}

// In subscription callback (called from zenoh read thread)
fn on_data_received(&self) {
    let (lock, cvar) = &*self.wake;
    *lock.lock().unwrap() = true;
    cvar.notify_one();
}
```

When idle, the executor sleeps up to 10 ms (same as before, saves CPU). When
data arrives, the condvar wakes the executor immediately. Zero-latency
dispatch for incoming messages.

**`no_std` consideration:** The condvar path is `std`-only. The `PollingExecutor`
(for `no_std`/RTIC) does not have a spin loop — the caller controls timing.
No change needed there.

**Verification:** `just quality`, `just test-integration`

## 25.2 Service Call Condvar Wait

**Priority: P0 — production latency**

**File:** `crates/zenoh-pico-shim-sys/c/shim/zenoh_shim.c`

**Current behavior** (line 991):

```c
// Multi-threaded: wait for completion with timeout
while (!ctx.done && elapsed < timeout_ms) {
    z_sleep_ms(poll_interval);   // 10 ms
    elapsed += poll_interval;
}
```

The reply dropper callback (`shim_get_reply_dropper`) sets `ctx.done = true`
from the read thread. The caller polls this flag at 10 ms intervals.

**Fix:**

Add a condvar to `get_reply_ctx_t`:

```c
typedef struct {
    uint8_t buf[ZENOH_SHIM_GET_REPLY_BUF_SIZE];
    size_t len;
    bool received;
    bool done;
#if Z_FEATURE_MULTI_THREAD == 1
    _z_mutex_t mutex;
    _z_condvar_t cond;
#endif
} get_reply_ctx_t;
```

Initialize before `z_get`, signal in dropper, wait in caller:

```c
// Caller (multi-threaded path)
_z_mutex_lock(&ctx.mutex);
while (!ctx.done) {
    z_result_t r = _z_condvar_wait_until(&ctx.cond, &ctx.mutex, &deadline);
    if (r == Z_ETIMEDOUT) break;
}
_z_mutex_unlock(&ctx.mutex);

// Dropper callback (read thread)
_z_mutex_lock(&rctx->mutex);
rctx->done = true;
_z_condvar_signal(&rctx->cond);
_z_mutex_unlock(&rctx->mutex);
```

The single-threaded path (`Z_FEATURE_MULTI_THREAD == 0`) keeps its existing
`zp_read` polling loop unchanged.

**Verification:** `just quality`, `just test-integration`, `just test-zephyr`

## 25.3 Test Output Polling with `poll(2)`

**Priority: P1 — test speed**

**Files:**
- `crates/nano-ros-tests/src/lib.rs` (`wait_for_pattern`, `collect_output`)
- `crates/nano-ros-tests/src/process.rs` (`wait_for_output`, `wait_for_all_output`)

**Current behavior:**

All output-reading functions use non-blocking reads with 50 ms sleeps:

```rust
match reader.read_line(&mut line) {
    Ok(0) => std::thread::sleep(Duration::from_millis(50)),
    Err(WouldBlock) => std::thread::sleep(Duration::from_millis(50)),
    // ...
}
```

**Fix:**

Use `poll(2)` via the `nix` crate (already a transitive dependency via
`libc`) to wait for fd readability:

```rust
use std::os::unix::io::AsRawFd;
use nix::poll::{poll, PollFd, PollFlags};

fn poll_readable(fd: std::os::unix::io::RawFd, timeout_ms: i32) -> bool {
    let mut fds = [PollFd::new(fd, PollFlags::POLLIN)];
    matches!(poll(&mut fds, timeout_ms), Ok(1))
}
```

Replace `sleep(50ms)` with `poll_readable(fd, remaining_ms)`. When data is
available, the poll returns immediately. When not, it blocks until data
arrives or timeout — no busy-loop overhead.

For `zephyr.rs` which uses `mpsc::channel`, replace `try_recv` + `sleep(100ms)`
with `recv_timeout(remaining)`:

```rust
match rx.recv_timeout(remaining) {
    Ok(output) => { /* process */ },
    Err(RecvTimeoutError::Timeout) => { /* check process, continue */ },
    Err(RecvTimeoutError::Disconnected) => break,
}
```

**Verification:** `just test-integration`, `just test-zephyr`

## 25.4 Remove Zenohd Fixture Sleep

**Priority: P1 — test speed (saves 500 ms per fixture)**

**File:** `crates/nano-ros-tests/src/fixtures/zenohd_router.rs`

**Current behavior** (line 68):

```rust
if !wait_for_port(port, Duration::from_secs(5)) {
    return Err(TestError::Timeout);
}
std::thread::sleep(Duration::from_millis(500));  // "full initialization"
```

**Fix:**

Remove the 500 ms sleep. The `wait_for_port` TCP connect already proves
zenohd is accepting connections. If a race exists where the session is not
ready despite the port being open, it should be handled by zenoh client
connection retry logic — not a global startup delay.

If removal causes flakiness, add a zenoh session open + close probe instead:

```rust
// Verify zenohd accepts zenoh protocol, not just TCP
fn probe_zenohd(locator: &str, timeout: Duration) -> bool {
    // Attempt z_open with short timeout; close immediately on success
}
```

**Verification:** `just test-integration` (run 3x to check for flakiness)

## 25.5 Event-Driven Test Synchronization

**Priority: P1 — test speed (largest aggregate savings)**

**Files:** All test suites in `crates/nano-ros-tests/tests/`

**Current behavior:**

Tests use fixed sleeps for process synchronization:

```rust
// nano2nano.rs:95
std::thread::sleep(Duration::from_secs(2));  // wait for subscriber

// nano2nano.rs:104
std::thread::sleep(Duration::from_secs(5));  // communication window
```

**Fix (two parts):**

### 25.5a — Startup readiness markers

Each example process should print a recognizable ready line after
initialization completes. Then tests use `wait_for_pattern` instead of
fixed sleep:

```rust
// Before:
let mut listener = ManagedProcess::spawn_command(cmd, "listener")?;
std::thread::sleep(Duration::from_secs(2));

// After:
let mut listener = ManagedProcess::spawn_command(cmd, "listener")?;
wait_for_pattern(&mut listener.stdout_reader(), "Subscription active", Duration::from_secs(5))?;
```

Example processes to add ready markers:
- `rs-talker`: print after publisher declared
- `rs-listener`: print after subscriber declared
- `rs-service-server`: print after queryable declared
- `rs-action-server`: print after all channels declared

### 25.5b — Message-count-based communication windows

Replace `sleep(5s)` communication windows with output-driven waits:

```rust
// Before:
std::thread::sleep(Duration::from_secs(5));  // hope for ~5 messages

// After:
wait_for_pattern(&mut listener.stdout_reader(), "Received: 3", Duration::from_secs(10))?;
```

This returns as soon as 3 messages arrive (typically < 1 s for 1 Hz pub
at ~100 ms subscriber setup time). The 10 s timeout is a safety net.

**Scope:** Start with the highest-impact test files:
1. `nano2nano.rs` — 4 fixed sleeps totaling ~11 s per test
2. `services.rs` — 5+ fixed sleeps totaling ~16 s per test
3. `actions.rs` — 3 fixed sleeps totaling ~21 s per test
4. `executor.rs` — 6+ fixed sleeps
5. `zephyr.rs` — already mostly event-driven, minor improvements

**Verification:** `just test-integration`, `just test-zephyr` (run 3x for flakiness)

## 25.6 BSP Network Readiness Polling

**Priority: P2 — embedded boot time**

**File:** `crates/nano-ros-bsp-zephyr/src/bsp_zephyr.c`

**Current behavior** (line 59):

```c
k_sleep(K_MSEC(2000));  // unconditional 2s wait
```

**Fix:**

Poll the network interface readiness using Zephyr's `net_if` API:

```c
#include <zephyr/net/net_if.h>

struct net_if *iface = net_if_get_default();
int elapsed = 0;
int timeout = CONFIG_NANO_ROS_INIT_DELAY_MS;  // or 2000 default
while (!net_if_is_up(iface) && elapsed < timeout) {
    k_sleep(K_MSEC(50));
    elapsed += 50;
}
if (!net_if_is_up(iface)) {
    LOG_ERR("Network interface not ready after %d ms", timeout);
    return NANO_ROS_BSP_ERR_CONNECT;
}
```

For zero-poll startup (optional future optimization), use Zephyr's network
management event callback:

```c
#include <zephyr/net/net_mgmt.h>

static K_SEM_DEFINE(net_ready, 0, 1);

static void net_event_handler(struct net_mgmt_event_callback *cb,
                              uint32_t mgmt_event, struct net_if *iface) {
    if (mgmt_event == NET_EVENT_IF_UP) {
        k_sem_give(&net_ready);
    }
}

// In init:
net_mgmt_add_event_callback(&cb);
k_sem_take(&net_ready, K_MSEC(timeout));
```

**Verification:** `just test-zephyr`

## Files Modified

| File | Changes |
|------|---------|
| `crates/nano-ros-node/src/executor.rs` | Add condvar wake to BasicExecutor spin loop |
| `crates/nano-ros-transport/src/zenoh/` | Signal executor wake from subscription callback |
| `crates/zenoh-pico-shim-sys/c/shim/zenoh_shim.c` | Condvar wait in `zenoh_shim_get` |
| `crates/nano-ros-tests/src/lib.rs` | `poll(2)` in wait_for_pattern, collect_output |
| `crates/nano-ros-tests/src/process.rs` | `poll(2)` in wait_for_output |
| `crates/nano-ros-tests/src/zephyr.rs` | `recv_timeout` instead of try_recv + sleep |
| `crates/nano-ros-tests/src/fixtures/zenohd_router.rs` | Remove 500 ms sleep |
| `crates/nano-ros-tests/tests/nano2nano.rs` | Event-driven sync |
| `crates/nano-ros-tests/tests/services.rs` | Event-driven sync |
| `crates/nano-ros-tests/tests/actions.rs` | Event-driven sync |
| `crates/nano-ros-tests/tests/executor.rs` | Event-driven sync |
| `crates/nano-ros-bsp-zephyr/src/bsp_zephyr.c` | net_if_is_up polling |
| `examples/native/rs-talker/src/main.rs` | Add ready marker |
| `examples/native/rs-listener/src/main.rs` | Add ready marker |
| `examples/native/rs-service-server/src/main.rs` | Add ready marker |
| `examples/native/rs-action-server/src/main.rs` | Add ready marker |

## Verification

```bash
just quality             # format + clippy + unit tests
just test-integration    # all integration tests (3x for flakiness)
just test-zephyr         # Zephyr E2E tests
just test-c              # C API tests
```
