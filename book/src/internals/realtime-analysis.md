# Real-Time Analysis

This chapter describes static analysis methods and tools for detecting anti-patterns that violate real-time guarantees in Rust embedded applications.

## Overview

Real-time systems require deterministic execution times. Common anti-patterns that break real-time guarantees include:

| Anti-Pattern | Problem | Detection Method |
|--------------|---------|------------------|
| Unbounded loops | Infinite execution time | Clippy + custom lints |
| Recursion | Stack overflow, unbounded depth | cargo-call-stack |
| Heap allocation | Non-deterministic timing, fragmentation | no_std + forbid patterns |
| Blocking I/O | Unbounded wait times | Custom lints |
| Missing timeouts | Operations can hang forever | Custom lints |
| Large stack frames | Stack overflow | cargo-call-stack |

## Built-in Clippy Lints

### Loop and Iteration Lints

```bash
# Enable all loop-related lints
cargo clippy -- \
    -D clippy::infinite_iter \
    -D clippy::while_immutable_condition \
    -D clippy::never_loop \
    -D clippy::empty_loop
```

| Lint | Detects |
|------|---------|
| `infinite_iter` | Iterator chains guaranteed to be infinite |
| `while_immutable_condition` | Loop conditions that can never change |
| `never_loop` | Loops that exit on first iteration |
| `empty_loop` | `loop { }` without body (use `loop { hint::spin_loop() }`) |

### Memory and Performance Lints

```bash
cargo clippy -- \
    -W clippy::large_stack_arrays \
    -W clippy::large_types_passed_by_value \
    -W clippy::box_collection \
    -W clippy::rc_buffer
```

### Recommended Clippy Configuration

Create `clippy.toml` in your project root:

```toml
# Maximum size for stack-allocated arrays (bytes)
array-size-threshold = 512

# Warn on types larger than this passed by value
trivial-copy-size-limit = 16

# Cognitive complexity threshold
cognitive-complexity-threshold = 15
```

### Running Clippy for Real-Time Code

```bash
# Strict mode for real-time code
cargo clippy --all-targets -- \
    -D warnings \
    -D clippy::all \
    -W clippy::pedantic \
    -D clippy::infinite_iter \
    -D clippy::while_immutable_condition \
    -A clippy::module_name_repetitions
```

## Stack Analysis with cargo-call-stack

### Installation and Usage

```bash
# Install (requires nightly)
cargo +nightly install cargo-call-stack

# Build with stack size info
cd examples/stm32f4/rust/zenoh/rtic
RUSTFLAGS="-Z emit-stack-sizes" cargo +nightly build --release

# Generate call graph with stack sizes
cargo +nightly call-stack --release > call_graph.dot

# Visualize (requires graphviz)
dot -Tsvg call_graph.dot -o call_graph.svg
```

### Interpreting Results

The output shows:
- Each function's stack frame size
- Call graph relationships
- Maximum stack depth through any path
- Cycles (recursion) in the call graph

**Example output:**
```
digraph {
    "main" [label="main\n256 bytes"]
    "zenoh_poll" [label="zenoh_poll\n128 bytes"]
    "publisher_task" [label="publisher_task\n512 bytes"]

    "main" -> "zenoh_poll"
    "main" -> "publisher_task"
}
```

### Limitations

- Requires fat LTO (`lto = "fat"` in Cargo.toml)
- Limited support for programs linking `std`
- Indirect calls (function pointers, trait objects) may not be analyzed
- Best for embedded `no_std` programs

## Preventing Heap Allocation

### Method 1: no_std Without Allocator

The simplest approach -- don't provide a global allocator:

```rust
#![no_std]
#![no_main]

// No #[global_allocator] defined
// Attempting to use Box, Vec, String will fail to compile
```

### Method 2: Compile-Time Enforcement

```rust
#![no_std]
#![forbid(unsafe_code)]  // Also prevents custom allocators

use heapless::{Vec, String};  // Static-sized alternatives

// This will NOT compile:
// let v = alloc::vec::Vec::new();  // Error: no allocator

// This works:
let v: heapless::Vec<u8, 256> = heapless::Vec::new();
```

### Method 3: Custom Lint (Dylint)

For projects that need `alloc` but want to restrict usage in certain modules:

```rust
// In real-time critical code, add:
#![deny(clippy::disallowed_methods)]
```

