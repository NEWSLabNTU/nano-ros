# Phase 406 — the RMW ABI's ergonomics, in one break

**Status (2026-08-31). NOT STARTED — this is the survey, and the ABI break it
argues for is deliberately held until the list is complete.** Every item below
is a UX divergence from upstream `rmw` that has NO platform reason, or a reason
that has since become false. They are collected rather than fixed one at a time
because each is an ABI change touching every backend and the codegen packs, and
five separate breaks is four more than the tree should absorb.

**Implements.** Continues phase-379 (the user API) and phase-393 (the contract),
one layer down. The evidence is
[`book/src/reference/rmw-api-comparison.md`](../../book/src/reference/rmw-api-comparison.md),
which is generated, so every count here can be re-derived rather than believed.

## The principle

**No `nros_` or `nano-ros` in this ABI.** The RMW interface is a standard, and a
vendor prefix in it says the interface is ours. Upstream names are reused where
we implement the thing (`rmw_publisher_t`, `rmw_qos_profile_t`, `rmw_gid_t`
already do this), and where a type has no upstream counterpart it still takes
the neutral `rmw_` prefix. Decided 2026-08-31.

## W1 — the type, as one argument

Measured today:

```c
/* ROS 2 */ rmw_create_publisher(node, type_support, topic_name, qos, options)
/* ours  */ (*create_publisher)(node, topic_name, type_name, type_hash,
                                domain_id, qos, options, out)
```

Five arguments become eight, and the type identity is FLATTENED into two
`const char *` that sit between `topic_name` and `qos` — so the order does not
even line up with upstream's. Nothing about `no_std` requires two arguments.

**Decision: `rmw_message_type_support_t` / `rmw_service_type_support_t`.**

```c
typedef struct rmw_message_type_support_t {
    const char *type_name;   /* "std_msgs/msg/String" */
    const char *type_hash;   /* "RIHS01_…"            */
} rmw_message_type_support_t;
```

Named `rmw_`, not `rosidl_`, and NOT because the shape differs. `rosidl_message_
type_support_t` is the literal upstream spelling, but it belongs to
`rosidl_runtime_c` — a package we do not implement — and upstream's definition
carries a `func` pointer for runtime dispatch that this ABI declined. Reusing
`rmw_publisher_t` is safe because we ARE the rmw implementation and own the
name; reusing a `rosidl_` name would redefine a type a host build can legitimately
have in scope. `rmw_` keeps the interface implementation-neutral without
squatting on someone else's namespace.

This is not a cosmetic regroup. `ROSIDL_GET_MSG_TYPE_SUPPORT(...)` already hands
back a pointer to a STATIC; codegen emits one `rmw_message_type_support_t` per
type the same way. No allocation, no runtime walk, no typesupport machinery
returning. The type argument goes back to upstream's POSITION, and future
type-carried data lands in the struct without another arity change — which
matters, because appending to a hand-mirrored FFI struct is exactly the hazard
`check-ffi-struct-mirrors` exists for (QoS `tx_express` drifted three times).

Affects `create_publisher`, `create_subscription`, `create_service`,
`create_client` (−1 argument each).

## W2 — the other flattened arguments, same move

23 slots take MORE arguments than the upstream symbol they answer. Grouped by
what the extra ones ARE:

| extra argument | n | what it is |
| --- | --- | --- |
| `session` | 16 | replaces `node`/`context` — a SWAP, not inflation; leave it |
| `visit` + `ctx` | 13 + 13 | the visitor replacing an owning out-param |
| `buf`, `buf_len`, `out_buf`, `out_len` | 22 | a byte range, flattened |
| `out` | 7 | no allocator; the caller owns the storage |
| `domain_id` | 5 | argued in `rmw_entity.h`; leave it |
| `cb` + `user_context` | 6 | a callback, flattened |

Three of those are the SAME defect as W1 — two or three correlated arguments
where upstream has one — and the same fix applies, with no allocator anywhere:

* **`rmw_byte_span_t { const uint8_t *data; size_t len; }`** and a mutable form
  carrying `capacity` + `size_t *written`. Upstream's `rmw_serialized_message_t`
  is declined because it is an `rcutils_uint8_array_t` carrying an ALLOCATOR; a
  plain span carries none, so this is available to us and the objection does not
  transfer.
* **`rmw_visitor_t { fn; void *ctx; }`** — 13 slots currently spend two
  arguments on this.
* **`rmw_callback_t { fn; void *user_data; }`** — same shape, 3 slots.

## W3 — `void *` that could be typed

26 slots carry a `void *`. Most are the visitor/callback `ctx`, which W2 absorbs.
The rest are the loan tokens (`void *token`, `void **out_token`), which want an
opaque typed handle rather than a raw pointer.

## W4 — reasons that are FALSE now

The campaign's own recurring failure: a reason argued once, and true then.

* **`rmw_init_publisher_allocation`** defers the capability question to
  **issue 0777**, which is RESOLVED — and resolved with the finding that
  *"'pools are baked' is true of one backend in five — every RMW deviation
  reason built on that clause was false"*. The reason now points at a closed
  issue for a decision the closure already made.
* **`rmw_take_loaned_message`** cites **issue 0781**, RESOLVED, whose Fix says
  plainly: decide whether the subscription-side loan pair *"earns its two slots
  given nothing implements them"*. Five loan slots are still inert. The decision
  0781 asked for was never taken.
* Four more reasons cite non-open issues (0785, 0800, 0776) and need the same
  read: a citation is not a reason once the issue is closed.

**These are the highest-value items here.** A wrong deviation reason is worse
than a missing one, because it looks settled.

## Acceptance

* Argument-count inflation drops to the items with a NAMED platform reason
  (`session`, `domain_id`, `out`); everything else is a grouped struct.
* No reason in `docs/reference/rmw-api-map.toml` cites a closed issue as if it
  were an open question.
* `just check rmw-api-comparison` regenerates with no unexplained argument drop,
  and the identical-row count RISES.
* One ABI break, one release.
