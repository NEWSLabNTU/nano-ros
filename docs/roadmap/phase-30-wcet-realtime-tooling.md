# Phase 30: WCET, Real-Time & Formal Verification Tooling

## Summary

Integrate practical WCET analysis, real-time schedulability verification, and formal verification tools into the nano-ros build pipeline for both Rust and C. This replaces the existing illustrative documentation (`docs/design/wcet-analysis.md`, `docs/design/schedulability-analysis.md`) with measured baselines, automated CI checks, and machine-checked proofs of real-time prerequisites.

## Current State

The existing documentation covers theory (RMA, RTA, Priority Ceiling Protocol) and mentions several tools, but:

- **No measured WCET baselines** exist — all numbers are illustrative
- **RAUK** and **RTIC-Scope** are dormant (last releases 2022, RTIC v1 only)
- **No CI integration** — all analysis is manual
- **No Zephyr/C coverage** — existing docs focus on RTIC (Rust) only
- **No formal verification** — no Kani, CBMC, or Verus proofs exist

## Real-Time Verification Layers

Real-time correctness decomposes into four layers, each requiring different tools:

| Layer  | Property                             | Question                                          | Tools                                    |
|--------|--------------------------------------|---------------------------------------------------|------------------------------------------|
| **L1** | Panic-freedom & bounded control flow | "Can this code path ever diverge or abort?"       | Kani (Rust), CBMC (C)                    |
| **L2** | Functional correctness               | "Does this code always compute the right result?" | Verus (Rust), CBMC (C)                   |
| **L3** | Bounded resource usage               | "Is stack/heap bounded? Do loops terminate?"      | Kani, cargo-call-stack, CBMC             |
| **L4** | WCET bounds                          | "How many cycles does the worst path take?"       | DWT measurement + Platin static analysis |

No single tool covers all four. The plan layers them:

```
Source-level                     Binary-level              Hardware
┌──────────────────────┐        ┌──────────────┐         ┌──────────────┐
│ Kani    (L1, L3)     │        │ Platin       │         │ DWT cycle    │
│ CBMC    (L1, L2, L3) │───────▶│ WCET (L4)    │────────▶│ counters (L4)│
│ Verus   (L1, L2)     │        │              │         │              │
└──────────────────────┘        └──────────────┘         └──────────────┘
 Proves: no panics, correct      Proves: upper bound      Validates: static
 logic, bounded loops             on cycles per path       bounds are realistic
```

## Tool Landscape Assessment

### Formal verification tools

| Tool      | Language | Approach                              | Annotation Burden              | `no_std`                   | Timing                 | Maturity                             |
|-----------|----------|---------------------------------------|--------------------------------|----------------------------|------------------------|--------------------------------------|
| **Kani**  | Rust     | Bounded model checking (CBMC backend) | Low (harness-style)            | Yes (host-verified)        | No                     | Production (AWS, Rust std lib)       |
| **CBMC**  | C        | Bounded model checking (SAT/SMT)      | Low-medium (harnesses + stubs) | Yes (embedded-native)      | Indirect (loop bounds) | Industrial (20+ years, AWS FreeRTOS) |
| **Verus** | Rust     | Deductive verification (Z3 SMT)       | High (4-7x proof:code)         | Yes (vstd supports no_std) | No                     | Research → production (CMU/MSR)      |

### Measurement and analysis tools (recommended)

| Tool                          | Language      | What It Does                                     | Static/Measured | Difficulty |
|-------------------------------|---------------|--------------------------------------------------|-----------------|------------|
| **DWT cycle counter**         | Rust, C       | Cycle-exact timing of instrumented code sections | Measured        | Trivial    |
| **cargo-call-stack**          | Rust          | Static call graph + max stack depth per path     | Static          | Easy       |
| **cargo-show-asm + llvm-mca** | Rust          | Assembly inspection + throughput estimates       | Static          | Easy       |
| **Zephyr tracing**            | C (Zephyr)    | Thread scheduling events, CPU load, ISR timing   | Measured        | Easy       |
| **Platin**                    | Rust, C (ELF) | Static WCET upper bounds via IPET                | Static          | Moderate   |

### Not recommended for nano-ros currently

| Tool           | Reason                                     |
|----------------|--------------------------------------------|
| **RAUK**       | Unmaintained, tied to RTIC v1 pre-release  |
| **RTIC-Scope** | Dormant since 2022, RTIC v1 only           |
| **Symex**      | No timing model, only path exploration     |
| **AbsInt aiT** | Commercial license required, expensive     |
| **OTAWA**      | Steep learning curve, sparse Cortex-M docs |
| **Chronos**    | No Rust support, no Cortex-M, unmaintained |
| **MIRAI**      | Orphaned by Meta, not timing-focused       |

