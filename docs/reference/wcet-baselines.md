# WCET Baselines

Worst-Case Execution Time (WCET) measurements for nros core operations, collected via DWT cycle counting on Cortex-M3.

## Platform

| Property     | Value                                              |
|--------------|----------------------------------------------------|
| Target       | ARM Cortex-M3 (`thumbv7m-none-eabi`)               |
| Machine      | QEMU `lm3s6965evb` (for infrastructure validation) |
| Clock        | DWT CYCCNT (cycle-accurate on real hardware)       |
| Iterations   | 100 per benchmark (10 warmup)                      |
| Optimization | `release` profile (`opt-level = "s"`)              |

### DWT Limitation on QEMU

QEMU's Cortex-M3 emulation does not implement the DWT cycle counter — all reads return 0. The benchmark infrastructure is validated by the fact that it compiles, runs, and produces structured output. **Actual cycle counts must be collected on real hardware** (e.g., STM32F4 at 168 MHz, STM32F7 at 216 MHz).

The same binary can run on any Cortex-M3+ target with DWT support. The `lm3s6965evb` machine is used only because it's QEMU's standard Cortex-M3 target with semihosting support.

## Results

All values are in CPU cycles. On QEMU these read as 0; the table structure is ready for real hardware measurements.

### CDR Serialization

| Function | Min | Max | Avg | Notes |
|----------|-----|-----|-----|-------|
| serialize `Int32` | — | — | — | Single i32 field |
| deserialize `Int32` | — | — | — | Single i32 field |
| serialize `Time` | — | — | — | Two fields (i32 + u32) |
| roundtrip `Int32` | — | — | — | Serialize + deserialize |
| serialize w/ header | — | — | — | CDR encapsulation header + Int32 |

### Node API

| Function              | Min | Max | Avg | Notes                          |
|-----------------------|-----|-----|-----|--------------------------------|
| `Node::new()`         | —   | —   | —   | StandaloneNode creation        |
| `create_publisher()`  | —   | —   | —   | Register Int32 publisher       |
| `serialize_message()` | —   | —   | —   | Node-level serialize to buffer |

### Safety E2E

| Function             | Min | Max | Avg | Notes                               |
|----------------------|-----|-----|-----|-------------------------------------|
| `crc32` (64B)        | —   | —   | —   | CRC-32/ISO-HDLC, 64-byte payload    |
| `crc32` (256B)       | —   | —   | —   | CRC-32/ISO-HDLC, 256-byte payload   |
| `crc32` (1024B)      | —   | —   | —   | CRC-32/ISO-HDLC, 1024-byte payload  |
| `validate()`         | —   | —   | —   | SafetyValidator sequence check      |
| full pipeline (128B) | —   | —   | —   | Extract attachment + CRC + validate |

### Sanity Bound

All functions are expected to complete within **100,000 cycles** for typical payloads on a Cortex-M3 at any clock rate. This is a conservative upper bound; actual WCET should be well below this for the measured operations:

- CRC-32 is O(n) in payload size with a single table lookup per byte
- `SafetyValidator::validate()` is O(1) — a few comparisons and an increment
- The full pipeline combines CRC computation with attachment parsing and validation

## Static WCET Analysis Candidates

For certification (ISO 26262 ASIL C/D, DO-178C DAL A/B), dynamic measurement is insufficient — static WCET analysis is required. Candidate tools:

| Tool        | Type                             | License               | LLVM Support       | Notes                                                        |
|-------------|----------------------------------|-----------------------|--------------------|--------------------------------------------------------------|
| **Platin**  | Static (IPET)                    | Open source           | Yes (LLVM bitcode) | Part of the T-CREST project; analyzes LLVM IR + machine code |
| **aiT**     | Static (abstract interpretation) | Commercial (AbsInt)   | ARM targets        | Industry standard for DO-178C / ISO 26262; certified tool    |
| **SWEET**   | Static (flow analysis)           | Academic (Mälardalen) | Limited            | Research tool; good for Cortex-M but less industrial support |
| **Chronos** | Static (IPET)                    | Open source           | No (SimpleScalar)  | Academic; limited embedded target support                    |

**Recommended path**: Use Platin for open-source WCET bounding on the LLVM IR produced by `rustc`. For certification evidence, aiT provides the strongest tool qualification story (pre-qualified per ISO 26262 Part 8).

## Running the Benchmark

```bash
# Build all QEMU examples (includes rs-wcet-bench)
just build-examples-qemu

# Run the WCET benchmark
just test-qemu-wcet
```

Output format:
```
========================================
  nros WCET Benchmark (Cortex-M3)
========================================

Iterations per benchmark: 100

--- CDR Serialization ---
  serialize Int32: min=X max=X avg=X cycles
  ...

--- Node API ---
  Node::new(): min=X max=X avg=X cycles
  ...

--- Safety E2E ---
  crc32 (64B): min=X max=X avg=X cycles
  ...

========================================
  Benchmark complete
========================================

[PASS]
```
