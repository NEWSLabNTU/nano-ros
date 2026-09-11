---
id: 1335
title: "The C++ API uses the POLL path where it means the DISPATCH path, and
  carries entity storage for both — an 888-byte `Subscription<M>` beside an
  arena entry that already holds the subscriber"
status: open
type: question
area: [api, api-c, core, docs]
related: [rfc-0022, rfc-0054, rfc-0096, phase-409, phase-412, phase-442]
---

## The question

The C++ API reaches the runtime by two paths for the same entity, and carries
storage for both.

**Correction to this issue's first version, which is also the point.** The table
originally here was built from the generated `nros_cpp_ffi.h` and listed only the
`*_create` family, concluding that the timer was the sole arena-shaped entity.
That reach was too narrow: the DISPATCH entry points are hand-declared
`extern "C"` in the entity headers, not in the generated header, so the scan
never saw them. They exist:

```c
/* subscription.hpp */
nros_cpp_ret_t nros_cpp_subscription_register(const nros_cpp_node_t* node, const char* topic,
                                              const char* type_name, const char* type_hash,
                                              nros_cpp_qos_t qos,
                                              nros_cpp_subscription_message_callback_t callback,
                                              void* context, size_t* out_handle_id,
                                              const nros_cpp_subscription_options_t* options);
```

and `subscription.rs` states what they mean:

> arena (rclcpp dispatch model), as opposed to the poll-style
> `nros_cpp_subscription_create` above. **The arena owns the subscriber**; spin …

So per entity there are two paths, and the C++ API uses both at once:

| path | entry point | owner |
| --- | --- | --- |
| poll-style | `nros_cpp_subscription_create(…, void *storage)` | the caller |
| dispatch | `nros_cpp_subscription_register(…, out_handle_id)` | the arena |

The arena's C-facing entry already holds everything a dispatch subscription
needs:

```rust
pub(crate) struct SubBufferedRawCEntry {
    pub(crate) handle: session::RmwSubscriber,
    pub(crate) buffer: BufferStrategy,
    pub(crate) callback: RawSubscriptionCallback,
    pub(crate) context: *mut core::ffi::c_void,
}
```

**And yet the C++ object still carries its own.** `Subscription<M>` is 888 bytes,
of which `alignas(8) uint8_t storage_[NROS_SUBSCRIBER_SIZE]` is unused on the
dispatch path. The returning `create_subscription` in `nros.hpp` adds a third
copy: a heap `detail::SubscriptionCallback<M>` cell whose only job is to hold a
`std::function` that the arena entry's own `callback` + `context` fields already
model.

`component.hpp` shows the shape the rest of the API could have had — its own
comment calls it a *"Thin wrapper over `nros_cpp_subscription_register`"* — and
it keeps nothing.

Services are the same one step less far along: `nros_cpp_service_server_register`
returns a handle id, so the slot exists, but it is handed `&out` — the C++
object — as its trampoline context. That back-reference is exactly what
`service.hpp`'s move constructor warns about:

> A callback-style service must NOT be moved after register — the arena holds
> `this` as the trampoline context (Phase 189.M3.3.e); the move only transfers
> bookkeeping and leaves that pointer stale, so don't.

A hazard that exists only because the C++ side kept an object the arena did not
need.

## Why it matters, in numbers

The caller-storage shape is why the C++ entity types are large. The object IS
the entity:

```cpp
class Publisher {                       // 824 bytes
    alignas(8) uint8_t storage_[NROS_PUBLISHER_SIZE];  // the RmwPublisher lives here
    char topic_name_[PUBLISHER_TOPIC_NAME_MAX];
    bool initialized_;
};
```

against the timer, which is a handle:

```cpp
class Timer {                           // 32 bytes
    void* executor_;
    size_t handle_id_;
    bool initialized_;
    void* closure_;
};
```

Measured `sizeof`, identical on all three toolchain arms:

| entity | bytes | shape |
| --- | --- | --- |
| `Timer` | 32 | arena handle |
| `Service<int>` | 560 | caller storage |
| `Publisher<int>` | 824 | caller storage |
| `Subscription<int>` | 888 | caller storage |
| `Client<int>` | **4 672** | caller storage |

`Client<int>` is not 4 672 bytes because a client is large. It is 4 672 bytes
because the C++ object holds the reply buffer. As an arena handle it would be
two words.

## Why the obvious explanation does not hold

The natural reading is "the arena's entries are generic, so they cannot cross a
C ABI" — and the Rust-native arena entries ARE generic:
`SubInfoEntry<M, F, const RX_BUF: usize>`, `SrvEntry<Svc, F, REQ_BUF,
REPLY_BUF>`, `TimerEntry<F>` (`packages/core/nros-node/src/executor/arena.rs`).

