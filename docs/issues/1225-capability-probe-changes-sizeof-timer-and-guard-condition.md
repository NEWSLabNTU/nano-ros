---
id: 1225
title: "`sizeof(nros::Timer)` follows `NROS_CPP_STD` — 24 vs 32 — and so do
  `GuardCondition` and `ComponentNode`; the layout gate's TYPES list never
  measured them"
status: open
type: bug
area: api
related: [phase-427, phase-430, rfc-0089, 0135, 0460]
---

## Problem

A capability probe may gate a METHOD. It may never change `sizeof`. Three public
C++ types break that rule, measured on this tree (2026-09-08, hosted
`g++ -std=c++17`, the same `ShowSize<sizeof(T)>` probe
`scripts/check-cpp-capability-layout.sh` uses):

| type | baseline | `-DNROS_CPP_STD=1` |
| --- | --- | --- |
| `nros::Timer` | 24 | **32** |
| `nros::GuardCondition` | 32 | **40** |
| `nros::ComponentNode` | 55784 | **55848** |

`ComponentNode` is not a third defect — it holds `Timer timers_[8]`
(`NROS_COMPONENT_MAX_TIMERS`), and 8 × 8 = 64 is exactly its delta. Fix `Timer`
and it follows.

The member in each case is the same one, and it is honest about what it is:

```cpp
// timer.hpp, and its twin in guard_condition.hpp
#ifdef NROS_CPP_STD
    /// Owns the heap-allocated `std::function<void()>` closure (if any).
    std::unique_ptr<std::function<void()>> closure_;
#endif
```

It is written by `attach_std_closure`, which `std_compat.hpp` calls from its
four `NROS_CPP_STD` convenience wrappers, and by nothing else.

## Why this is the shipped scar, not a hypothetical

This is issue 0135 / 0460's class, and phase-417 already paid for it once with
`rclcpp::Node::timers_` (3752 vs 3776 bytes). **Two TUs of one image disagreeing
about a capability is a SUPPORTED state here, not a misconfiguration:**

* `examples/px4/cpp/bridge/src/modules/nros_uorb_bridge/CMakeLists.txt:123` sets
  `-DNROS_CPP_STD=1` on ONE module of a larger image, deliberately;
* `zephyr/cmake/nros_rmw_cyclonedds.cmake` adds the `cxx-compat` include dir for
  some targets only, so `__has_include` can answer differently for two TUs of
  one build.

So a `nros::Timer` constructed in one such TU and read in another is written
through one layout and read through the other. Silently. That is precisely what
the gate exists to make impossible.

Not currently reachable in the px4 bridge as far as a source read goes — that
module declares no `nros::Timer` — but "no caller today" is what the phase-417
scar also looked like until a module gained one, and the rule is deliberately
about the LAYOUT rather than about whether anyone has tripped it.

## Why nothing caught it

`scripts/check-cpp-capability-layout.sh` MEASURES the rule rather than grepping
for it, which is right, but its reach is an authored list:

```sh
# Types whose layout must not move. Add a type here when it gains members.
TYPES=("rclcpp::Node" "::nros::Node" "::nros::QoS")
```

Three of the API's types, chosen when the gate was written for the `timers_`
defect. Every other public type has been unmeasured since. This is the issue
0196 shape — a gate whose coverage is narrower than the rule it enforces — and
it is why the defect survived phase-417, which is the phase that built the gate.

Found by sweeping the rest of the umbrella's types with the same probe while
landing phase-427; `nros::Node` (200/200), `QoS`, `Executor`, `Clock`, `Time`,
`Duration`, `CallbackGroup`, `NodeBuilder`, `LifecycleNode` and
`rclcpp::NodeOptions` are all clean.

## Fix — and it needs a decision, which is why this is filed rather than done

The mechanical fix is `rclcpp::Node`'s (phase-427 W1): one UNCONDITIONAL `void*`
whose pointee begins with a `void (*destroy)(void*)`, so the destructor is
byte-identical in every configuration and never names the hosted type. Applied
here it costs a FREESTANDING target **+8 bytes per `Timer`** and per
`GuardCondition` — and +64 on every `ComponentNode`, which is a fixed-arena
firmware type. That is a real number on the targets this project exists for, so
it is a decision, not a refactor.

The alternative worth pricing first: **make the closure caller-owned.**
`attach_std_closure` is `@internal` and reached only from `std_compat.hpp`'s
four wrappers, so the cell could live beside the entity in the caller's storage
— the shape `rclcpp::detail::WallTimer` already uses for the ported path
(`std::function` + `nros::Timer` in one heap cell the node keeps alive), and the
shape that let phase-430 W7 delete `TimerBase` without the timer growing at all.
If that works, freestanding pays ZERO and the hosted path pays what it already
pays.

Whichever lands, the gate's `TYPES` list must grow with it, or the next type to
gain a gated member repeats this. Better than growing the list by hand: derive
it, or at minimum add every type the umbrella declares.

## Repro

`tmp/sizesweep.sh` in the phase-427 branch is the throwaway that found it; the
durable form is the gate itself with a wider list. By hand:

```sh
cargo build -p nros-c -p nros-cpp --no-default-features \
    --features "std,rmw-cffi,platform-posix,ros-humble"
printf '#include <nros/nros.hpp>\ntemplate <int N> struct ShowSize;\nShowSize<static_cast<int>(sizeof(::nros::Timer))> probe;\n' > /tmp/s.cpp
c++ -fsyntax-only -std=c++17 -Itarget/nros-cpp-generated -Itarget/nros-c-generated \
    -Ipackages/api/nros-cpp/include -Ipackages/api/nros-c/include \
    -Ipackages/platform/nros-platform-api/include /tmp/s.cpp 2>&1 | grep ShowSize
# ...and again with -DNROS_CPP_STD=1
```
