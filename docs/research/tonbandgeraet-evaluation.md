# Tonbandgeraet Evaluation for nano-ros FreeRTOS Tracing

**Date**: 2026-03-26
**Repo**: https://github.com/schilkp/Tonbandgeraet (cloned to `external/Tonbandgeraet/`)
**License**: MIT (tracer library + docs), GPL3 (host tools)
**Status**: Early development — binary format subject to change

## What It Does

Tonbandgeraet is a lightweight embedded tracer for FreeRTOS (and bare-metal) that:
1. Hooks into FreeRTOS trace macros to capture task switches, queue ops, mutex ops, ISRs
2. Encodes events as compact binary (COBS-framed, varint timestamps)
3. Stores in a RAM ring buffer (snapshot mode) or streams out (streaming mode)
4. Host-side Rust CLI converts to **Perfetto protobuf**
5. View at [ui.perfetto.dev](https://ui.perfetto.dev) (browser, free, no install)

## Architecture

```
Embedded (C)                    Host (Rust)              Browser
────────────                    ───────────              ───────
FreeRTOS trace hooks            tband-cli conv           ui.perfetto.dev
  → tband_freertos.c              ↓
  → binary encode (COBS)        Perfetto protobuf (.pf)
  → RAM snapshot buffer           ↓
  → semihosting dump            Open in Perfetto
```

## Embedded Footprint

| Resource | Size | Notes |
|----------|------|-------|
| Code | ~2,750 lines C (3 files) | `tband.c`, `tband_freertos.c`, `tband_backend.c` |
| Snapshot buffer | 32 KB default | Configurable via `tband_configBACKEND_SNAPSHOT_BUF_SIZE` |
| Metadata buffer | 256 B default | Task names, ISR names, event types |
| Stack per event | ~512 B | COBS encoding buffer, within critical section |
| Flash | ~4-8 KB estimate | Depends on enabled features |

## FreeRTOS Events Traced

**Task lifecycle**: create, delete, switched-in, moved-to-ready, suspend, resume, delay, priority-set, priority-inherit/disinherit
**Queues/Semaphores**: create, send, send-from-ISR, receive, receive-from-ISR, blocking-on-send, blocking-on-receive, reset
**User markers**: instant events, begin/end slices, numeric values, function enter/exit, ISR enter/exit

## Integration Steps for nano-ros

### 1. Port header (`tband_port.h`)

For our MPS2-AN385 FreeRTOS board crate (Cortex-M3):

```c
// Critical sections — FreeRTOS-aware (handles ISR context)
#define tband_portENTER_CRITICAL_FROM_ANY()    taskENTER_CRITICAL()
#define tband_portEXIT_CRITICAL_FROM_ANY()     taskEXIT_CRITICAL()

// Single-core
#define tband_portNUMBER_OF_CORES  1
#define tband_portGET_CORE_ID()    0

// Timestamp — use DWT CYCCNT (Cortex-M cycle counter)
// MPS2-AN385 runs at 25 MHz → 40 ns/cycle
static inline uint64_t tband_portTIMESTAMP(void) {
    return (uint64_t)DWT->CYCCNT;
}
#define tband_portTIMESTAMP_RESOLUTION_NS  40  // 25 MHz
```

### 2. Config header (`tband_config.h`)

```c
#define tband_configENABLE                     1
#define tband_configFREERTOS_TRACE_ENABLE       1
#define tband_configISR_TRACE_ENABLE            1
#define tband_configMARKER_TRACE_ENABLE         1
#define tband_configUSE_BACKEND_SNAPSHOT        1
#define tband_configBACKEND_SNAPSHOT_BUF_SIZE   (16 * 1024)  // 16 KB
#define tband_configMETADATA_BUF_SIZE           256
```

### 3. FreeRTOSConfig.h additions

```c
#define configUSE_TRACE_FACILITY          1
#define INCLUDE_xTaskGetIdleTaskHandle    1

// At the END of FreeRTOSConfig.h:
#if (configUSE_TRACE_FACILITY == 1)
#include "tband.h"
#endif
```

### 4. Build (3 source files)

```
tband/src/tband.c
tband/src/tband_freertos.c
tband/src/tband_backend.c
```

Include paths: `tband/inc/`

### 5. Usage in app task

```c
// After scheduler starts:
tband_freertos_scheduler_started();

// ... run test ...

// After test completes:
tband_trigger_snapshot();

// Dump via semihosting:
const uint8_t *meta = tband_get_metadata_buf(0);
size_t meta_len = tband_get_metadata_buf_amnt(0);
const uint8_t *snap = tband_get_core_snapshot_buf(0);
size_t snap_len = tband_get_core_snapshot_buf_amnt(0);
// Write meta + snap to file via semihosting fopen/fwrite/fclose
```

### 6. Convert and view

```bash
# Install CLI (Rust)
cargo install tband-cli

# Convert binary dump → Perfetto format
tband-cli conv --format=bin --core-count=1 trace.bin --output=trace.pf

# Or open directly in browser
tband-cli conv --format=bin --open trace.bin
```

## Feasibility Assessment for nano-ros

### Pros

- **Minimal integration**: 3 C files, ~16-32 KB RAM, no stdlib required
- **FreeRTOS-native**: Hooks all the events we care about (task switches, queue ops, priority changes)
- **QEMU-compatible**: Snapshot mode → semihosting dump after test (our existing pattern)
- **Perfetto visualization**: Professional timeline view, free, browser-based, no install
- **MIT license**: Compatible with our project
- **Validates Phase 76**: Can visually confirm that scheduling config changes actually affect task execution patterns

### Cons

- **Early stage**: API/format may change, limited community
- **DWT CYCCNT on Cortex-M3**: The MPS2-AN385 (Cortex-M3) does NOT have DWT CYCCNT by default — need to check if QEMU emulates it. Alternative: use SysTick or a hardware timer
- **16-32 KB RAM overhead**: Our FreeRTOS heap is 256 KB, so this is manageable (~6-12%)
- **Semihosting file I/O**: Need to implement the dump path (C code for fopen/fwrite via semihosting)
- **Host tool dependency**: `tband-cli` (Rust) must be installed to convert traces

### Open Questions

1. **DWT CYCCNT on QEMU MPS2-AN385**: Does QEMU emulate the DWT cycle counter for Cortex-M3? If not, use SysTick counter (1 kHz = 1 ms resolution) or CMSDK Timer0 (25 MHz)
2. **Semihosting file I/O**: Our C startup code uses ARM semihosting for printf. Need to verify `fopen`/`fwrite` work via the newlib-nano semihosting layer
3. **Integration with test infra**: Should the trace dump be automatic (always) or opt-in (env var)?
4. **Buffer sizing**: With 5 tasks × ~10 events/sec for 20s test = ~1000 events. At ~4 bytes/event average, 4 KB would suffice. 16 KB provides ample headroom

## Comparison with Alternatives

| Feature | Tonbandgeraet | Tracealyzer | Custom JSON |
|---------|--------------|-------------|-------------|
| License | MIT + GPL3 | Commercial (free viewer) | N/A |
| Embedded files | 3 | ~15 | ~1 |
| RAM | 16-32 KB | 32 KB+ | 4-8 KB |
| FreeRTOS hooks | Native | Native | Manual |
| Output format | Perfetto protobuf | .psf (proprietary) | Chrome JSON |
| Viewer | ui.perfetto.dev (free) | Percepio View (free) | chrome://tracing |
| CI-friendly | Yes (CLI conversion) | Limited | Yes |
| Maturity | Early | Production | N/A |

## Recommendation

Tonbandgeraet is a good fit for nano-ros. The integration is lightweight (3 C files, 16 KB RAM), the Perfetto visualization is excellent, and the snapshot-then-dump pattern matches our QEMU semihosting workflow.

**Suggested approach**: Add as opt-in feature (`NROS_TRACE=1` build flag) in the FreeRTOS board crate. When enabled, trace hooks are active and the snapshot is dumped to `trace.bin` via semihosting after the test completes. The `tband-cli` Rust tool converts to Perfetto format.

**Not recommended for production firmware** due to the RAM overhead and early-stage format, but ideal for development and CI test analysis.
