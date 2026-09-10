---
id: 1225
title: "`sizeof(nros::Timer)` follows `NROS_CPP_STD` — 24 vs 32 — and so do
  `GuardCondition` and `ComponentNode`; the layout gate's TYPES list never
  measured them"
status: open
type: bug
area: api
related: [phase-427, phase-430, rfc-0089, 0135, 0460, 0196, 1204]
---

## Problem

A capability probe may gate a METHOD. It may never change `sizeof`. Three public
C++ types break that rule, measured on this tree (2026-09-08, hosted
`g++ -std=c++17`, the same `sizeof` probe
`scripts/check-cpp-capability-layout.py` uses):

| type | baseline | `-DNROS_CPP_STD=1` |
| --- | --- | --- |
| `nros::Timer` | 24 | **32** |
| `nros::GuardCondition` | 32 | **40** |
| `nros::ComponentNode` | 55784 | **55848** |

`ComponentNode` is not a third defect — it holds `Timer timers_[8]`
(`NROS_COMPONENT_MAX_TIMERS`), and 8 × 8 = 64 is exactly its delta. Fix `Timer`
and it follows.

The gate's derived subject list adds two more SPELLINGS of the same defect,
which the authored list could not see: `rclcpp::Timer` and `rclcpp::TimerBase`
are aliases of `nros::Timer`, so a ported file writing either depends on the
same moving layout. Five baseline lines, three types, one member:

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

`check-cpp-capability-layout` MEASURES the rule rather than grepping for it,
which is right, but its reach was an authored list:

```sh
# Types whose layout must not move. Add a type here when it gains members.
TYPES=("rclcpp::Node" "::nros::Node" "::nros::QoS")
```

Three of the API's types (five by the time this was filed), chosen when the gate
was written for the `timers_` defect. Every other public type had been
unmeasured since. This is the issue 0196 shape — a gate whose coverage is
narrower than the rule it enforces — and it is why the defect survived
phase-417, which is the phase that built the gate. Same class as the RMW parity
map, whose authored table read `("gap", "no vtable slot")` for 28 slots that had
moved.

## STATUS — the gate is FIXED (2026-09-09); the three types are not

### 1. The subject list is DERIVED

