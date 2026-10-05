# phase-438 — the C++ std surface is an opt-in PORTING surface, not a discovered capability

**Status (2026-09-09). W0-W5 LANDED — issue 1187 is closed; the phase is complete.** The C++ half of phase-359's
argument. Implements RFC-0089's compile-or-conform rule by making the surface that
rule needs an explicit request rather than a property of the toolchain. W4 was
written as a re-cut of phase-427 W1; the node merge landed that implementation
first, so what this phase contributes there is the MEASUREMENT that its
acceptance always named and could not run before W2.

**It is also the class fix issue 1187 asks for**, which is the part that changes
its priority. This was written as tidiness. The measurement says the construct it
removes is a live breakage on the pinned embedded toolchain, and that the gate
meant to catch that breakage structurally cannot see it (issue 1223).

## Why

phase-359 removed `std` from the Rust crates on the ground that it was not a
convenience layer over the platform but a SECOND implementation of one. The C++
side has the same shape and has never had the same campaign.

The difference is that the C++ split is not even chosen. Six macros gate the
std-flavoured API, and each is defined by this block, repeated **15 times across
11 headers** (`client`, `fixed_string`, `heap_string`, `log` ×2, `nros`, `options`
×2, `polling_subscription`, `publisher`, `service`, `subscription` ×2, `timer` ×2):

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
(`nros.hpp:454`), not the requested one.

**So the API shape a consumer gets is decided by what headers happen to exist.**
A native build gets the std-flavoured node because libstdc++ is reachable, not
because anyone wanted it.

### The `#elif` arm is not a benign over-broadening — it breaks the embedded build

Measured on the FreeRTOS lane's own pinned compiler
(`~/.nros/sdk/arm-none-eabi-gcc/13.2-nros1`, `-std=c++14 -ffreestanding
-fno-exceptions -fno-rtti`), every header compiled standalone:

```
TRACKED  (arm-none-eabi 13.2, -ffreestanding): pass=26 fail=19
STRIPPED (the #elif arms removed)            : pass=45 fail=0
```

The 19 failures are `action_client action_server client component component_node
fixed_string heap_string log node nros options polling_action_client
polling_action_server polling_subscription publisher qos service subscription
timer`, and the diagnostic is the same every time:

```
In file included from .../c++/13.2.1/string:38,
                 from packages/api/nros-cpp/include/nros/log.hpp:115,   <-- the #elif arm
                 from packages/api/nros-cpp/include/nros/qos.hpp:24,
                 from packages/api/nros-cpp/include/nros/node.hpp:30,
.../bits/requires_hosted.h:34:4: error: #error "This header is not available in freestanding mode."
```

Deleting the arm **fixes 19 headers**. On the hosted loop the same strip is a
no-op — 46 pass before and after — so this phase's central edit is not a
trade-off between the two lanes. It is a fix on one and neutral on the other.

That is issue 1187, whose "Fix direction" asks for exactly this and asks for it
**for the class**: "One shared spelling, not a 14th."

### Neither probe is right on both embedded lanes; only the request is

The reason `__has_include` cannot be repaired in place, measured directly:

| | `__STDC_HOSTED__` | `__has_include(<string>)` | `#include <string>` |
| --- | --- | --- | --- |
| arm-none-eabi 13.2, `-ffreestanding` (FreeRTOS lane) | **0** | **TRUE** | hard `#error` |
| Zephyr `-nostdinc++`, minimal libcpp (issue 0112's lane) | 1 | FALSE | absent |
| hosted g++ 12.3 | 1 | TRUE | works |

Each probe is right on exactly one embedded lane and wrong on the other. On the
FreeRTOS lane `__STDC_HOSTED__` is the correct probe and `__has_include` is the
wrong one — **the exact reverse of what eleven header comments state.** Only
`NROS_CPP_STD` is right on both *as a single probe*, because it is not a probe.

**CORRECTION (2026-09-09, measured).** This section then concluded that ANDing
the two does not work either. That does not follow, and the build disproves it.
Both wrong answers above are of the same sign — *include it when you must not* —
so the CONJUNCTION fails closed: the FreeRTOS row dies on `__STDC_HOSTED__ == 0`,
the Zephyr row on `__has_include` FALSE, and only the hosted row has both. Issue
1240 shipped exactly that AND, and `fixtures-build.sh freertos cpp zenoh` is
green on this table's own toolchain with zero `requires_hosted` diagnostics. The
argument for the opt-in is the API-shape one above, not this one.

### Those eleven comments cite issue 0112 for something 0112 did not say

Eleven headers carry a comment reading, in substance, *"why the test is
`__has_include` rather than `__STDC_HOSTED__`, issue 0112"*. Issue 0112's
Resolution reads:

> Fix (`component_node.hpp`): moved the `<string>` include into its own
> `#ifdef NROS_CPP_STD` block, so it follows its actual consumer.

0112 chose **`NROS_CPP_STD` over `__STDC_HOSTED__`** — the opt-in. It never chose
`__has_include`. The `#elif __has_include` arm arrived later, in `acf213871`
("feat(phase-417 stage 0+1+2b)"), and re-broadened the gate 0112 had narrowed.

So this phase is not a departure from 0112. It is 0112's own fix, restored, and
the eleven comments are wrong twice over — about what 0112 decided, and about
which probe is correct on the lane that is actually failing. 0112's *finding*
still holds: hostedness does not imply header availability. What changed is that
`__has_include` stopped being a fix for it, because presence stopped implying
usability.

### The measurement: nothing but ported code wants the std surface

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

Confirmed against the examples by rebuilding each TU's real compile line from its
own build dir's `build.ninja` and re-running it against a stripped include tree:
**59 TUs, 0 regressions.** Not one example names `rclcpp::Node`; the whole
`rclcpp::` surface they touch (`shutdown`, `Result`, `Publisher`, `ok`,
`Subscription`, `Client`, `Service`, `ClientBase`) is declared outside the guard,
and their only library includes are `<cstdio> <cstdlib> <cstring> <cstdint>
<cstddef> <new>`.

That is the whole finding. The split is not host versus embedded. It is *ported
rclcpp code* versus *code written for nano-ros*, and every real nano-ros
program — native included — is on the freestanding side already.

### What it costs today

* ~~**The FreeRTOS C++ build does not compile at all** (issue 1187)~~ —
  **no longer true as of 2026-09-09.** Issue 1240's both-probes gate
  (`NROS_CPP_STD || (__STDC_HOSTED__ && __has_include(<hdr>))`, `c9c861360`)
  fixed it, MEASURED: `fixtures-build.sh freertos cpp zenoh` is green on the
  pinned arm-none-eabi 13.2 and all six `cpp_*` images link. Issue 1146's C++
  app-task stack figure is now takeable — `freertos_app_config.c.in`'s 512 KiB
  still serves C and C++ from one number measured only on the C half, but
  nothing structural is in the way any more.
* **The gate that should have caught it reports green** (issue 1223).
  `check-cpp-freestanding-includes` pushes a guard frame on `#if
  defined(NROS_CPP_STD)` and pops only on `#endif`; `#elif` and `#else` match
  neither rule, so the frame survives into the alternative arm and all 14 `#elif`
  arms are scored as guarded. Issue 1023 already fixed the `#endif` half of this
  same defect and left the other two spellings unmodelled.
* **`rclcpp::Node` does not exist on a freestanding target.** That is why
  phase-427's node merge is hard: the type that is supposed to become the one
  node type is absent from half the targets it must serve. (The guard is not the
  only obstacle — see W3.)
* **The layout hazard has something to bite on.** Hosted-only MEMBERS
  (`owned_entities_`, the `enable_shared_from_this` base) exist because the
  hosted API is a different shape of the same class rather than additive methods
  over a fixed one. Issues 0135 and 0460 are this class, and px4 sets
  `-DNROS_CPP_STD` on one module of a larger image deliberately, so the
  disagreement is reachable.
* **15 hand-rolled copies of one idiom.** Not a shared helper — a repeated block,
  which is the "second spelling rather than one helper" antipattern CLAUDE.md
  records for the Zephyr unset-variable guard (#282 → #326).

## Work items

* **W0 — teach `check-cpp-freestanding-includes` about `#elif` and `#else`
  (issue 1223). LANDED.** On either, the current frame's condition no longer
  holds, so the frame is REPLACED rather than kept: `stack[sp]` becomes
  `"other"`, and an `#elif` that itself names `NROS_CPP_STD` becomes `"std"`.
  Replaced rather than popped, because at `strict=0` the Cyclone backend
  legitimately takes `<chrono>`/`<thread>` in the `#else` of an
  `NROS_PLATFORM_*` chain — a pop would make depth 0 there and break it.

  **Correction to this phase's first draft, which said the gate should go RED
  and W2 should turn it green.** That is not affordable: this gate is on the
  fast line, so it is on the `pre-push` hook, and a red one on `main` blocks
  every push in the repository by every contributor. This session had already
  spent a turn clearing exactly that state for `check-doc-commit-citations`.
  The intent survives in the affordable form — W0 lands with
  `.config/cpp-freestanding-includes-baseline.txt`, a shrink-only ratchet
  carrying the 14 sites the walker can finally see, and **W2 empties it**. A
  stale entry FAILS, so the file is still W2's acceptance rather than its
  paperwork; it just does not hold the tree hostage while W2 is written.

  Measured: exactly **14 violations across 10 headers** — the 14 `#elif` arms,
  with `nros.hpp` correctly absent, since its `STD_CHRONO` block has no `#elif`
  arm to be blind to.
  *Acceptance (met):* six selftest cases, of which three flip when the `#elif`
  rule is reverted — the negative control this gate had none of. Both ratchet
  directions mutation-tested: removing a baseline line reports the violation,
  and adding a paid-off one fails as stale.

* **W1 — one detection site.** Replace the 15 blocks with a single
  `nros/std_detect.hpp` the others include. Measured feasible: **14 of the 15
  normalise to exactly one block text**, with header name and macro name
  consistent at all three positions in every instance.

  **One block genuinely diverges and must NOT be normalised** —
  `nros.hpp:330`, `NROS_CPP_HAS_STD_CHRONO`:

  ```cpp
  #if defined(NROS_CPP_STD) || (defined(__STDC_HOSTED__) && __STDC_HOSTED__ && __has_include(<chrono>) && __has_include(<ratio>))
  ```

  Three differences, none cosmetic: no `#elif` arm at all, an `__STDC_HOSTED__`
  conjunct, and a second probe (`<ratio>`) used as a *prerequisite* rather than a
  proxy — `duration_cast` is defined via `std::ratio_divide`, and the safety
  island's toolchain ships `<chrono>` with `<ratio>` absent. The 85-line comment
  above it records three successive corrections, each measured. For this block W2
  reduces to dropping the `||` clause.

  Second thing W1 fixes, latent today: **`nros.hpp` USES `SHARED_PTR`,
  `STD_STRING`, `STD_VECTOR` and `STD_FUNCTION` and DEFINES none of them** — it
  depends on `publisher.hpp` / `options.hpp` / `subscription.hpp` having been
  included first. It is the only header in that state, and a shared
  `std_detect.hpp` removes the ordering dependency the hand-rolled copies
  preserve.

  *Acceptance:* `sizeof` of every subject `check-cpp-capability-layout` derives
  is unchanged, hosted and freestanding; the per-header parse loop and both
  `-nostdinc++` lanes are unchanged; the `STD_CHRONO` divergence is recorded in
  the new header rather than silently normalised.

* **W2 — delete the `#elif __has_include` arm.** The std surface becomes
  reachable only through `NROS_CPP_STD`. Every in-tree build path that wants it
  says so.

  Six places need the flag, enumerated by measurement rather than by grep:

  1. `just/check/lanes.just` — the compile-test invocations for
     `ros2_api_adoption.cpp` and `ros2_param_launch_seed.cpp`.
  2. `scripts/build/compile-check-fixtures.sh:636` — the `cpp_compat_snippets`
     arm (`rclcpp_node_options.cpp`, `spin_until_future_complete.cpp`), also
     reached by `cpp_api_drift.rs`, `platform_header_compile.rs` and
     `compile-check-signature.sh`.
  3. **`cmake/compat/NrosRclcppCompat.cmake`** — which sets **no compile
     definition at all** today. Every ported file reaching `<rclcpp/rclcpp.hpp>`
     subclasses `rclcpp::Node`, so this covers five `compile_check_fixture` rows:
     `cpp_port_minimal_publisher`, `cpp_port_rclcpp_compat_smoke`,
     `cpp_port_topic_state_monitor`, `local_msg_pkg`, `shadowing`.
  4. `cmake/compat/diagnostic-updater/.../diagnostic_updater.hpp` — covered by
     (3) if the shim target carries the flag.
  5. `scripts/check-cpp-capability-layout.py` — **measured to fail without it**,
     `rc=1`: *"could not measure sizeof(rclcpp::Node) in the baseline
     configuration"*. Its `HOSTED_FLAGS=(-std=c++17)` baseline TU needs the flag.
  6. `scripts/api-parity.py` — its `base` and `component` TUs. Note the file's
     stated justification for passing the flag on `compat` — *"that is the
     flavour the compat CMake path builds under"* — is **false today**, per (3);
     W2 makes it true.

  Only ONE shipping build path defines `NROS_CPP_STD` today:
  `examples/px4/cpp/bridge/.../CMakeLists.txt:123`.

  Measured regressions from the strip, hosted, before adding any flag: **6 TUs**,
  in three groups — (A) `rclcpp::Node` and its factories vanish
  (`ros2_api_adoption`, `ros2_api_adoption_stage2`, `ros2_one_dispatch_path`,
  `ros2_param_launch_seed`, `spin_until_future_complete`); (B)
  `rclcpp::NodeOptions` vanishes (`rclcpp_node_options`); (C)
  `FixedString`↔`std::string` interop vanishes (`ros2_api_adoption:100-106`, which
  moves with A). Every one is a *porting* TU by name and intent. A synthetic
  `class MyNode : public rclcpp::Node` probe confirms the flag fully restores
  today's behaviour: 3 errors stripped, 0 errors stripped + `-DNROS_CPP_STD`.

  Side-finding worth its own cleanup: **`ros2_api_adoption_stage2.cpp` and
  `ros2_one_dispatch_path.cpp` are compiled by no lane** — two of the four
  "regressions" are in files nobody runs. Eight more compile TUs are in the same
  state (`action_goal_uuid`, `executor_cancel`, `node_graph_forwarders`,
  `ros2_init_argv_refusal`, and four `ros2_refuse_*_probe`). That is the
  `check-required-features-reachable` class one directory over.

  *Acceptance:* `bash scripts/build/fixtures-build.sh freertos cpp zenoh` green
  with the arm-freertos toolchain — issue 1187's own acceptance, and a BUILD, not
  a gate. Plus: `check-cpp-freestanding-includes` (W0-fixed) green; the per-header
  loop on arm-none-eabi at `fail=0`; the five porting templates and the compat
  shim build with the flag; `just check cpp` green.

* **W3 — `rclcpp::Node` off the hosted-only list, and the honest reason it was
  there. LANDED.** The phase originally claimed the `nros.hpp:454` guard was all
  that kept the class off freestanding targets. **That is wrong, and the
  correction is the useful part.** With the guard replaced by `#if 1` and the
  macros off, the stripped tree failed at the class head itself:

  ```
  nros.hpp:546:49: error: expected template-name before '<' token
  ```

  — that is `class Node : public std::enable_shared_from_this<Node>`.
  `rclcpp::Node` was not merely *guarded by* std, it was *spelled in* std:
  inside the guarded block, 23 × `std::string`, 18 × `std::shared_ptr`, 9 ×
  `std::make_shared`, 4 × `std::vector`, 4 × `std::chrono`, plus the base class.
  Every factory returned `std::shared_ptr<...>` where the freestanding
  `nros::Node` takes an out-ref; every name parameter was `const std::string&`
  where `nros::Node` takes `const char*`; `Node::SharedPtr` — the alias every
  ported file names — *is* `std::shared_ptr<Node>`; and `rclcpp::NodeOptions`
  was a second, separate guard that a freestanding `Node` would need first.

  So W3's outcome was decided by whether the class reached layout invariance,
  and phase-427 W1-W3/W5 reached it first — the node merge, not this phase — so
  the exemption came OFF there, forced by the ratchet phase-427 W0 installed
  ("listed hosted-only but DOES measure freestanding (200)").
  `rclcpp::Node` carries no exemption now, and the ratchet is what keeps it
  that way. The registry itself has since moved out of the script: issue 1225
  replaced the authored `TYPES` list with a derived one (90 subjects, not 5) and
  the `hosted_only_reason()` function with
  `.config/cpp-capability-layout-baseline.txt`, which is not empty — it records
  the debt the wider gate could finally see, none of it `rclcpp::Node`.
  Two lessons are recorded in its place, both paid for:

  * **A reason string must name the CONSTRUCT, never a line.** The removed entry
    said "at nros.hpp:447", which was wrong when it was written — 447 was the
    preceding `} // namespace rclcpp` and the `#if` was at 454 — and the line
    then moved twice more inside this phase alone.
  * **A reason must say what is MISSING, not what is guarded.** "guarded by
    `NROS_CPP_HAS_SHARED_PTR` && …" describes the symptom, and a symptom-shaped
    reason invites the wrong fix (delete the guard), which is what this phase's
    first draft of W3 proposed before the compile measurement refuted it. The
    real reason was the ownership shape.

  What this phase adds is the THIRD arm, the one the gate's own `HOSTED_FLAGS`
  comment had asked for: hosted `-std=c++17` **without** `-DNROS_CPP_STD`. It is
  the arm that isolates the FLAG from the toolchain — the freestanding arm
  varies `-std`, `-nostdinc++` and the shim all at once, so on its own it cannot
  tell "the porting surface moved a layout" from "the two libc++ shims disagree
  about a member". It was not measurable before W2, because the macros were
  discovered from the include path and a hosted TU always had them.

  *Acceptance (met):* `rclcpp::Node` measures in all three arms, no exemption;
  and the arms have teeth on THIS type, mutation-tested — an
  `#ifdef NROS_CPP_HAS_SHARED_PTR`-gated `double` injected next to `clock_` in a
  throwaway copy of the include tree reports `200 without, 208 with` on the new
  arm and `208 vs 200` on the freestanding one. That is the exact mutation issue
  1204 measured as uncatchable, and the reason it was uncatchable — the whole
  class sat inside a guard naming that macro, so the `#ifdef` was tautological —
  is what the node merge removed.

* **W4 — the hosted half becomes additive. LANDED — as phase-427 W1-W3/W5, and
  what remained here is the MEASUREMENT.** This work item was written as "this
  IS phase-427 W1", relocated, on the reasoning that it is a consequence of W2
  rather than of the node merge. The node merge landed first and did the
  implementation; what phase-438 owed, and pays here, is the arm that proves it.

  `rclcpp::Node` was two different SHAPES of one class: with the capability
  macros on it had a base class, a `std::vector` member and a full set of
  `shared_ptr`-returning factories; with them off it did not exist at all. That
  was tolerable only while the macros were DISCOVERED from the include path,
  because then "hosted" and "std surface" moved together. W2 made the surface a
  per-TU request, and px4 sets `-DNROS_CPP_STD` on ONE module of a larger image
  deliberately, so two shapes of one class in one image became reachable —
  issues 0135 / 0460.

  What phase-427 changed, stated here because this phase's acceptance is about
  it:

  * **The layout is unconditional.** `rclcpp::Node` is `using Node =
    ::nros::Node`, an alias declared outside any capability guard, over a class
    declared the same way in `node.hpp`. The
    `std::enable_shared_from_this<Node>` base and the
    `std::vector<std::shared_ptr<void>>` of owned entities moved into a
    `detail::NodeHosted` box reached through one unconditional
    `detail::NodeHostedBase* hosted_`, allocated LAZILY — so a freestanding
    node, and a hosted node that only ever takes the out-ref `create_*` family,
    calls `operator new` never.
  * **The box carries its own deleter**, as a `void (*destroy)(void*)` on
    `NodeHostedBase` rather than a second pointer in the node. An `#ifdef`
    inside `~Node()` would have satisfied the `sizeof` rule and still given one
    inline symbol two bodies across TUs that are allowed to disagree; with the
    deleter stored, every `~Node()` in the image is the same lines and does the
    right thing for whichever arm CONSTRUCTED the node.
  * **The factories became overloads.** `create_publisher(out, "topic", qos)`
    and its siblings — the out-ref forms mirroring `nros::Node` — are declared
    always; the `shared_ptr`-returning and `const std::string&`-keyed forms sit
    beside them under `NROS_CPP_NODE_HOSTED`.
  * **`shared_from_this` cost no source edit.** The base is gone, and the verb
    survives as a hosted method returning a pointer that ALIASES `this` with an
    empty owner: it observes the node without extending its lifetime, where
    upstream's shares ownership. In this API the node is constructed by a
    generated entry (or a `main`) and outlives what it is handed to, so the two
    behave the same; a caller who stores it past the node's scope gets a
    dangling pointer where upstream would have kept the node alive. Upstream
    also THROWS `bad_weak_ptr` on an unowned node; this never throws, which is
    the RFC-0018 direction. (An earlier draft of this work item required an
    explicit `bind_shared(self)` per ported file and aborted loudly when it was
    omitted. The merged class needs neither, so the migration line is not
    there — the templates are unchanged.)

  **What W4 adds: the third arm.** `check-cpp-capability-layout` measured hosted
  (with the flag, since W2) and freestanding, and the acceptance below is not
  answerable from those two, because the freestanding arm varies `-std`,
  `-nostdinc++` and the shim all at once — it cannot tell "the porting surface
  moved a layout" from "the two libc++ shims disagree about a member". ARM 2 is
  hosted `-std=c++17` with the opt-in WITHHELD: same compiler, same standard,
  same headers as the baseline, one variable. It is also the configuration every
  hosted consumer that has not opted in now compiles in, which before W2 did not
  exist. A type that fails to compile there is NOT excusable by a `hosted-only`
  baseline entry — that kind is about types absent on FREESTANDING targets. A
  subject whose whole surface really is the porting surface gets its own kind,
  `std-only`, and there is exactly one: `rclcpp::NodeOptions`, which is
  `std::string` plus `std::vector<Parameter>` and has no smaller shape. The
  subject DERIVATION asks for the flag too, for the same reason — without it the
  list is 89 instead of 90 and that subject would silently leave the gate.

  *Acceptance (met), measured:* `sizeof(rclcpp::Node)` is **200** in all three
  arms — hosted `-std=c++17 -DNROS_CPP_STD=1`, hosted `-std=c++17` with no
  flag, and `-std=c++14 -ffreestanding -nostdinc++` against the ThreadX shim —
  and the type carries no exemption. Over the DERIVED subject list that is 90
  subjects the arm covers, not the 5 it was authored against.

  **The half `sizeof` cannot state, and the second check for it.** Deleting a
  method moves no layout, so a layout gate is green on one; the unconditional
  out-ref factories were therefore still a CLAIM — declared, never instantiated,
  since the lane's other probes all pass `-DNROS_CPP_STD=1` and the per-header
  loop only PARSES. `packages/api/nros-cpp/tests/compile/
  rclcpp_node_freestanding_surface.cpp` instantiates them, and derives from the
  class, in both no-flag arms. Mutation-tested: the out-ref `create_publisher`
  moved back behind `#ifdef NROS_CPP_HAS_SHARED_PTR` in a throwaway copy of the
  include tree fails the probe (`no matching function for call to
  'nros::Node::create_publisher'`) while the capability gate stays green.

  **Where the unconditional line actually falls, measured rather than assumed.**
  `NROS_CPP_NODE_HOSTED` gates more than the `shared_ptr` factories:
  `initialized()`, `get_node_options()`, `parameters()` and the WHOLE parameter
  facade — the `const char*`-keyed forms included — are hosted-only, because the
  store they read and the options object both live in the hosted box. The probe
  uses `ok()`, the unconditional answer to the same question as `initialized()`,
  and does not name the parameter forwarders. A freestanding `rclcpp::Node` is
  layout-identical, constructible, derivable, and its out-ref factories work; it
  is not a full freestanding port. `Node::SharedPtr`, `rclcpp::spin(node)` and
  the `shared_ptr` factories stay on the porting surface because their
  signatures are spelled in an ownership type this tree does not have
  freestanding. A freestanding program drives the same executor through
  `nros::spin()` / `nros::spin_once()`, which are unconditional.


* **W5 — say which surface a consumer is on. LANDED.** `NROS_CPP_STD` is
  documented as the PORTING surface, in four voices: the book's C++ API
  reference gains a `Two surfaces` section with a "do I need this flag?" table;
  `book/src/getting-started/porting-a-cpp-node.md` gains the one compile
  definition and the fact that `NrosRclcppCompat.cmake` already sets it;
  `docs/reference/c-api-cmake.md` carries the same at reference depth beside the
  other CMake knobs; and `docs/guides/cpp-api.md`'s "Optional std Mode" is
  re-cut, since "for any toolchain with a C++ standard library" was exactly the
  host-versus-embedded framing this phase disproves. Each states the split as
  ported-rclcpp-code versus code-written-for-nano-ros — measured, not asserted:
  1 in-tree user of the hosted `shared_ptr`-returning `create_*` against 27 call
  sites of the freestanding out-ref form in `examples/` alone — and each carries
  the three-lane probe table as the reason nothing detects it.

  Two things the item did not anticipate. **The changelog entry is `breaking`,
  not `docs`** (`changelog.d/1187.breaking.md`): an out-of-tree consumer that
  relied on the discovered macros now gets the freestanding API where it used to
  get the std one, and there is no deprecation path, because nothing can warn on
  a macro that stops being defined. And **RFC-0018 itself asserted the removed
  behaviour** — "When `NROS_CPP_STD` is defined (or detected via
  `__STDC_HOSTED__`)" — which is the same claim one layer above the header
  comments; corrected in place, with no status flip.

  The eleven header comments miscitng 0112 were already corrected by W1, which
  moved them to `std_detect.hpp`. The tree-wide sweep for the same claim
  elsewhere found **one live residue**, `nros.hpp:237` — *"gated on the
  standard-library pieces their signatures are spelled in (`__has_include`,
  never `__STDC_HOSTED__` — issue 0112, rationale in `publisher.hpp`)"* — stale
  twice over now, since the gate is `NROS_CPP_STD` and the rationale moved. It
  was left to W4, which owns that file; **W4 has since landed without taking
  it** — verified 2026-09-10, the two lines still read that way at
  `nros.hpp:242` and `git log -- packages/api/nros-cpp/include/nros/nros.hpp`
  shows no phase-438 commit touching the file. One stale comment is what the
  phase leaves behind, and it is a comment, not a gate. Every other
  `__STDC_HOSTED__` citation in
  the tree is about a **C-library** facility (`printf`/`getenv`/`fopen`), which
  is the different question this phase's "Not in scope" already enumerates, and
  each reads correctly.
  *Acceptance (met):* a reader can answer "do I need this flag" from the book
  without reading a header — the table does it in one row per consumer shape.

## Not in scope

* Deleting the std surface. It is what makes a ported file compile, which is
  RFC-0089's whole goal. This phase makes it REQUESTED, not absent.
* Giving `rclcpp::Node` a freestanding spelling. W3 measures the distance;
  closing it needs a freestanding ownership type, which is its own design.
* **The other thirteen `__STDC_HOSTED__` sites** (`result.hpp:17`,
  `result.hpp:242`, `main.hpp:45`, `main.hpp:105`, `component_node.hpp:116`,
  `component_node.hpp:190`, `component_node.hpp:226`, `log.hpp:29`,
  `node.hpp:16`, `node.hpp:18`, `node.hpp:1073`, `node.hpp:1209`,
  `nros-c/check.h:36`). All thirteen guard **C-library** facilities — `printf` /
  `fprintf`, `getenv`, `fopen` — which is a different question ("is there a libc
  to print with"), and `__STDC_HOSTED__` is precisely what answers it. Only the
  fourteenth, `nros.hpp:330`, mixes the two, and W1 handles that one.
* The Rust `std` campaign, which is phase-359 and nearly done.

## Ordering

**W0 before W2**, so the gate is the acceptance rather than a casualty — and
W0 lands with a ratchet rather than a red, because a red fast-line gate on
`main` blocks every push (see W0).

**W1 before W2**, and it earned its place: W2 landed as a five-line deletion in
one file instead of a fourteen-site sweep, and the baseline is the count —
14 entries after W0, 5 after W1, 0 after W2, with no debt paid to get from 14
to 5.

**The phase before phase-426 and phase-427.** W2 changes what "delete both C++
parameter stores" is deleting from, and W4 is phase-427 W1 relocated. Doing it
after would mean building the node merge on a layout whose conditionality is the
thing being removed.

~~Independently: **W0+W1+W2 close issue 1187**~~ — issue 1240 closed it first,
on 2026-09-09, with the AND this doc argued could not work (see the correction
under "Neither probe is right on both embedded lanes"). **That removes this
phase's urgency, not its argument**: the surface is still DISCOVERED rather than
requested, `rclcpp::Node` is still guarded on the discovered macros, and W0's
gate blindness (issue 1223) is still real. W0-W4 have since landed on those
merits rather than on the red lane, which is the status line at the top of this
doc; the ordering above is recorded as history.

## Risk

W2 is a breaking change for any OUT-OF-TREE consumer that relies on the
discovered macros — they get the freestanding API where they used to get the std
one, and the failure is a compile error naming a missing overload, which is the
loud direction. It needs a changelog entry and a line in the book, not a
deprecation cycle: there is no way to warn on a macro that stops being defined.

The in-tree cost is bounded and enumerated (W2, six sites). The measurement that
bounds it is a strip of the tracked headers into `tmp/`, re-running each TU's own
recorded compile line — not a reasoning exercise, and reproducible by repeating
it.


## Superseded (2026-09-09) — the premise was one layer too low

This phase argued the C++ std surface should be REQUESTED rather than
discovered. That was right about the defect and wrong about the remedy, and the
distance between the two is worth recording.

**What was right.** The capability macros make the API's shape a property of the
toolchain rather than a decision. That diagnosis stands and RFC-0096 inherits
it, with the measurement this phase never made: all six macros are ON for NuttX
and off for FreeRTOS and ThreadX, because one toolchain file omits
`-ffreestanding` and the other carries it.

**What was wrong.** Making the surface an opt-in leaves TWO surfaces and lets a
consumer pick. The owner's requirement is that there is ONE API, identical on
every platform, so the user is platform-agnostic — at which point there is
nothing to opt into. RFC-0096 D1.

**And the opt-in does not even work as specified.** Measured: forcing
`NROS_CPP_STD` on a `-ffreestanding` build reintroduces issue 1187's `#error`
exactly, because the macro pulls `<string>` into a build whose libstdc++ refuses
it. The request had to select a LAYER, not a macro — and under RFC-0096 it
selects nothing, because there is only one API.

**What landed and stays landed.** W0 (the `#elif`/`#else` blindness in
`check-cpp-freestanding-includes`, issue 1223), W1 (the fifteen hand-copied
capability blocks consolidated into `nros/std_detect.hpp`, rebuilt on issue
1240's predicate after that landed first), and W5 (the surface documented in the
book and `c-api-cmake.md`). W3 and W4 move to phase-442 as W8 and W3
respectively.

Issue 1187 was closed by issue 1240's fix on `main`, not by this phase's W2.
