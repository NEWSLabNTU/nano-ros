# phase-438 — the C++ std surface is an opt-in PORTING surface, not a discovered capability

**Status (2026-09-08). Opened.** The C++ half of phase-359's argument. Implements
RFC-0089's compile-or-conform rule by making the surface that rule needs an
explicit request rather than a property of the toolchain. Re-cuts phase-427 W1,
which currently works around this rather than fixing it.

## Why

phase-359 removed `std` from the Rust crates on the ground that it was not a
convenience layer over the platform but a SECOND implementation of one. The C++
side has the same shape and has never had the same campaign.

The difference is that the C++ split is not even chosen. Six macros gate the
std-flavoured API, and each is defined by this block, repeated **29 times across
13 headers**:

```cpp
#if defined(NROS_CPP_STD)
#include <memory>
#define NROS_CPP_HAS_SHARED_PTR 1
#elif defined(__has_include)          // <-- the problem
#if __has_include(<memory>)
#include <memory>
#define NROS_CPP_HAS_SHARED_PTR 1
#endif
#endif
```

`NROS_CPP_STD` is a consumer opt-in. The `#elif` arm makes the same macros
DISCOVERED from the include path. On any hosted compiler they are on whether the
consumer asked or not — and `rclcpp::Node` is guarded on the discovered ones
(`nros.hpp:447`), not the requested one.

**So the API shape a consumer gets is decided by what headers happen to exist.**
A native build gets the std-flavoured node because libstdc++ is reachable, not
because anyone wanted it.

### The axis is misnamed, and the measurement says so

`NROS_CPP_HAS_SHARED_PTR` asks *"can this toolchain reach `<memory>`"*. The
question the codebase actually needs answered is *"does this consumer want the
surface that lets an upstream rclcpp file compile unmodified"*. Two different
questions, one name — the same collapse CLAUDE.md records for
`native`/`posix`/`linux`.

Measured, nothing but ported code wants the std surface:

| API form | users in the tree |
| --- | --- |
| hosted, `shared_ptr`-returning `create_*` | **1** — a compile test |
| freestanding, out-ref `create_*` | **27** call sites in `examples/` alone |

Every consumer of the hosted `rclcpp::Node` class is a ported-code template
(`cpp-port-minimal-publisher`, `rclcpp-compat-smoke`, `topic-state-monitor-port`,
and two workspace consumers — all five `compile_check_fixture` entries whose job
is to prove upstream files compile), a compile test under
`packages/api/nros-cpp/tests/compile/`, or the `diagnostic_updater` compat shim.
**No application code uses it, on any platform.**

That is the whole finding. The split is not host versus embedded. It is *ported
rclcpp code* versus *code written for nano-ros*, and every real nano-ros
program — native included — is on the freestanding side already.

### What it costs today

* **`rclcpp::Node` does not exist on a freestanding target**, because its guard
  is the discovered macros. That single guard is why phase-427's node merge is
  hard: the type that is supposed to become the one node type is absent from
  half the targets it must serve.
* **The layout hazard has something to bite on.** Hosted-only MEMBERS
  (`owned_entities_`, the `enable_shared_from_this` base) exist because the
  hosted API is a different shape of the same class rather than additive
  methods over a fixed one. Issues 0135 and 0460 are this class, and px4 sets
  `-DNROS_CPP_STD` on one module of a larger image deliberately, so the
  disagreement is reachable.
* **29 hand-rolled copies of one idiom.** Not a shared helper — a repeated
  block, which is the "second spelling rather than one helper" antipattern
  CLAUDE.md records for the Zephyr unset-variable guard (#282 → #326).

## Work items

* **W1 — one detection site.** Replace the 29 blocks with a single
  `nros/std_detect.hpp` the others include. No behaviour change; this is the
  refactor that makes W2 a one-line edit instead of a 29-site sweep.
  *Acceptance:* `sizeof` of every type in `check-cpp-capability-layout`'s
  `TYPES` is unchanged, hosted and freestanding; the per-header parse loop and
  both `-nostdinc++` lanes are unchanged; any site whose condition genuinely
  differed from the others is recorded rather than silently normalised.

* **W2 — delete the `#elif __has_include` arm.** The std surface becomes
  reachable only through `NROS_CPP_STD`. Every in-tree build path that wants it
  says so.
  *Acceptance:* on a hosted compiler with `NROS_CPP_STD` unset, the header set
  parses and the freestanding API is complete; the five porting templates and
  the compat shim build with the flag set; `just check cpp` green.

* **W3 — `rclcpp::Node` off the hosted-only list.** With the guard no longer
  discovered, the class exists on every target and comes off
  `check-cpp-capability-layout`'s `hosted_only_reason()` exemption. The gate's
  two-way ratchet (phase-427 W0, issue 1204) fails if it is left on.
  *Acceptance:* the freestanding arm measures `rclcpp::Node`; deleting the
  exemption is required, not optional, and the gate says so.

* **W4 — the hosted half becomes additive.** Hosted state moves behind one
  unconditional `void* hosted_`; the std-flavoured `create_*` become overloads
  rather than a second shape of the class. **This IS phase-427 W1**, and it
  belongs here because it is a consequence of W2 rather than of the node merge.
  *Acceptance:* `sizeof(rclcpp::Node)` is identical with and without
  `NROS_CPP_STD` — measured on the freestanding arm, which is the only place
  the macros are genuinely off.

* **W5 — say which surface a consumer is on.** The book and
  `docs/reference/c-api-cmake.md` document `NROS_CPP_STD` as the porting
  surface, and the CMake verb that turns it on. A consumer porting an rclcpp
  file asks for it; a consumer writing for nano-ros never does.
  *Acceptance:* a reader can answer "do I need this flag" from the book without
  reading a header.

## Not in scope

* Deleting the std surface. It is what makes a ported file compile, which is
  RFC-0089's whole goal. This phase makes it REQUESTED, not absent.
* `__STDC_HOSTED__`-gated sites (`result.hpp:17`, `component_node.hpp:116`, …).
  They answer a different question — "is there a libc to print with" — and
  issue 0112 records why `__has_include` was preferred for the C++ ones. W1
  enumerates them; changing them is separate work.
* The Rust `std` campaign, which is phase-359 and nearly done.

## Ordering

Before phase-426 and phase-427. W2 changes what "delete both C++ parameter
stores" is deleting from, and W4 is phase-427 W1 relocated. Doing it after would
mean building the node merge on a layout whose conditionality is the thing being
removed.

## Risk

W2 is a breaking change for any OUT-OF-TREE consumer that relies on the
discovered macros — they get the freestanding API where they used to get the std
one, and the failure is a compile error naming a missing overload, which is the
loud direction. It needs a changelog entry and a line in the book, not a
deprecation cycle: there is no way to warn on a macro that stops being defined.