## Verification Targets

Crate survey for formal verification prioritization:

| Crate                | LOC    | Unsafe Blocks | Unit Tests | Verification Priority                              |
|----------------------|--------|---------------|------------|----------------------------------------------------|
| `nano-ros-serdes`    | 1,410  | 0             | 33         | **Tier 1** — pure `no_std` math, ideal for BMC     |
| `nano-ros-core`      | 3,889  | 3             | 75         | **Tier 1** — state machines, duration arithmetic   |
| `nano-ros-params`    | 1,842  | 0             | 30         | **Tier 1** — bounded collections, type conversions |
| `nano-ros-c`         | 9,259  | 445           | 77         | **Tier 1** — C FFI boundary, highest risk          |
| `nano-ros-transport` | 3,422  | 27            | 56         | **Tier 2** — modular contracts at FFI boundary     |
| `nano-ros-node`      | 11,453 | 15            | 124        | **Tier 2** — executor scheduling contracts         |

## Integration Plan

### 30.1: DWT Measurement Infrastructure

**Goal:** Provide a reusable DWT timing harness for Rust BSP examples with measured WCET baselines.

**Targets:**
- QEMU MPS2-AN385 (Cortex-M3) — simulated cycle counter
- STM32F4 (Cortex-M4) — hardware cycle counter
- ESP32-C3 (RISC-V) — `mcycle` CSR counter

**Approach:**

1. Create a `timing` module in each BSP crate with platform-specific cycle counter access:

   ```rust
   // packages/bsp/nano-ros-bsp-qemu/src/timing.rs
   pub struct CycleCounter;

   impl CycleCounter {
       /// Read the current DWT cycle count (Cortex-M3/M4/M7)
       pub fn read() -> u32 {
           cortex_m::peripheral::DWT::cycle_count()
       }

       /// Measure cycles for a closure
       pub fn measure<F: FnOnce() -> R, R>(f: F) -> (R, u32) {
           let start = Self::read();
           let result = f();
           let elapsed = Self::read().wrapping_sub(start);
           (result, elapsed)
       }
   }
   ```

2. Add WCET measurement examples that publish timing results via semihosting/defmt:

   ```rust
   // Measure publish overhead
   let (_, cycles) = CycleCounter::measure(|| {
       publisher.publish(&Int32 { data: 42 }).unwrap();
   });
   hprintln!("publish: {} cycles", cycles);
   ```

3. Collect baseline measurements for core operations:

   | Operation | Expected Range |
   |-----------|---------------|
   | CDR serialize `Int32` | 50–200 cycles |
   | CDR deserialize `Int32` | 50–200 cycles |
   | `publisher.publish(&Int32)` | 500–5,000 cycles |
   | `node.spin_once(0)` (idle) | 200–2,000 cycles |
   | `node.spin_once(0)` (1 msg) | 2,000–10,000 cycles |

**Verification:** Run on QEMU, compare measured cycles with theoretical analysis.

### 30.2: cargo-call-stack Integration

**Goal:** Automated stack usage analysis for all BSP crates in CI.

**Setup:**

```bash
# Install (requires nightly)
cargo +nightly install cargo-call-stack
```

**Usage on BSP crates:**

```bash
# Stack analysis for QEMU BSP publisher example
cargo +nightly call-stack \
    --target thumbv7m-none-eabi \
    --example qemu-bsp-talker \
    -- -C link-arg=-Tlink.x \
    > stack-report.dot

# Convert to text summary
dot -Tsvg stack-report.dot -o stack-report.svg
```

**CI integration (justfile recipe):**

```just
# Stack analysis for embedded targets
check-stack:
    cargo +nightly call-stack --target thumbv7m-none-eabi -p nano-ros-bsp-qemu 2>&1 \
        | grep "Maximum call stack" || echo "See full .dot output"
```

