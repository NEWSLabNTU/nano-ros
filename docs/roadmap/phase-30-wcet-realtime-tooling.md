# Phase 30: WCET & Real-Time Schedulability Tooling

## Summary

Integrate practical WCET analysis and real-time schedulability verification tools into the nano-ros build pipeline for both Rust and C examples. This replaces the existing illustrative documentation (`docs/design/wcet-analysis.md`, `docs/design/schedulability-analysis.md`) with measured baselines and automated CI checks.

## Current State

The existing documentation covers theory (RMA, RTA, Priority Ceiling Protocol) and mentions several tools, but:

- **No measured WCET baselines** exist — all numbers are illustrative
- **RAUK** and **RTIC-Scope** are dormant (last releases 2022, RTIC v1 only)
- **No CI integration** — all analysis is manual
- **No Zephyr/C coverage** — existing docs focus on RTIC (Rust) only

## Tool Landscape Assessment

### Practical for nano-ros (recommended)

| Tool | Language | What It Does | Static/Measured | Difficulty |
|------|----------|-------------|-----------------|------------|
| **DWT cycle counter** | Rust, C | Cycle-exact timing of instrumented code sections | Measured | Trivial |
| **cargo-call-stack** | Rust | Static call graph + max stack depth per path | Static | Easy |
| **cargo-show-asm + llvm-mca** | Rust | Assembly inspection + throughput estimates | Static | Easy |
| **Kani** | Rust | Bounded model checking (panics, overflow, unbounded loops) | Static | Easy |
| **Zephyr tracing** | C (Zephyr) | Thread scheduling events, CPU load, ISR timing | Measured | Easy |
| **Platin** | Rust, C (ELF) | Static WCET upper bounds via IPET | Static | Moderate |

### Not recommended for nano-ros currently

| Tool | Reason |
|------|--------|
| **RAUK** | Unmaintained, tied to RTIC v1 pre-release |
| **RTIC-Scope** | Dormant since 2022, RTIC v1 only |
| **Symex** | No timing model, only path exploration |
| **AbsInt aiT** | Commercial license required, expensive |
| **OTAWA** | Steep learning curve, sparse Cortex-M docs |
| **Chronos** | No Rust support, no Cortex-M, unmaintained |
| **MIRAI** | Orphaned by Meta, not timing-focused |

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

### 30.4: Kani Verification for Real-Time Properties

**Goal:** Prove absence of panics, unbounded loops, and integer overflow in core serialization and deserialization paths.

**Setup:**

```bash
cargo install --locked kani-verifier
cargo kani setup
```

**Verification harnesses (add to `nano-ros-core` and `nano-ros-serdes`):**

```rust
#[cfg(kani)]
mod verification {
    use super::*;

    #[kani::proof]
    #[kani::unwind(5)]
    fn cdr_write_i32_no_panic() {
        let mut buf = [0u8; 256];
        let mut writer = CdrWriter::new_with_header(&mut buf).unwrap();
        let val: i32 = kani::any();
        // Should never panic for any i32 value
        let _ = writer.write_i32(val);
    }

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

    #[kani::proof]
    #[kani::unwind(10)]
    fn publish_buffer_bounds() {
        // Verify that serialization into a fixed buffer
        // either succeeds or returns BufferTooSmall (never panics)
        let mut buf = [0u8; 8]; // Small buffer
        let result = CdrWriter::new_with_header(&mut buf);
        // Should return Err, not panic
        match result {
            Ok(mut w) => { let _ = w.write_i32(kani::any()); }
            Err(_) => {} // BufferTooSmall is fine
        }
    }
}
```

**Justfile recipe:**

```just
# Run Kani verification on core crates
verify-kani:
    cd packages/core/nano-ros-serdes && cargo kani
    cd packages/core/nano-ros-core && cargo kani
```

**What it proves:**
- CDR serialization never panics for any valid input
- Buffer overflow returns `Err`, never UB
- Deserialization of any byte sequence either produces valid output or `Err`