`scripts/check/cpp_capability_subjects.py` asks clang what
`packages/api/nros-cpp/include/nros/nros.hpp` exports in our four namespaces
(`nros`, `rclcpp`, `rclcpp_action`, `rclcpp_lifecycle` — imported from
`scripts/api-parity.py`'s `RCLCPP_NAMESPACES`, not re-spelled) and returns every
spelling that can legally appear inside a `sizeof`:

* **90 subjects**, against the 5 that were authored;
* classes, structs, enums and concrete typedefs measured bare;
* **class templates at a DERIVED instantiation** — every parameter filled, a
  type parameter with `int` and a non-type parameter with `4`. All 27 templates
  instantiate that way, message- and service-parameterised ones included
  (`Publisher<int>` is 824 bytes). Defaults are filled too rather than left
  alone, because `Future<T, Cap = rx_buffer_capacity<T>::value>` computes its
  default FROM the first argument and `int::SERIALIZED_SIZE_MAX` does not exist;
* a template template parameter (none today) is REPORTED as unmeasurable rather
  than dropped, so the derivation cannot narrow silently;
* a `FLOOR` tripwire asserts the derived list still contains the five names the
  authored list had. A derived list SMALLER than the one it replaced is the
  narrowing this whole change is about.

Clang's AST rather than a text scan, following `scripts/api_parity/extract_cxx.py`
(and reusing its `dump_ast`): a regex cannot tell a class from a forward
declaration, a nested type from a top-level one, a defaulted template parameter
from a required one, or an alias template (not instantiable) from a concrete
typedef — and every one of those decides whether a name can appear in a `sizeof`.

### 2. Two things the widening also fixed

* **The gate was measuring a TU that does not compile.** It included
  `-Itarget/nros-{c,cpp}-generated`, dirs a pristine tree does not have, so its
  probe TU had **149 errors** and printed a `sizeof` anyway — GCC keeps going
  past `#error`. It now passes `-DNROS_PLATFORM_NUTTX` for the COMMITTED sizes
  header, the same choice `check-api-parity` makes, and the derivation refuses
  to run at all on a parse error, so that state cannot recur.
* **The gate gated nothing.** It ran inside `check-cpp`, which is
  `build-serial`, which no merge-gating event reaches. It is its own fast-lane
  gate now (`just check cpp-capability-layout`), which it can afford because it
  got FASTER while getting 18× wider: **9.4 s** for 90 subjects against **65 s**
  for 5, since all subjects share one compile per arm (the probe template is
  named per subject, so the index is in the diagnostic beside the size) and the
  nine arms run concurrently. In the `check-fast` fan-out at -P80 it is outside
  the ten slowest gates; the lane's wall is `check-api-parity` at 64 s.

### 3. The three violators are BASELINED, not fixed

`.config/cpp-capability-layout-baseline.txt`, a ratchet that may only shrink,
in the shape `.config/cpp-freestanding-includes-baseline.txt` already uses:

```
diverges ::nros::ComponentNode
diverges ::nros::GuardCondition
diverges ::nros::Timer
diverges ::rclcpp::Timer
diverges ::rclcpp::TimerBase
hosted-only ::rclcpp::NodeOptions
hosted-only ::rclcpp::Rate
hosted-only ::rclcpp::WallRate
```

`packages/api/nros-cpp/**` is owned by PR #755, in flight, so the wider gate
lands with the debt it can finally see recorded rather than editing headers
underneath it. Every other subject is protected from today.

The three `hosted-only` lines are the other verdict the widening needed:
`rclcpp::Rate` / `WallRate` sit inside `#ifdef NROS_CPP_HAS_STD_CHRONO`
(nros.hpp:936) and `rclcpp::NodeOptions` inside
`#if defined(NROS_CPP_HAS_STD_STRING) && defined(NROS_CPP_HAS_STD_VECTOR)`
(options.hpp:217), so they do not EXIST in the freestanding arm. Absence is not
a divergence — no freestanding TU can name the type, so no two TUs can disagree
about its shape — but it is DECLARED rather than inferred, because "the type was
supposed to exist there" is a real defect wearing the same clothes. That is
issue 1204's lesson kept.

### 4. Negative controls, on the normal path

Six, every run:

1. a capability-gated MEMBER changes `sizeof` (synthetic TU);
2. a capability-gated METHOD does not (the shape the rule permits);
3. a gated member injected into a THROWAWAY COPY of `nros/node.hpp` fails the
   real pipeline, and the unmutated copy passes first so a broken copy cannot
   be mistaken for a caught mutation (issue 1204's case);
4. **a `diverges` entry whose subject no longer diverges FAILS** — the direction
   #755's wave will hit, and one no measurement of today's tree can produce. A
   ratchet that tolerates stale entries has stopped ratcheting: the next real
   divergence in that subject would be absorbed by the dead line;
5. a baseline entry naming a subject the derivation no longer produces fails;
6. an unbaselined divergence still fails the comparison the two ratchet arms
   wrap.

Demonstrated by hand as well, on the tracked tree: an
`#ifdef NROS_CPP_HAS_SHARED_PTR double` member planted in `qos.hpp` failed the
gate for **9 subjects** — `nros::QoS` and the eight `rclcpp::*QoS` profiles that
derive from it, of which the authored list named exactly one.

## What is LEFT — for PR #755's wave

Delete the five `diverges` lines in the commits that fix them. The gate is that
wave's acceptance rather than its paperwork: a stale line is a failure, so the
baseline empties itself as the fixes land.

**The decision is made, so it is not re-litigated at the keyboard: state hides
behind a pointer only when it EXCEEDS a pointer.**

* `rclcpp::Node`'s hosted block is far larger than a pointer, so phase-427 W1's
  indirection — one unconditional `void*` whose pointee begins with a
  `void (*destroy)(void*)` — is right there, and it is already landed.
* `nros::Timer` and `nros::GuardCondition` differ by **exactly 8 bytes**, which
  IS a pointer. So they take the member **UNCONDITIONALLY in both
  configurations**: the same 8 bytes a freestanding target would have paid for
  the indirection, one stable layout, no one-definition hazard, and no
  dereference on the hot path. `ComponentNode` follows `Timer` at +64 and needs
  no change of its own.

The alternative the original filing wanted priced — making the closure
caller-owned, the shape `rclcpp::detail::WallTimer` uses — is not needed to
reach a stable layout under this decision, and it is a bigger change to the
`std_compat.hpp` surface than an unconditional 8-byte member.

## Status 2026-09-11 — still open; the wave it was handed to had already merged

A triage of open issues whose fix commits had landed found this one. The GATE
half is done (`d93cc31e6`); the three types are **not**, and nothing is carrying
them:

* **The divergence is unchanged.** `check-cpp-capability-layout --report` on
  `origin/main` today: `::nros::Timer` 32 vs 24, `::nros::GuardCondition` 40 vs 32,
  `::nros::ComponentNode` 496 vs 432 (still exactly 8 × 8 — it shrank from 55784
  with phase-427, the delta did not), and `rclcpp::Timer` / `TimerBase` with
  `Timer`. `timer.hpp:160-167` and `guard_condition.hpp:122-123` still carry
  `closure_` under `#ifdef NROS_CPP_STD`.
* **The five `diverges` lines are still in the baseline**, and the gate is green —
  which, because its selftest proves a `diverges` entry on a subject that no
  longer diverges FAILS, is itself the measurement that all five still diverge.
* **"For PR #755's wave" cannot happen as written.** #755 merged at
  2026-09-08 18:25 UTC, 55 minutes BEFORE `d93cc31e6` / `6b1886104` handed it
  this work. Nothing open cites 1225.

What closes it is unchanged and fully decided above: make `closure_` an
UNCONDITIONAL member of `nros::Timer` and `nros::GuardCondition` (the
`std::unique_ptr<std::function<void()>>` needs a freestanding-safe spelling —
an opaque pointer plus destroyer, the `rclcpp::Node` W1 shape — since the
freestanding arm has no `<functional>`), then delete the five `diverges` lines
in the same commit. `ComponentNode` follows. The gate is the acceptance: a
stale line fails.

## Repro

```sh
python3 scripts/check-cpp-capability-layout.py --report   # every subject, all arms
python3 scripts/check/cpp_capability_subjects.py          # the derived list
just check cpp-capability-layout                          # the gate
```
