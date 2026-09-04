# port-metric

Times the `nros_platform_*` ABI primitives, so ports can be compared and
regressions noticed.

nano-ros supports Zephyr, FreeRTOS, ThreadX, NuttX, POSIX, ESP-IDF and several
bare-metal boards. Nothing said what a port cost. The other benches here each
measure the middleware on one target — `wcet-cycles-qemu` times publish and
serialize through the DWT counter, `wake-latency-cortex-m3` times a wake — so
none of them compares ports, because none measures the surface the ports
actually implement.

## What it measures

EEMBC's Thread-Metric suite, reduced to the primitives this ABI exposes:

| Thread-Metric        | here                |
|----------------------|---------------------|
| memory alloc/dealloc | `alloc+free`        |
| cooperative switch   | `yield`             |
| semaphore processing | `mutex lock+unlock` |

Absolute numbers are board-specific and not the point. The per-port table and
the regression signal are.

## Running

```console
$ cargo run --release
nros port-metric — platform ABI primitives
port: posix
budget: 200000 us per test

test                     iterations       per second
----------------------------------------------------
alloc+free                 43662080        218310400
yield                       2738496         13692206
mutex lock+unlock          50379968        251899840
```

`NROS_BENCH_BUDGET_US` sets the per-test budget (default 200 ms, so the suite
is under a second). `NROS_BENCH_PORT` labels the row.

## Adding a target

`src/bench.rs` has no target, allocator or std dependency, and takes its clock
as a parameter. A board runner supplies an entry point and a clock and calls
`bench::all(..)`; `src/main.rs` is the host version of exactly that. Depend on
the port's platform crate instead of `nros-platform-cffi[posix-c-port]`.

The clock is a parameter rather than something `bench.rs` reaches for because
`nros_platform_time_now_ns` returns 0 on ports with no RTC, and a benchmark
that silently divided by that would report nonsense.

## Reading the output

A time budget, not an iteration count, so a slow port finishes in the same
wall time as a fast one. The clock is read once per 64 iterations, so what is
timed is the primitive rather than the clock.

An unsupported primitive is printed as `unsupported` rather than omitted: a
missing row reads as "not measured yet", which is a different claim from "this
port does not provide it".

Uncontended for the mutex, deliberately. A contended number measures the
scheduler's wake path, which `wake-latency-cortex-m3` already covers; this is
the cost every guarded access pays whether or not anyone is waiting.