**Limitations:**
- Cannot verify timing properties (only functional correctness)
- Embedded BSP crates require `no_std` harness support (Kani's `no_std` support is experimental)
- Slow on complex functions (minutes per harness)

### 30.5: Zephyr Tracing for C Examples

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

### 30.6: Platin Static WCET Analysis (Stretch Goal)

**Goal:** Produce static WCET upper bounds for bare-metal Rust and C examples using Platin.

**Why Platin:** Open source, supports ARMv7-M (QEMU/STM32F4) and RISC-V RV32IMC (ESP32-C3), analyzes ELF binaries (language-agnostic).

**Status:** Academic tool presented at WCET 2024. Integration requires building from source and understanding IPET methodology. This is a stretch goal — proceed only after 30.1–30.5 are validated.

**Approach:**

1. Build the Platin toolchain
2. Annotate loop bounds (required for tight WCET bounds)
3. Run analysis on compiled ELF binaries
4. Compare static bounds with DWT measurements from 30.1

**Loop bound annotations (Rust):**

```rust
// Platin reads annotations from the binary's debug info.
// For Rust, loop bounds must be provided via external annotation file.
// Example annotation (platin YAML format):
// - function: nano_ros_bsp_qemu::publisher::Publisher::publish
//   loops:
//     - line: 44
//       bound: 1   # serialize loop runs once for Int32
```

**Practical limitations:**
- Cortex-M3 microarchitecture model may need manual tuning
- zenoh-pico C code has complex control flow (many loops, function pointers)
- Useful for nano-ros core (serialization, keyexpr formatting) but impractical for full publish path including zenoh-pico transport

## Work Items

| ID | Task | Effort | Priority |
|----|------|--------|----------|
| 30.1 | DWT measurement infrastructure + baselines | 2 days | High |
| 30.2 | cargo-call-stack CI recipe | 0.5 day | High |
| 30.3 | cargo-show-asm recipes + critical function docs | 0.5 day | Medium |
| 30.4 | Kani verification harnesses for CDR | 1 day | Medium |
| 30.5 | Zephyr tracing overlay + C measurement code | 1 day | Medium |
| 30.6 | Platin static WCET (stretch) | 3+ days | Low |

## Verification

```bash
just quality              # Existing checks still pass
just check-stack          # Stack analysis (30.2)
just verify-kani          # Kani proofs (30.4)
```

DWT measurements (30.1) and Zephyr tracing (30.5) require hardware or QEMU and are run manually or via `just test-qemu`.

## References

- [cargo-call-stack](https://github.com/japaric/cargo-call-stack) — stack analysis (nightly, v0.1.16)
- [cargo-show-asm](https://github.com/pacak/cargo-show-asm) — assembly inspection (stable, v0.2.50)
- [Kani](https://github.com/model-checking/kani) — bounded model checking (v0.67.0, monthly releases)
- [Platin](https://drops.dagstuhl.de/entities/document/10.4230/OASIcs.WCET.2024.2) — static WCET (ARMv7-M, RISC-V)
- [Zephyr tracing](https://docs.zephyrproject.org/latest/services/tracing/index.html) — CTF/SystemView
- [Percepio View](https://percepio.com/percepio-launches-view-a-free-trace-tool-for-zephyr-rtos/) — free Zephyr trace viewer (2025)
- [AbsInt aiT](https://www.absint.com/ait/index.htm) — commercial static WCET (supports Rust via LLVM binaries)
- [Ferrocene](https://ferrocene.dev) — ISO 26262 / IEC 61508 qualified Rust compiler
- [DWT cycle counting](https://docs.rs/cortex-m/latest/cortex_m/peripheral/struct.DWT.html) — cortex-m crate
- [RTIC-Scope](https://github.com/rtic-scope/cargo-rtic-scope) — dormant since 2022
- [RAUK](https://www.uppsatser.se/uppsats/3122363010/) — academic, unmaintained
