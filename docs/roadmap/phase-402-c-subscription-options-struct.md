# Phase 402 — the C subscription API takes an options struct

**Status (2026-08-30). Complete — W1-W4 landed.** Opened from
[issue 0896](../issues/0896-c-cpp-subscriptions-never-state-a-buffer-hint.md),
whose subscribe half has no way to deliver a receive-buffer hint: the register
function's argument list cannot grow. Implements the Q1 decision recorded there.
Breaking change to the C API, deliberately.

## The shape today

Three registration functions, and they differ in EXACTLY ONE thing — the
callback type:

| function | callback | `callback_group` |
| --- | --- | --- |
| `nros_cpp_subscription_register` | `RawSubscriptionCallback` | yes |
| `nros_cpp_subscription_register_with_info` | `RawSubscriptionInfoCallback` | **no** |
| `nros_cpp_subscription_register_validated` | `RawSubscriptionSafetyCallback` | **no** |

Everything else is identical: `node, topic, type_name, type_hash, qos, callback,
context, sched_context, out_handle_id`.

**That table is the argument for this phase.** The variance is one parameter and
the duplication is eight, and the duplication has already drifted — two of the
three cannot take a callback group at all, for no reason anyone chose. The flat
list has also already produced one real bug: `component.h`'s own comment records
that a C caller built against the 9-arg shape "left the 11th slot as stack
garbage, which the Rust side dereferenced (SIGSEGV in `cstr_to_str` on Zephyr
native_sim; silent luck elsewhere)".

Adding `rx_buffer_hint` as a tenth/eleventh parameter makes that worse three
times over.

## The shape proposed

```c
typedef struct nros_cpp_subscription_options_t {
    /// Receive-buffer size hint, bytes. 0 = use the image default.
    /// Issue 0896: a subscription that does not state one takes the small
    /// size class regardless of its message type.
    uint32_t    rx_buffer_hint;
    /// Scheduling-context slot. 0 = inherit the executor default.
    uint8_t     sched_context;
    uint8_t     _reserved[3];
    /// Callback group name. NULL = default.
    const char* callback_group;
} nros_cpp_subscription_options_t;

nros_cpp_subscription_options_t nros_cpp_subscription_default_options(void);

int32_t nros_cpp_subscription_register(
    const nros_cpp_node_t* node, const char* topic,
    const char* type_name, const char* type_hash,
    nros_cpp_qos_t qos,
    nros_c_subscription_callback_t callback, void* context,
    size_t* out_handle_id,
    const nros_cpp_subscription_options_t* options /* NULLable = all defaults */);
```

Nine arguments instead of ten, the same trailing options struct on all three
variants, and `callback_group` becomes available to all three rather than one.

**Precedent, not invention.** Issue 0808 hit this on `create_session` and
`rmw_entity.h` records the resolution: take one options struct, because
`rmw_publisher_options_t` / `rmw_subscription_options_t` already solved exactly
this for entities, and because encoding config in a locator string means every
backend reimplements a parser.

`_with_info` is NOT retired as a function — its callback type is genuinely
different — but it stops being a parallel argument list. The Q1 decision said
"retire `_with_info`"; what is actually retired is the DUPLICATION, and this doc
says so rather than letting the summary read as a deletion.

## Waves

**W1 — the struct and the three signatures.** `nros-cpp` Rust side plus the
hand-written `component.h` prototypes. `options == NULL` means all defaults, so
the NULL case is the old behaviour exactly.

**W2 — the hint reaches the backend.** `rx_buffer_hint` into the `TopicInfo`
the register already builds (`nros-c/src/subscription.rs:487` does this on the
other path). This is the wave that closes issue 0896's title.

**W3 — call sites.** Nine C files under `examples/`, plus `nros-cpp`'s
`component.hpp` / `component_node.hpp`. Mechanical once W1 lands.

**W4 — the generated `_subscribe` helper** (issue 0896 layer 4). Codegen emits
`{Msg}_subscribe` passing `RX_MAX_SERIALIZED_SIZE` as the hint, so the type is
named ONCE and the hint cannot drift from it. This is the user-visible payoff;
everything above is plumbing for it.

## Explicitly not in this phase

The publisher side. `nros_cpp_publisher_register` has its own argument list and
its own reasons, and mixing the two would make this diff unreviewable. If the
same fix is wanted there it is a sibling phase.

## Landed 2026-08-30

All four waves. Two findings worth keeping.

**The call sites were NOT all defaults.** The plan assumed the sweep was
mechanical; five were not — `ComponentNode::create_subscription_in` and
`Node::create_subscription_in` pass a real callback group, and
`Node::create_subscription{,_with_info,_with_safety}` each pass a RUNTIME
`sched` computed from `SubscriptionOptions::sched_context`, so the literal `0`
never appears while the value is not statically zero. All five now start from
`nros_cpp_subscription_default_options()` and set only what they mean, so a
field appended later keeps its intended default rather than whatever the frame
held.

**`subscription.hpp` was missing from the survey** and is the largest caller —
it carries its own three local `extern "C"` prototypes plus four call sites.
Nothing C++ compiles without it. The blast-radius list in this doc was built
from `git grep` on the function name, which missed it because those prototypes
are cbindgen-excluded and spelled locally.

**Two defects in W1 itself, both caught downstream rather than by review:**

* the new typedef had no `#ifndef NROS_CPP_FFI_H` guard, unlike the
  `nros_cpp_qos_t` mirror beside it, so `check-c`'s issue-0160 cross-include TU
  failed with `redefinition of struct`;
* the struct was inserted BETWEEN `nros_cpp_subscription_register`'s doc comment
  and the function, so the register's prose documented the struct — visible in
  the regenerated `nros_cpp_ffi.h`.

**W4's macro needed a C helper it could not have known to ask for.** The natural
body takes the address of a compound literal, `&(const T){...}`, which C++
rejects outright ("taking address of rvalue") with no initializer spelling that
fixes it. `nros_cpp_subscription_register_hinted` — a `static inline` taking the
hint as a scalar — sidesteps the compound literal, so ONE macro serves both
languages instead of a C-only macro plus a poisoned C++ arm. Verified by
compiling a TU under both `gcc -std=c11` and `g++ -std=c++17`, not by argument.

## Still owed

No RUNTIME evidence. Everything above is compile-tier: `just check-c`,
`check-cpp`, `check-ffi-struct-mirrors` and `just ci-l1` are green, and no
fixture was rebuilt, so nothing here demonstrates that a hinted subscription
actually lands in the large payload class. That measurement is issue 0896's
remaining debt, not this phase's — but it is the thing that would turn "the
plumbing exists" into "the saving is real".