**What it catches:**
- Unexpected recursion in nano-ros or zenoh-pico call chains
- Stack depth exceeding configured limits (e.g., Zephyr's `CONFIG_MAIN_STACK_SIZE`)
- Function pointer / dynamic dispatch indirection

**Limitations:**
- Requires nightly Rust and may break with nightly updates
- Cannot follow C FFI calls into zenoh-pico (only analyzes Rust LLVM IR)
- Function pointers (`fn(&M)` callbacks) cause "unsolvable" warnings

### 30.3: cargo-show-asm for Critical Path Inspection

**Goal:** Document generated assembly for timing-critical functions so developers can reason about cycle counts.

**Usage:**

```bash
# Inspect publish serialization
cargo asm --target thumbv7m-none-eabi -p nano-ros-bsp-qemu \
    'nano_ros_bsp_qemu::publisher::Publisher<M>::publish'

# Run llvm-mca for throughput estimate
cargo asm --target thumbv7m-none-eabi -p nano-ros-bsp-qemu \
    'nano_ros_bsp_qemu::publisher::Publisher<M>::publish' --llvm-mca

# Inspect CDR serialization for a specific message type
cargo asm --target thumbv7m-none-eabi -p nano-ros-core \
    'nano_ros_core::CdrWriter::write_i32'
```

**Justfile recipe:**

```just
# Inspect assembly of critical functions
show-asm target='thumbv7m-none-eabi' fn='':
    cargo asm --target {{target}} {{fn}}
```

**When to use:**
- After optimizing serialization code
- When investigating unexpected cycle counts from DWT measurements
- Before and after compiler upgrades to check for regressions

### 30.4: Kani — Bounded Model Checking for Rust Core Crates

**Goal:** Prove absence of panics, unbounded loops, and integer overflow in core `no_std` crates for all inputs up to the unwind bound.

**Setup:**

```bash
cargo install --locked kani-verifier
cargo kani setup
```

**How Kani works:** Kani compiles Rust MIR to CBMC's goto-program format, unrolls all loops to a configurable bound, encodes the program as a SAT formula, and uses a solver (CaDiCaL) to find counterexamples. If no violation exists within the bound, the property is proven. Kani automatically checks for panics, arithmetic overflow, out-of-bounds access, and user assertions.

**Target crates and harnesses:**

#### nano-ros-serdes (1,410 LOC, 0 unsafe)

```rust
#[cfg(kani)]
mod verification {
    use super::*;

    // Primitive write/read panic-freedom for every CDR type
    #[kani::proof]
    #[kani::unwind(5)]
    fn cdr_write_i32_no_panic() {
        let mut buf = [0u8; 256];
        let mut writer = CdrWriter::new_with_header(&mut buf).unwrap();
        let val: i32 = kani::any();
        let _ = writer.write_i32(val);
    }

    // Serialization round-trip correctness
    #[kani::proof]
    #[kani::unwind(5)]
    fn cdr_roundtrip_i32() {
        let mut buf = [0u8; 256];
        let mut writer = CdrWriter::new_with_header(&mut buf).unwrap();
        let val: i32 = kani::any();
        writer.write_i32(val).unwrap();
        let data = writer.as_slice();
        let mut reader = CdrReader::new_with_header(data).unwrap();
        let result = reader.read_i32().unwrap();
        assert_eq!(val, result);
    }

    // Buffer exhaustion returns Err, never panics
    #[kani::proof]
    #[kani::unwind(10)]
    fn publish_buffer_bounds() {
        let mut buf = [0u8; 8]; // Small buffer
        let result = CdrWriter::new_with_header(&mut buf);
        match result {
            Ok(mut w) => { let _ = w.write_i32(kani::any()); }
            Err(_) => {} // BufferTooSmall is fine
        }
    }

    // Alignment arithmetic never overflows
    #[kani::proof]
    fn alignment_no_overflow() {
        let offset: usize = kani::any();
        let alignment: usize = kani::any();
        kani::assume(alignment > 0 && alignment <= 8);
        kani::assume(offset <= usize::MAX - alignment);
        let padding = (alignment - (offset % alignment)) % alignment;
        let aligned = offset + padding;
        assert!(aligned % alignment == 0);
        assert!(aligned >= offset);
        assert!(aligned < offset + alignment);
    }

    // Deserialization of arbitrary bytes: Ok or Err, never panic
    #[kani::proof]
    #[kani::unwind(5)]
    fn deserialize_arbitrary_bytes_i32() {
        let mut buf = [0u8; 16];
        for i in 0..16 { buf[i] = kani::any(); }
        let result = CdrReader::new_with_header(&buf);
        if let Ok(mut reader) = result {
            let _ = reader.read_i32(); // Ok or Err, not panic
        }
    }
}
```

Similar harnesses for: `u8`, `u16`, `u32`, `u64`, `i8`, `i16`, `i64`, `f32`, `f64`, `bool`, string (bounded).

#### nano-ros-core (3,889 LOC, 3 unsafe)

```rust
#[cfg(kani)]
mod verification {
    use super::*;

    // Action state machine: only valid transitions are reachable
    #[kani::proof]
    fn goal_status_from_i8_bounded() {
        let val: i8 = kani::any();
        let status = GoalStatus::from_i8(val);
        // Must return Some for 0..=6, None otherwise
        if (0..=6).contains(&val) {
            assert!(status.is_some());
        } else {
            assert!(status.is_none());
        }
    }

    // Duration arithmetic never overflows
    #[kani::proof]
    fn duration_from_nanos_no_overflow() {
        let nanos: u64 = kani::any();
        let dur = Duration::from_nanos(nanos);
        // Must not panic for any u64 value
        let _ = dur;
    }

    // GoalResponse acceptance check is consistent
    #[kani::proof]
    fn goal_response_is_accepted_consistent() {
        let val: i8 = kani::any();
        kani::assume(val >= 0 && val <= 2);
        let resp = GoalResponse::from_i8(val).unwrap();
        assert_eq!(resp.is_accepted(), val >= 1);
    }
}
```

#### nano-ros-params (1,842 LOC, 0 unsafe)

```rust
#[cfg(kani)]
mod verification {
    use super::*;

    // Bounded string operations never panic
    #[kani::proof]
    #[kani::unwind(8)]
    fn parameter_name_bounded() {
        let len: usize = kani::any();
        kani::assume(len <= 256);
        // heapless::String<256> push_str with bounded length
        let mut s = heapless::String::<256>::new();
        let byte: u8 = kani::any();
        kani::assume(byte.is_ascii());
        let _ = s.push(byte as char); // Ok or Err, not panic
    }

    // Type conversion round-trip fidelity
    #[kani::proof]
    fn parameter_i64_roundtrip() {
        let val: i64 = kani::any();
        let pv = ParameterValue::from_i64(val);
        assert_eq!(pv.as_i64(), Some(val));
    }
}
```

**Justfile recipe:**

```just
# Run Kani bounded model checking on core crates
verify-kani:
    cargo kani -p nano-ros-serdes
    cargo kani -p nano-ros-core
    cargo kani -p nano-ros-params
```

**What this proves for real-time:** Every code path in the serialization pipeline either completes normally or returns `Err`. No panics, no arithmetic overflow, no unbounded loops. This guarantees the worst-case path is always finite and predictable — a prerequisite for any WCET analysis.

**Limitations:**
- Bounded: proofs hold up to the unwind limit (set per harness)
- Cannot verify timing properties — only functional correctness
- Cannot cross FFI boundary into zenoh-pico C code
- `no_std` crates are verified on the host target, not `thumbv7m-none-eabi`
- Slow on complex functions (minutes per harness)

### 30.5: CBMC — C API Formal Verification

**Goal:** Prove pointer safety, buffer bounds, and absence of undefined behavior in the C FFI layer (`nano-ros-c`, 9,259 LOC, 445 unsafe blocks).

**How CBMC works:** CBMC is a bounded model checker for C/C++. It compiles C code to a "goto-program" IR, unrolls loops to a bound, encodes the program as a SAT/SMT formula, and exhaustively checks for property violations. It automatically detects null dereferences, buffer overflows, signed overflow, division by zero, and use of uninitialized variables.

**Setup:**

```bash
# Install CBMC
sudo apt install cbmc  # or from GitHub releases
# Or: brew install cbmc (macOS)
```

**Proof infrastructure (AWS pattern):**

```
packages/core/nano-ros-c/
└── cbmc/
    ├── proofs/
    │   ├── Makefile.common              # Shared build rules
    │   ├── run-cbmc-proofs.py           # CI batch runner
    │   ├── nrc_publisher_publish/
    │   │   ├── Makefile                 # Per-proof config
    │   │   └── nrc_publisher_publish_harness.c
    │   ├── nrc_cdr_serialize_i32/
    │   │   └── ...
    │   ├── nrc_node_create/
    │   │   └── ...
    │   ├── nrc_executor_spin_once/
    │   │   └── ...
    │   └── nrc_service_handle_request/
    │       └── ...
    ├── stubs/
    │   ├── zenoh_stubs.c               # Nondeterministic zenoh-pico models
    │   └── platform_stubs.c            # RTOS abstraction stubs
    └── include/
        └── proof_helpers.h             # __CPROVER_assume wrappers
```

**Example harness — `nrc_publisher_publish`:**

```c
#include "nano_ros_c.h"
#include "proof_helpers.h"

void nrc_publisher_publish_harness(void) {
    // Create nondeterministic but constrained inputs
    nrc_publisher_t *pub = nondet_publisher();
    __CPROVER_assume(pub != NULL);

    size_t len;
    __CPROVER_assume(len <= NRC_MAX_MSG_SIZE);
    uint8_t data[NRC_MAX_MSG_SIZE];

    nrc_ret_t ret = nrc_publisher_publish(pub, data, len);

    // Must return a valid error code, never segfault or UB
    __CPROVER_assert(
        ret == NRC_OK ||
        ret == NRC_ERR_INVALID_ARGUMENT ||
        ret == NRC_ERR_TRANSPORT,
        "valid return code"
    );
}
```

**Per-proof Makefile:**

```makefile
HARNESS_ENTRY = nrc_publisher_publish_harness
HARNESS_FILE = nrc_publisher_publish_harness.c
PROJECT_SOURCES += $(SRCDIR)/src/publisher.c
PROOF_SOURCES += $(PROOFDIR)/$(HARNESS_FILE)
UNWINDSET += nrc_publisher_publish.0:10
CBMC_FLAGS += --pointer-check --bounds-check --signed-overflow-check
CBMC_FLAGS += --unwinding-assertions  # Soundness: verify unwind bound is sufficient
include ../Makefile.common
```

**Key properties to verify:**

| Function Group | Properties | CBMC Flags |
|----------------|------------|------------|
| `nrc_publisher_*` | Null-safe, buffer bounds, valid return codes | `--pointer-check --bounds-check` |
| `nrc_cdr_*` | Buffer never overwritten, alignment correct | `--bounds-check --signed-overflow-check` |
| `nrc_node_*` | Create/destroy lifecycle (no use-after-free) | `--pointer-check` |
| `nrc_executor_*` | Bounded callback dispatch, no null dereference | `--pointer-check --bounds-check` |
| `nrc_service_*` | State machine transitions, request/response safety | `--pointer-check` |
| `nrc_action_*` | Goal lifecycle, UUID handling | `--pointer-check --bounds-check` |

**Stubbing strategy:**

```c
// stubs/zenoh_stubs.c — model zenoh-pico as nondeterministic
int z_put(z_session_t session, z_keyexpr_t key, const uint8_t *data, size_t len) {
    int result;
    __CPROVER_assume(result == 0 || result == -1);
    return result;
}

// stubs/platform_stubs.c — model RTOS primitives
int pthread_mutex_lock(pthread_mutex_t *mutex) {
    __CPROVER_assert(mutex != NULL, "mutex not null");
    return 0;  // Always succeeds in the model
}
```

**Justfile recipe:**

```just
# Run CBMC proofs on C API layer
verify-cbmc:
    cd packages/core/nano-ros-c/cbmc && python3 proofs/run-cbmc-proofs.py
```

**What this proves for real-time:** The C API boundary — where user code meets nano-ros — never exhibits undefined behavior. UB in C can cause arbitrary timing anomalies (compilers assume UB doesn't happen and may delete safety checks). Proving UB-freedom is a prerequisite for trusting any WCET measurement or analysis on the compiled binary.

**Limitations:**
- Bounded: proofs hold up to loop unwind limits (verified via `--unwinding-assertions`)
- Requires manually written stubs for zenoh-pico and RTOS interfaces
- Cannot reason about timing or scheduling
- Concurrent code verification limited to small context-switch bounds

### 30.6: Kani Contracts — Modular Verification at Transport Boundaries

**Goal:** Verify the publish path and executor dispatch using function contracts, without whole-program model checking of large crates.

**Requires:** Kani experimental flag `-Zfunction-contracts`.

The transport layer (3,422 LOC) and node layer (11,453 LOC) are too large for whole-program BMC. Function contracts allow modular verification: prove each function satisfies its contract, then use verified contracts as stubs for callers.

**Key contracts:**

```rust
// Publisher::publish — either succeeds or returns error, never panics
#[kani::requires(/* message serializable, buffer allocated */)]
#[kani::ensures(|result| result.is_ok() || result.is_err())]
#[kani::modifies(/* internal buffer position */)]
fn publish<M: RosMessage>(&self, msg: &M) -> Result<(), RclrsError> { ... }

// Executor::spin_once — processes at most N callbacks
#[kani::requires(timeout_ns >= 0)]
#[kani::ensures(|result| /* bounded iteration count */)]
fn spin_once(&mut self, timeout_ns: i64) -> Result<(), RclrsError> { ... }

// ServiceServer::handle_request — decode failure → error response, not panic
#[kani::requires(/* raw bytes from transport */)]
#[kani::ensures(|result| /* always produces a response */)]
fn handle_request<S: RosService>(&self, raw: &[u8]) -> Result<Vec<u8>, RclrsError> { ... }
```

**Stubbing zenoh-pico FFI:**

```rust
#[kani::stub(zenoh_pico_shim::Session::put, stub_session_put)]
fn stub_session_put(_key: &str, _payload: &[u8]) -> Result<(), TransportError> {
    if kani::any() { Ok(()) } else { Err(TransportError::SendFailed) }
}
```

**Workflow:**
1. Write contracts on leaf functions (serialization, transport put/get)
2. Verify contracts with `#[kani::proof_for_contract(fn)]` harnesses
3. Use `#[kani::stub_verified(fn)]` to replace verified functions with their contracts when verifying callers
4. Build upward: serialize → transport put → publish → spin_once

**What this proves for real-time:** The publish path from user code through serialization to transport handoff has bounded, predictable behavior. Every intermediate function either completes or returns an error — no hidden panics or infinite loops in the middleware.

### 30.7: Zephyr Tracing for C Examples

**Goal:** Enable measurement-based timing analysis for Zephyr C examples (c-talker, c-listener).

**Configuration (add to example `prj.conf`):**

```ini
# Enable tracing subsystem
CONFIG_TRACING=y
CONFIG_TRACING_CTF=y
CONFIG_TRACING_BUFFER_SIZE=4096

# Or use Segger SystemView (if J-Link available)
# CONFIG_SEGGER_SYSTEMVIEW=y

# Enable timing functions for manual measurement
CONFIG_TIMING_FUNCTIONS=y
```

**Manual measurement in C code:**

```c
#include <zephyr/timing/timing.h>

timing_init();
timing_start();

timing_t start = timing_counter_get();
// ... publish message ...
timing_t end = timing_counter_get();

uint64_t cycles = timing_cycles_get(&start, &end);
uint64_t ns = timing_cycles_to_ns(cycles);
printk("publish: %llu ns (%llu cycles)\n", ns, cycles);
```

**CTF trace analysis:**

```bash
# Build with tracing enabled
west build -b native_sim examples/zephyr/c-talker -- -DOVERLAY_CONFIG=overlay-tracing.conf

# Run and capture trace
./build/zephyr/zephyr.elf
# Trace written to ctf_data/

# Analyze with babeltrace2
babeltrace2 ctf_data/ | grep -E "thread_switched|isr_enter"
```

**Overlay config (`examples/zephyr/c-talker/overlay-tracing.conf`):**

```ini
CONFIG_TRACING=y
CONFIG_TRACING_CTF=y
CONFIG_TRACING_BUFFER_SIZE=4096
CONFIG_TIMING_FUNCTIONS=y
```

### 30.8: Platin Static WCET Analysis

**Goal:** Produce static WCET upper bounds for bare-metal Rust and C examples using Platin, with loop bounds fed from Kani/CBMC verification.

**Why Platin:** Open source, supports ARMv7-M (QEMU/STM32F4) and RISC-V RV32IMC (ESP32-C3), analyzes ELF binaries (language-agnostic).

**Status:** Academic tool presented at WCET 2024. Integration requires building from source and understanding IPET methodology.

**Approach:**

1. Build the Platin toolchain
2. Extract verified loop bounds from Kani/CBMC proofs into Platin annotation format
3. Run analysis on compiled ELF binaries
4. Compare static bounds with DWT measurements from 30.1

**Loop bound annotations (derived from Kani unwind bounds):**

```yaml
# platin-annotations.yml
# Auto-generated from Kani harness unwind bounds and CBMC proof configs
- function: nano_ros_serdes::CdrWriter::write_string
  loops:
    - line: 112
      bound: 256    # heapless::String<256> max length (verified by Kani)
- function: nano_ros_serdes::CdrWriter::write_i32
  loops: []         # No loops (verified by Kani: straight-line code)
```

**Validation against DWT measurements:**

```
Operation                  Platin WCET    DWT Measured    Margin
CDR serialize Int32        ~180 cycles    ~142 cycles     ~1.27x
publisher.publish(Int32)   ~4,200 cycles  ~3,800 cycles   ~1.11x
spin_once (1 msg)          ~9,500 cycles  ~8,100 cycles   ~1.17x
```

A margin of 1.1–1.3x between static WCET and measured worst-case indicates the analysis is tight enough to be useful without being dangerously optimistic.

**Practical limitations:**
- Cortex-M3 microarchitecture model may need manual tuning
- zenoh-pico C code has complex control flow (many loops, function pointers)
- Useful for nano-ros core (serialization, keyexpr formatting) but impractical for full publish path including zenoh-pico transport

### 30.9: Verus — Unbounded Deductive Verification (Stretch Goal)

**Goal:** Prove properties for **all inputs** (not just up to a bound) on the most safety-critical algorithms, using SMT-based deductive verification.

**Why Verus:** Kani proves properties up to a loop unwind bound. Verus proves them for all executions, forever. For safety-critical deployments (ISO 26262, DO-178C contexts), unbounded proofs provide the strongest assurance.

**How Verus works:** Code is annotated with `requires`/`ensures` preconditions and postconditions inside `verus! { }` macro blocks. Verus generates verification conditions and discharges them via the Z3 SMT solver. Ghost code (specs, proofs) is erased after verification — the compiled binary is standard Rust.

**Annotation burden:** Expect 4:1 to 7.5:1 lines of specification+proof per line of executable code. This is the highest-effort task in the plan.

**Setup:**

```bash
# Install Verus (from source)
git clone https://github.com/verus-lang/verus
cd verus && tools/get-z3.sh && source tools/activate
vargo build --release
```

**Target 1: CDR alignment logic (unbounded proof)**

```rust
verus! {
    spec fn aligned(offset: nat, alignment: nat) -> nat {
        let padding = (alignment - (offset % alignment)) % alignment;
        offset + padding
    }

    proof fn alignment_always_valid()
        ensures
            forall|offset: nat, align: nat|
                align > 0 ==>
                    aligned(offset, align) % align == 0 &&
                    aligned(offset, align) >= offset &&
                    aligned(offset, align) < offset + align,
    {
        // SMT solver handles this automatically
    }
}
```

**Target 2: Action state machine (exhaustive transition proof)**

```rust
verus! {
    spec fn valid_transition(from: GoalStatus, to: GoalStatus) -> bool {
        match (from, to) {
            (Accepted, Executing) => true,
            (Executing, Succeeded) => true,
            (Executing, Aborted) => true,
            (Executing, Canceling) => true,
            (Canceling, Canceled) => true,
            (Canceling, Aborted) => true,
            _ => false,
        }
    }

    fn transition(&mut self, to: GoalStatus)
        requires valid_transition(self.status, to)
        ensures self.status == to
    { self.status = to; }
}
```

**Target 3: Parameter value bounded storage**
- Prove `ParameterValue` never exceeds heapless bounds for all inputs
- Prove type conversion round-trip fidelity (set i64, get i64 == original)

**What this proves for real-time:** Mathematical certainty across all inputs that core algorithms are correct. Combined with WCET analysis (30.8), this gives: "this operation is correct AND completes within N cycles" — the strongest statement possible for a real-time system.

**Limitations:**
- High annotation burden (2-4 weeks for targets above)
- Verus supports a subset of Rust (some complex borrowing patterns unsupported)
- SMT solver can be unpredictable on complex proofs (timeouts)
- No C support — only applies to Rust code
- Does not verify timing properties directly

## Work Items

| ID | Task | Effort | Priority | Layer |
|----|------|--------|----------|-------|
| 30.1 | DWT measurement infrastructure + baselines | 2 days | High | L4 |
| 30.2 | cargo-call-stack CI recipe | 0.5 day | High | L3 |
| 30.3 | cargo-show-asm recipes + critical function docs | 0.5 day | — |
| 30.4 | Kani proof harnesses for serdes/core/params | 2–3 days | **High** | L1, L3 |
| 30.5 | CBMC proof harnesses for C API | 3–5 days | **High** | L1, L2 |
| 30.6 | Kani function contracts for transport/node | 3–4 days | Medium | L1, L3 |
| 30.7 | Zephyr tracing overlay + C measurement code | 1 day | Medium | L4 |
| 30.8 | Platin static WCET (with verified loop bounds) | 1–2 weeks | Medium | L4 |
| 30.9 | Verus unbounded proofs for critical algorithms | 2–4 weeks | Low | L1, L2 |

**Recommended execution order:** 30.1 → 30.2 → 30.4 → 30.5 → 30.3 → 30.6 → 30.7 → 30.8 → 30.9

Rationale: DWT measurements (30.1) and stack analysis (30.2) provide immediate diagnostic value. Kani (30.4) and CBMC (30.5) are the highest-ROI formal verification steps — low annotation burden, high coverage. Platin (30.8) depends on 30.1 and 30.4/30.5 for loop bounds. Verus (30.9) is last because it has the highest effort, and Kani already covers the bounded case.

## Tool Coverage Matrix

| Component | Kani (30.4) | CBMC (30.5) | Kani Contracts (30.6) | Verus (30.9) | Platin+DWT (30.1,30.8) |
|-----------|:-----------:|:-----------:|:---------------------:|:------------:|:----------------------:|
| `nano-ros-serdes` | Panic-free, roundtrip | — | — | Alignment proof | WCET bounds |
| `nano-ros-core` | State machines, overflow | — | — | Action FSM proof | — |
| `nano-ros-params` | Bounded collections | — | — | Type safety proof | — |
| `nano-ros-c` | — | Pointer safety, UB-free | — | — | — |
| `nano-ros-transport` | — | — | Publish contract | — | — |
| `nano-ros-node` | — | — | Executor contract | — | WCET bounds |
| Full publish path | — | — | — | — | End-to-end WCET |

## Verification

```bash
just quality              # Existing checks still pass
just check-stack          # Stack analysis (30.2)
just verify-kani          # Kani bounded proofs (30.4)
just verify-cbmc          # CBMC C API proofs (30.5)
```

DWT measurements (30.1) and Zephyr tracing (30.7) require hardware or QEMU and are run manually or via `just test-qemu`. Verus (30.9) requires a separate toolchain: `vargo build && vargo verify`.

## References

### Formal verification
- [Kani](https://github.com/model-checking/kani) — bounded model checking for Rust (v0.67.0, monthly releases)
- [CBMC](https://github.com/diffblue/cbmc) — bounded model checking for C/C++ (v6.8.0, 20+ years)
- [Verus](https://github.com/verus-lang/verus) — deductive verification for Rust (CMU/MSR, SOSP 2024)
- [Verify Rust Std Lib](https://model-checking.github.io/verify-rust-std/) — Kani applied to Rust standard library
- [AWS FreeRTOS CBMC proofs](https://github.com/aws/amazon-freertos/blob/main/tools/cbmc/README.md) — industrial CBMC deployment
- [CBMC Starter Kit](https://model-checking.github.io/cbmc-starter-kit/tutorial/index.html) — proof scaffolding tools
- [Atmosphere](https://dl.acm.org/doi/10.1145/3731569.3764821) — verified microkernel built with Verus (SOSP 2025)
- [AutoVerus](https://dl.acm.org/doi/10.1145/3763174) — LLM-driven automated Verus proof generation (OOPSLA 2025)
- [Surveying the Rust Verification Landscape](https://arxiv.org/html/2410.01981v1) — tool comparison (2024)

### Measurement and analysis
- [cargo-call-stack](https://github.com/japaric/cargo-call-stack) — stack analysis (nightly, v0.1.16)
- [cargo-show-asm](https://github.com/pacak/cargo-show-asm) — assembly inspection (stable, v0.2.50)
- [Platin](https://drops.dagstuhl.de/entities/document/10.4230/OASIcs.WCET.2024.2) — static WCET (ARMv7-M, RISC-V)
- [Zephyr tracing](https://docs.zephyrproject.org/latest/services/tracing/index.html) — CTF/SystemView
- [Percepio View](https://percepio.com/percepio-launches-view-a-free-trace-tool-for-zephyr-rtos/) — free Zephyr trace viewer (2025)
- [DWT cycle counting](https://docs.rs/cortex-m/latest/cortex_m/peripheral/struct.DWT.html) — cortex-m crate

### Standards and qualified toolchains
- [AbsInt aiT](https://www.absint.com/ait/index.htm) — commercial static WCET (supports Rust via LLVM binaries)
- [Ferrocene](https://ferrocene.dev) — ISO 26262 / IEC 61508 qualified Rust compiler

### Dormant / not recommended
- [RTIC-Scope](https://github.com/rtic-scope/cargo-rtic-scope) — dormant since 2022
- [RAUK](https://www.uppsatser.se/uppsats/3122363010/) — academic, unmaintained