But the C++ path does not use those. It creates TYPE-ERASED `Rmw*` objects,
selected by `type_name` / `type_hash` STRINGS passed across the ABI, and those
are not generic. Nothing about an `RmwPublisher` requires it to sit in a caller
buffer rather than in an arena slot — and `nros_cpp_timer_create` is the
existence proof that an arena slot works for the C++ path.

The nearest thing to a recorded reason is RFC-0022's line that C "can't size
inline storage at runtime", which argues for DERIVED sizes. It does not say
which side holds the bytes.

So this may be accreted rather than decided. That is what the issue asks.

## RE-SCOPED 2026-09-12 — it does not block phase-442, and the question is sharper

The first filing asked "where should entity storage live". That was the wrong
question, and RFC-0096 D9 revision 3 answers it: for the rclcpp DISPATCH model
the Rust arena already owns the entity, and `subscription.rs` says so —

> arena (rclcpp dispatch model), as opposed to the poll-style
> `nros_cpp_subscription_create` above. **The arena owns the subscriber**; spin …

So there are TWO ABI paths per entity, not one shape to choose:

| path | entry point | owner |
| --- | --- | --- |
| poll-style | `nros_cpp_subscription_create(…, void *storage)` | the caller |
| dispatch | `nros_cpp_subscription_register(…, out_handle_id)` | the arena |

and the arena's C-facing entry already holds everything a dispatch subscription
needs:

```rust
pub(crate) struct SubBufferedRawCEntry {
    pub(crate) handle: session::RmwSubscriber,
    pub(crate) buffer: BufferStrategy,
    pub(crate) callback: RawSubscriptionCallback,
    pub(crate) context: *mut core::ffi::c_void,
}
```

**The real defect is that the C++ API does not pick one.** `create_subscription`
registers through the arena path AND keeps an 888-byte `Subscription<M>` whose
`storage_` is unused there, AND heap-allocates a `detail::SubscriptionCallback<M>`
cell to hold a `std::function` that duplicates the entry's own `callback` +
`context`. Three copies of one subscription's identity.

The same shape, one step less advanced, in services:
`nros_cpp_service_server_register` returns a handle id — so the arena slot
exists — but is handed `&out`, the C++ object, as its trampoline context. That
back-reference is what `service.hpp`'s move constructor warns about:

> A callback-style service must NOT be moved after register — the arena holds
> `this` as the trampoline context (Phase 189.M3.3.e); the move only transfers
> bookkeeping and leaves that pointer stale, so don't.

A hazard that exists only because the C++ side kept an object the arena did not
need.

**So the question is now:** per entity kind, should the C++ dispatch path carry
any storage of its own at all, and where a capture does not fit the single
`context` pointer, should its bytes live in the arena's trailing allocation
(which already exists for the rx buffer, `arena_alloc_with_trailing`)?

phase-442 W8 does not wait on this. It can take the handle shape for
subscriptions with the entry point that already exists. What this issue governs
is the remaining kinds and whether the poll-path storage stays where it is.

## What would answer it

1. **Whether caller storage was chosen or accreted.** `git log -S` on the first
   `void *storage` entry point against RFC-0022's history; if a commit argues
   for it, this issue closes as `wontfix` with the argument cited.
2. **What the arena shape would cost**, if it was accreted: the entity bytes do
   not vanish, they move from the C++ object into the executor arena, and the
   subtraction has to be MEASURED rather than stated — the same trap issues
   1145 and 1171 record for the executor backing, where bytes leaving the
   allocator arena and becoming linker-visible were reported as growth by
   everyone who forgot to subtract what they replaced. `just mem-report
   --baseline` on one real C++ image, before and after.
3. **Whether the C API follows.** It has the same concept in a third shape, and
   leaving it behind would make this two divergences instead of one. RFC-0054
   makes that a header-and-bindgen change, not a C++ change.

## Repro

```sh
python3 - <<'EOF'
import re
txt = open("packages/api/nros-cpp/include/nros/nros_cpp_ffi.h").read()
for m in re.finditer(r"nros_cpp_ret_t (nros_cpp_\w*create\w*)\((.*?)\);", txt, re.S):
    p = " ".join(m.group(2).split())
    print("%-42s %s" % (m.group(1),
          "ARENA" if "out_handle_id" in p else "CALLER STORAGE" if "void *storage" in p else "-"))
EOF

python3 scripts/check-cpp-capability-layout.py --report | grep -E 'Publisher<int>|Client<int>|::nros::Timer'
```
