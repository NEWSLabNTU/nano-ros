---
id: 1204
title: "`check-cpp-capability-layout` forces capability macros ON against a
  baseline where they are already on, so it cannot see the layout divergence it
  exists to catch — proven by mutation"
status: open
type: bug
area: [cpp, ci]
related: [0135, 0460, phase-417, phase-427]
---

## What

`scripts/check-cpp-capability-layout.py` enforces "a capability probe may gate a
METHOD; it may never change `sizeof`". It compiles a probe TU once per
capability macro, forcing that macro **on**, and compares `sizeof` against a
baseline.

The baseline is a plain hosted `c++ -std=c++17` compile. Every
`NROS_CPP_HAS_*` macro is self-defined by the headers under
`__has_include(<memory>)` and friends (`client.hpp:23-32`, and the same block in
`publisher.hpp`, `service.hpp`, `subscription.hpp`, `polling_subscription.hpp`),
which always succeeds on a hosted compiler. So forcing one on is a strict no-op,
and the gate compares the baseline against itself seven times.

The script's own header comment anticipates this — *"forcing a macro that is
already on in the baseline is a no-op, so a version of this gate could look green
and detect nothing"* — and mitigates it with a `selftest()` that uses a
**synthetic** struct (`Conditional` / `Invariant`, lines 77-118) in a standalone
TU with no nros headers. The synthetic struct's macro genuinely starts
undefined, so the selftest passes honestly while saying nothing about the three
real types.

## Measured

All seven forced measurements are identical to the baseline:

```
rclcpp::Node baseline = 3752
  -DNROS_CPP_STD=1            -> 3752
  -DNROS_CPP_HAS_SHARED_PTR=1 -> 3752
  -DNROS_CPP_HAS_STD_STRING=1 -> 3752
  -DNROS_CPP_HAS_STD_VECTOR=1 -> 3752
  -DNROS_CPP_HAS_STD_FUNCTION=1 -> 3752
  -DNROS_CPP_HAS_STD_CHRONO=1 -> 3752
  -DNROS_CPP_HAS_STD_SSTREAM=1 -> 3752
```

## Mutation test — CORRECTED 2026-09-07

**The first demonstration filed here was invalid, and the gate was right to pass
it.** It injected a member gated on `NROS_CPP_HAS_SHARED_PTR` into
`rclcpp::Node`. That mutation is tautological: `rclcpp::Node` is *itself* inside
`#if defined(NROS_CPP_HAS_SHARED_PTR) && defined(NROS_CPP_HAS_STD_STRING) && ...`
(`nros.hpp:447`), so the inner `#ifdef` is true wherever the type exists at all.
Measured with it applied, every configuration reports 3760 — a uniform shift, no
divergence, nothing for any gate to detect. Keeping the original claim would have
aimed the next reader at a defect that is not there.

The conclusion survives; the evidence had to be replaced. The honest
demonstration uses `::nros::Node`, which is NOT inside a capability guard, so a
gated member there really is present hosted and absent freestanding:

```cpp
#ifdef NROS_CPP_HAS_SHARED_PTR
    double __mutation_probe_member;
#endif
    nros_cpp_node_t handle_;
```

Old gate, on that mutation:

```
check-cpp-capability-layout --selftest: 2 case(s) OK (member diverges, method does not)
check-cpp-capability-layout: OK — 3 type(s) x 7 capability macro(s), no layout depends on a probe
OLD GATE EXIT=0
```

Green on a genuine layout divergence. The negative control passes on its
synthetic struct in the same run, which is what made the gate look trustworthy.

New gate, same mutation:

```
FAIL: sizeof(::nros::Node) differs hosted vs -nostdinc++ freestanding — 200 vs 192
NEW GATE EXIT=1
```

(Both mutations reverted; the tree is byte-identical.)

### What this means for `rclcpp::Node` specifically

`rclcpp::Node` cannot be covered by the freestanding arm today, because it does
not exist freestanding. Nor can a non-tautological mutation be constructed for
it: that needs a configuration with `<memory>` present and some other capability
header absent, and an include path can add a header, never hide one. Neither
shim offers such a configuration. So `rclcpp::Node` stays on the forced-macro arm
alone until phase-427 makes it freestanding — recorded as a limit in the script
rather than papered over.

## Why it matters now

phase-427 merges three node types into one `rclcpp::Node` that must exist on
freestanding targets. W1's acceptance is *"`check-cpp-capability-layout` passes
with the hosted members present, and `sizeof(rclcpp::Node)` is identical with and
without `-DNROS_CPP_STD`"*. Both halves of that are satisfiable by a broken
implementation, so the work item cannot currently be verified.

It is also why the gate is green today on a type that does not exist
freestanding at all: `rclcpp::Node` sits inside `#if defined(NROS_CPP_HAS_SHARED_PTR)
&& defined(NROS_CPP_HAS_STD_STRING) && ...` (`nros.hpp:447`). When a forced macro
makes the TU not compile the script does `continue` (`:132`), treating "the type
is absent in this configuration" as "not a layout question". For a type that is
supposed to exist everywhere, absence is the failure.

## Fix

The macros are only genuinely off in one configuration: `-nostdinc++` against a
shim without `<memory>`. The `cpp` lane already has two such configurations
(`just/check/lanes.just:708-747`, the ThreadX `cxx-compat` shim and the Zephyr
minimal libcpp). Measure there, rather than by forcing macros on:

1. Add a measurement arm that compiles the probe TU with the ThreadX shim flags
   and compares against the hosted baseline.
2. Replace the blanket `continue` on an unmeasurable type with a per-type policy:
   a type declared to exist on every target fails when it cannot be measured;
   one that is legitimately hosted-only is skipped by name, with the reason
   recorded beside it.
3. Extend the selftest to mutate one of the REAL types, not only the synthetic
   struct — the mutation above is the case it must catch.

## Note on the C library gap

The `-nostdinc++` lanes take C library headers from the host
(`just/check/lanes.just:704-705` states this). That does not affect the
measurement here, which is about C++ standard-library capability macros.
