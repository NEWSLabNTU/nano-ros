---
id: 1335
title: "One entity concept, three storage shapes: the C++ timer lives in the
  Rust arena, the other seven C++ entities live in a caller buffer, and the C
  API puts all of them in caller structs — no recorded decision says why"
status: open
type: question
area: [api, api-c, core, docs]
related: [rfc-0022, rfc-0054, rfc-0096, phase-409, phase-412, phase-442]
---

## The question

Where an entity's memory lives is answered three different ways in this tree for
one concept, and the difference is not recorded anywhere as a decision.

Measured on `nros_cpp_ffi.h` — every `create` entry point, classified by whether
it returns an arena index or writes into a caller buffer:

| entry point | shape |
| --- | --- |
| `nros_cpp_timer_create` | **arena** — `size_t *out_handle_id` |
| `nros_cpp_timer_create_on_clock` | **arena** |
| `nros_cpp_timer_create_oneshot` | **arena** |
| `nros_cpp_timer_create_in_group` | **arena** |
| `nros_cpp_publisher_create` | caller storage — `void *storage` |
| `nros_cpp_subscription_create` | caller storage |
| `nros_cpp_service_server_create` | caller storage |
| `nros_cpp_service_client_create` | caller storage |
| `nros_cpp_action_server_create` | caller storage |
| `nros_cpp_action_client_create` | caller storage |
| `nros_cpp_guard_condition_create` | caller storage |

And the C API is a third shape again: `nros_publisher_init_with_qos` and
`nros_timer_init` both take a pointer to a caller-declared
`struct nros_publisher_t` / `struct nros_timer_t`, so there the TIMER is
caller-storage too.

So: C++ timer in the Rust arena, C++ everything-else in a C++ buffer, C timer
and C publisher in a C struct. Three answers, one concept.

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

## What it blocks

phase-442 W8, directly. RFC-0096 D9 has to say where a returned
`X::SharedPtr` handle's entity lives, and the answer is different depending on
this: if entity storage moves into a Rust-side arena the way the timer's
already has, every C++ entity becomes handle-shaped, `nros::Handle<T>` is
trivially correct because C++ owns nothing, and D9 dissolves instead of being
answered. If caller storage is the deliberate design, D9 has to pick among
pools, node template parameters, or derived counts — all of which are heavier
and none of which delete the 4 672.

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
