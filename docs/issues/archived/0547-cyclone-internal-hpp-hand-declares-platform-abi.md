---
id: 547
title: "The Cyclone backend hand-declared the platform ABI, so RFC-0073's clock rename compiled fine and failed at link"
status: resolved
type: bug
severity: high
area: rmw, zephyr, build
related: [issue-0541, issue-0160, rfc-0054, rfc-0073, phase-352]
resolved_in: "issue-0547 (include nros/platform.h; delete the hand copies)"
---

## Symptom

The zephyr lane fails, so `build-test-fixtures lane=all` fails:

```
/usr/bin/ld: nros-rmw-cyclonedds/src/internal.hpp:63:
  undefined reference to `nros_platform_clock_ms'
```

It reads as a missing implementation. It is a stale DECLARATION.

## Cause

`internal.hpp` hand-declared the ABI it uses, once per platform:

```cpp
#elif defined(NROS_PLATFORM_ZEPHYR) || defined(__ZEPHYR__)
extern "C" {
uint64_t nros_platform_clock_ms(void);
void nros_platform_sleep_ms(size_t ms);
uint64_t nros_platform_random_u64(void);
}
```

RFC-0073 (phase-352, `bde6638ed`) then replaced the `clock_ms` / `clock_us` pair
with `clock_ns`, and `nros/platform.h` now carries `clock_ms` as a `static
inline` shim:

```c
static inline uint64_t nros_platform_clock_ms(void) {
    return nros_platform_clock_ns() / 1000000u;
}
```

A local re-declaration saying `extern` still COMPILES against that — nothing in
the TU is wrong until link time, when there is no such symbol to bind. All three
names were already declared in `platform.h` (`clock_ms` 151, `sleep_ms` 221,
`random_u64` 246), so not one of the hand copies was load-bearing. Their only
effect was to let this file disagree with the header.

RFC-0054's rule is that the C header IS the SSoT for this ABI, and CLAUDE.md
lists hand-mirrored FFI declarations as a recurring defect class (issue 0160,
the QoS / `callback_group` struct mirrors, three times). This is that class in
FUNCTION form. The struct version corrupts a field; the function version fails
at link, one layer later and less legibly.

## Third breakage from one rename

Each was only visible once the previous was cleared, because the zephyr lane
aborts on the first failure:

1. the cargo lane compiling vendored zenoh-pico at all — issue 0541;
2. the committed cbindgen header declaring the old symbol — fixed upstream,
   `5dc2fa869`;
3. this consumer.

`check-cbindgen-headers` passes throughout, because it asks whether the header
matches a fresh generation — not whether it agrees with the hand copies in other
files. A gate narrower than the property it protects (issue 0196's shape).

## Fix

Delete all three `extern "C"` blocks; include `nros/platform.h` in the branches
that use the ABI.

The include stays INSIDE the `#if` guards, and that part is load-bearing: the
hosted build compiles this backend WITHOUT `nros-platform-api/include` on its
path (hosted uses `<chrono>`/`<thread>` and never touches the platform ABI), so
an unguarded include fails with `nros/platform.h: No such file or directory`.
Measured — the first cut of this fix hoisted the include and broke
`check-rmw-cyclonedds`. So the guards select the implementation AND gate the
header backing it; what was never justified is declaring the ABI by hand inside
them.

## Verified

```
just check rmw-cyclonedds                          -> 17/17, exit 0   (hosted)
NROS_ZEPHYR_FIXTURE_FILTER='build-c-listener-cyclonedds' \
  just zephyr build-fixtures                       -> exit 0, 0 undefined refs
```

Sweep for siblings — every C/C++ consumer of `nros_platform_{clock_*,sleep_ms,
random_u64}` outside the API package, counting hand declarations against the
include:

```
git grep -ln "nros_platform_clock_\|nros_platform_sleep_ms\|nros_platform_random_u64" \
  -- '*.hpp' '*.cpp' | grep -v nros-platform-api
```

One file, now `handdecl=0`. The other grep hits are platform PORTS defining the
symbols, which is what they are for.