With `clippy.toml`:
```toml
disallowed-methods = [
    { path = "alloc::vec::Vec::push", reason = "Use heapless::Vec in RT code" },
    { path = "alloc::boxed::Box::new", reason = "No heap in RT code" },
    { path = "alloc::string::String::new", reason = "Use heapless::String" },
]
```

## Detecting Missing Timeouts

### Pattern: I/O Operations Without Timeout

Anti-pattern:
```rust
// BAD: No timeout - can block forever
let data = socket.read(&mut buf)?;
```

Correct pattern:
```rust
// GOOD: Explicit timeout
socket.set_read_timeout(Some(Duration::from_millis(100)))?;
let data = socket.read(&mut buf)?;
```

### Manual Audit Checklist

For code review, check that these operations have timeouts:

- `TcpStream::connect()` -- use `connect_timeout()`
- `socket.read()` / `socket.write()` -- set socket timeout
- `channel.recv()` -- use `recv_timeout()`
- zenoh operations -- configure session timeout
- Any blocking syscall

## Detecting Unbounded Loops

### Clippy Detection

```bash
cargo clippy -- -D clippy::infinite_iter -D clippy::while_immutable_condition
```

### Manual Patterns to Audit

**Potentially unbounded:**
```rust
// BAD: No termination guarantee
loop {
    if let Some(msg) = queue.pop() {
        process(msg);
    }
}

// BAD: Condition may never be false
while !flag.load(Ordering::Relaxed) {
    do_work();
}
```

**Bounded alternatives:**
```rust
// GOOD: Bounded iteration count
for _ in 0..MAX_ITERATIONS {
    if let Some(msg) = queue.pop() {
        process(msg);
    } else {
        break;
    }
}

// GOOD: Timeout-based termination
let deadline = Instant::now() + Duration::from_millis(10);
while Instant::now() < deadline {
    if let Some(msg) = queue.pop() {
        process(msg);
    } else {
        break;
    }
}
```

## Detecting Recursion

### cargo-call-stack Cycle Detection

```bash
# Will report cycles in call graph
cargo +nightly call-stack --release 2>&1 | grep -i "cycle"
```

### Clippy Recursion Lint

```bash
# Warn on unconditional recursion
cargo clippy -- -D clippy::unconditional_recursion
```

### Manual Pattern Detection

```rust
// BAD: Direct recursion
fn process(node: &Node) {
    process(&node.child);  // May overflow stack
}

// BAD: Mutual recursion
fn a() { b(); }
fn b() { a(); }

// GOOD: Iterative with explicit stack
fn process(root: &Node) {
    let mut stack: heapless::Vec<&Node, 32> = heapless::Vec::new();
    stack.push(root).ok();

    while let Some(node) = stack.pop() {
        // Process node
        if stack.push(&node.child).is_err() {
            // Handle stack full - bounded!
            break;
        }
    }
}
```

## Advanced: Custom Dylint Lints

### Setting Up Dylint

```bash
# Install dylint
cargo install cargo-dylint dylint-link

# Create a new lint library
cargo dylint new realtime_lints
cd realtime_lints
```

### Recommended Custom Lints for Real-Time

| Lint | Purpose |
|------|---------|
| `blocking_in_async` | Detect std::thread::sleep, blocking I/O in async |
| `unbounded_loop_in_task` | Flag loops without bounds in RTIC tasks |
| `heap_in_interrupt` | Detect allocation in `#[interrupt]` handlers |
| `mutex_in_interrupt` | Flag std::sync::Mutex in interrupt context |
| `float_in_isr` | Warn about FP operations in ISRs (soft-float targets) |
| `missing_timeout` | I/O operations without timeout |

## Tool Summary

| Tool | Detects | Effort | CI-Ready |
|------|---------|--------|----------|
| **Clippy** | Infinite iters, recursion, large arrays | Low | Yes |
| **cargo-call-stack** | Stack usage, recursion cycles | Medium | Yes |
| **Miri** | Undefined behavior, memory errors | Low | Yes |
| **Dylint** | Custom patterns (blocking, heap, etc.) | High | Yes |
| **MIRAI** | Abstract interpretation, all paths | High | Experimental |
| **no_std** | Prevents std heap allocation | Low | Automatic |
| **heapless** | Static collections | Low | N/A |

## Quick Reference: Lint Flags

