# Phase 64 — Embedded Transport Tuning Guide

**Goal**: Document zpico transport tuning for embedded deployments and benchmark
memory usage per entity. Motivated by lessons from ARM's actuation_porting project.

**Status**: Not Started

**Priority**: Medium

**Depends on**: None (independent of other phases)

## Overview

ARM's [actuation_porting](https://github.com/oguzkaganozt/actuation_porting) project — which
ports Autoware's trajectory follower to Zephyr RTOS with CycloneDDS — has well-tuned transport
buffer sizes (`config.hpp`: 8KB receive, 2KB chunks, 1400B max message) for embedded targets.
nano-ros has equivalent zpico compile-time constants (`ZPICO_*` env vars) but they are
undocumented beyond the reference table in `docs/reference/environment-variables.md`.

A tuning guide with recommended profiles and concrete memory numbers would help users size
their deployments without trial and error.

### What this phase does NOT cover

- **Parameter storage API** — already implemented in `nros-params` (`ParameterServer` with
  `declare()`, `get()`, `set()`, typed accessors, descriptors, range constraints, read-only
  enforcement). Capacity configurable via `NROS_MAX_PARAMETERS` env var (default 32).
- **Parameter services** — already implemented in `nros-node/src/parameter_services.rs`
  (full ROS 2 `~/get_parameters`, `~/set_parameters`, etc.), gated by `param-services` feature.
- **Dynamic reconfigure / parameter callbacks** — intentionally omitted. The existing model
  (read-only for safety-critical params, mutable via `ros2 param set` for tuning) is sufficient
  for embedded use. Callbacks add unbounded execution in the parameter path, which conflicts
  with real-time guarantees.

## Work Items

- [x] 64.1 — Embedded transport tuning guide
- [ ] 64.2 — Benchmark transport memory usage

### 64.1 — Embedded Transport Tuning Guide

Create a guide documenting all `ZPICO_*` environment variables, their defaults, and recommended
values for different deployment scenarios.

**Content outline:**

1. **Buffer sizes** — `ZPICO_FRAG_MAX_SIZE`, `ZPICO_BATCH_UNICAST_SIZE`,
   `ZPICO_BATCH_MULTICAST_SIZE`, `ZPICO_SUBSCRIBER_BUFFER_SIZE`, `ZPICO_SERVICE_BUFFER_SIZE`,
   `ZPICO_GET_REPLY_BUF_SIZE`
2. **Entity limits** — `ZPICO_MAX_PUBLISHERS`, `ZPICO_MAX_SUBSCRIBERS`,
   `ZPICO_MAX_QUERYABLES`, `ZPICO_MAX_LIVELINESS`
3. **Network** — smoltcp socket counts, buffer sizes, timeouts; MTU considerations;
   fragmentation behavior; maximum message sizes
4. **Discovery** — scouting timeout, multicast vs unicast, locator configuration
5. **Memory budget** — stack vs heap allocation, per-publisher/subscriber overhead
6. **Recommended configurations** — profiles for different targets:
   - Minimal (Cortex-M4, 256KB RAM): 4 pub, 4 sub, small buffers
   - Standard (Cortex-M7, 1MB RAM): 16 pub, 16 sub, moderate buffers
   - Large (Cortex-R52, 4MB+ RAM): 48 pub, 48 sub, large buffers (sentinel profile)
7. **Comparison with CycloneDDS** — contrast ARM project's `config.hpp` settings

**Reference**: ARM project's `config.hpp`
(`external/actuation_porting/actuation_module/include/common/dds/config.hpp`).

**Status**: Complete

**Files**:
- `docs/guides/embedded-tuning.md`

### 64.2 — Benchmark Transport Memory Usage

Profile zpico memory allocation to provide concrete numbers for the tuning guide.

- Measure per-publisher and per-subscriber memory overhead
- Measure session baseline memory (no entities)
- Measure message serialization buffer sizes for common Autoware message types
- Compare against CycloneDDS numbers from ARM project (1MB heap total)
- Document results in `docs/guides/embedded-tuning.md`

**Status**: Not Started

## Acceptance Criteria

- [x] Embedded tuning guide with zpico constants and recommended configurations
- [ ] Memory benchmark numbers for at least 3 message types
- [ ] `just quality` passes
- [ ] Existing tests unaffected

## References

- ARM DDS config: `external/actuation_porting/actuation_module/include/common/dds/config.hpp`
- ARM Zephyr config: `external/actuation_porting/actuation_module/prj_actuation.conf` (1MB heap)
- Existing env var reference: `docs/reference/environment-variables.md`
- Existing parameter API: `packages/core/nros-params/` (`ParameterServer`, `NROS_MAX_PARAMETERS`)
- Existing parameter services: `packages/core/nros-node/src/parameter_services.rs`
- zpico source: `packages/zpico/zpico-sys/zenoh-pico/`