```bash
# Copy-paste for strict real-time checking
cargo clippy -- \
    -D warnings \
    -D clippy::all \
    -D clippy::infinite_iter \
    -D clippy::while_immutable_condition \
    -D clippy::never_loop \
    -D clippy::empty_loop \
    -D clippy::unconditional_recursion \
    -W clippy::large_stack_arrays \
    -W clippy::large_types_passed_by_value \
    -W clippy::cognitive_complexity
```

## WCET Baselines

Worst-Case Execution Time (WCET) measurements for nros core operations, collected via DWT cycle counting on Cortex-M3.

### Measurement Platform

| Property     | Value                                              |
|--------------|----------------------------------------------------|
| Target       | ARM Cortex-M3 (`thumbv7m-none-eabi`)               |
| Machine      | QEMU `lm3s6965evb` (for infrastructure validation) |
| Clock        | DWT CYCCNT (cycle-accurate on real hardware)       |
| Iterations   | 100 per benchmark (10 warmup)                      |
| Optimization | `release` profile (`opt-level = "s"`)              |

**DWT limitation on QEMU:** QEMU's Cortex-M3 emulation does not implement the DWT cycle counter -- all reads return 0. The benchmark infrastructure is validated by the fact that it compiles, runs, and produces structured output. Actual cycle counts must be collected on real hardware (e.g., STM32F4 at 168 MHz, STM32F7 at 216 MHz).

### Benchmark Categories

**CDR Serialization:**

| Function | Notes |
|----------|-------|
| serialize `Int32` | Single i32 field |
| deserialize `Int32` | Single i32 field |
| serialize `Time` | Two fields (i32 + u32) |
| roundtrip `Int32` | Serialize + deserialize |
| serialize w/ header | CDR encapsulation header + Int32 |

**Node API:**

| Function | Notes |
|----------|-------|
| `Node::new()` | StandaloneNode creation |
| `create_publisher()` | Register Int32 publisher |
| `serialize_message()` | Node-level serialize to buffer |

**Safety E2E:**

| Function | Notes |
|----------|-------|
| `crc32` (64B) | CRC-32/ISO-HDLC, 64-byte payload |
| `crc32` (256B) | CRC-32/ISO-HDLC, 256-byte payload |
| `crc32` (1024B) | CRC-32/ISO-HDLC, 1024-byte payload |
| `validate()` | SafetyValidator sequence check |
| full pipeline (128B) | Extract attachment + CRC + validate |

### Sanity Bound

All functions are expected to complete within **100,000 cycles** for typical payloads on a Cortex-M3 at any clock rate. This is a conservative upper bound; actual WCET should be well below this:

- CRC-32 is O(n) in payload size with a single table lookup per byte
- `SafetyValidator::validate()` is O(1) -- a few comparisons and an increment
- The full pipeline combines CRC computation with attachment parsing and validation

### Running the Benchmark

```bash
# Build all QEMU examples (includes rs-wcet-bench)
just build-examples-qemu

# Run the WCET benchmark
just test-qemu-wcet
```

### Static WCET Analysis

For certification (ISO 26262 ASIL C/D, DO-178C DAL A/B), dynamic measurement is insufficient -- static WCET analysis is required. Candidate tools:

| Tool | Type | License | Notes |
|------|------|---------|-------|
| **Platin** | Static (IPET) | Open source | Analyzes LLVM IR + machine code |
| **aiT** | Static (abstract interpretation) | Commercial (AbsInt) | Industry standard for DO-178C / ISO 26262 |
| **SWEET** | Static (flow analysis) | Academic | Research tool; good for Cortex-M |
| **Chronos** | Static (IPET) | Open source | Academic; limited embedded target support |

**Recommended path**: Use Platin for open-source WCET bounding on the LLVM IR produced by `rustc`. For certification evidence, aiT provides the strongest tool qualification story (pre-qualified per ISO 26262 Part 8).

## References

- [Clippy Lints Reference](https://rust-lang.github.io/rust-clippy/master/index.html)
- [cargo-call-stack](https://github.com/japaric/cargo-call-stack)
- [Dylint - Custom Lints](https://github.com/trailofbits/dylint)
- [MIRAI Abstract Interpreter](https://github.com/facebookexperimental/MIRAI)
- [heapless Crate](https://docs.rs/heapless)
- [Embedded Rust Book](https://docs.rust-embedded.org/book/)
- [RTIC Book](https://rtic.rs/)
