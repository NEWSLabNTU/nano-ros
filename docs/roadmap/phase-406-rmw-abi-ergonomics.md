# Phase 406 — the RMW ABI's ergonomics, in one break

**Status (2026-08-31). LANDED — W1-W4, one break.** Every item below is a UX
divergence from upstream `rmw` that has NO platform reason, or a reason that has
since become false. They were collected rather than fixed one at a time because
each is an ABI change touching every backend and the codegen packs, and five
separate breaks is four more than the tree should absorb.

The break reached further than the vtable header: the four XRCE C entry points
bind `type_support`'s fields as locals, and ~60 vtable stubs plus 12 call sites
across ten `packages/rmw/cffi/tests/` files carry the new shapes. Migrating
those by text regex raised the error count three times running — the files vary
in parameter naming, `c_char` spelling and module scope. Matching on the
parameter TYPE SEQUENCE instead worked first try, because that is what does not
vary. Worth remembering for the next ABI break: the stubs are uniform in the
dimension a parser sees and irregular in the one a regex sees.

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
  arguments on this. Landed as five per-payload structs (node,
  names_and_types, topic_endpoint_info, content_filter,
  network_flow_endpoint), because the callback signature differs per payload
  and one struct cannot carry all five.
* **`rmw_callback_t { fn; void *user_data; }`** — same shape, 3 slots.
  **Landed, then REVERTED, and the measurement is why.** Upstream passes
  `(rmw_event_callback_t, const void *)` as a loose pair on the three
  `*_set_on_new_*_callback` slots. Grouping them changed nothing semantically —
  same two values, one struct — and cost exactly 3 rows out of the comparison
  doc's "identical on both sides" column, on 3 slots that are inert. The
  visitor grouping earns its keep because it replaces an owning OUT-PARAMETER
  across 13 slots; a callback pair is an IN-pair passed once, so there is
  nothing to earn. Consistency with a standard we mirror beats internal
  uniformity: `rmw_event_callback_reg_t` is gone, and the identical count went
  16 -> 19.

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
  given nothing implements them"*. Five loan slots are still inert.

  **This bullet was itself wrong, and the correction is the point of W4.** The
  survey said "the decision 0781 asked for was never taken"; reading 0781 to the
  end shows its item 2 *was* decided — the pair is KEPT, because
  `process_raw_in_place`'s scoped callback ends the borrow when it returns and
  so cannot serve `nros-c` / `nros-cpp` `try_borrow`, whose view outlives the
  call. What was never done is carry that reason into the map: the row read
  `"slot carried, no backend fills it — issue 0781"`, a citation standing in for
  an argument. So the defect was real and its diagnosis was not, which is the
  same failure mode W4 exists to catch, one level up. The row now states the
  reason and marks the decision DECIDED rather than pointing at a closed issue.
* Four more reasons cite non-open issues (0785, 0800, 0776) and need the same
  read: a citation is not a reason once the issue is closed.

**These are the highest-value items here.** A wrong deviation reason is worse
than a missing one, because it looks settled.

## Acceptance — met, with one criterion CORRECTED

* **Argument-count inflation drops to the items with a NAMED platform reason.**
  Met, and by more than the target: **+27 -> -1**, measured off the generated
  doc (upstream 180 argument slots, ours 179). The residual is `session` (16, a
  swap for `node`/`context`), `visitor` (10), `out` (9), `domain_id` (5) and
  `type_support` (4) — every one of them a named reason.
* **No reason in `docs/reference/rmw-api-map.toml` cites a closed issue as if it
  were an open question.** Met — and see W4 above: the survey's own reading of
  0781 was wrong, which is the failure mode W4 exists to catch, one level up.
* ~~the identical-row count RISES~~ — **this criterion was WRONG and is
  withdrawn.** It went 21 -> 19. Three of the five losses were the callback
  grouping and are reverted. The other two are
  `return_loaned_message_from_{publisher,subscription}`, and they are a
  CORRECTION, not a regression: upstream's second parameter there is
  `void *loaned_message` — the message — while ours is a release TOKEN. Both
  spelled `void *` before W3, so the generator called them identical *by type
  erasure while the semantics differed*. Typing the token made a real,
  pre-existing divergence visible. The generator's own header warns about
  exactly this class of false "identical" row.

  The criterion could never have been met anyway, and it is worth saying why:
  W1's whole move is to take one type-identity argument like upstream does, but
  upstream's type is `rosidl_message_type_support_t *` and rosidl types are
  declined ABI-wide. Converging on upstream's SHAPE cannot converge on its
  SPELLING. Argument-count inflation is the measure that actually tracks the
  goal; identical-row count tracks something else.
* One ABI break, one release.

## What the break cost outside the header

Recorded because the next ABI break will pay it again:

* Four XRCE C entry points, ~60 vtable stubs and 12 call sites across ten
  `packages/rmw/cffi/tests/` files, 33 call sites across 11 cyclonedds C++
  tests, and three `nros_test_take*` adapters for take-family calls whose
  `out_len` is read inside an `if` condition.
* Migrating stubs by text regex raised the error count three times running —
  the files vary in parameter naming, `c_char` spelling and module scope.
  Matching on the parameter TYPE SEQUENCE worked first try, because that is
  what does not vary. It is not collision-free: the matcher rewrote a
  TRANSPORT write callback `(ctx, *const u8, usize)`, which is shape-identical
  to a publish slot, and the rule needed narrowing to a publisher-typed
  receiver.
* **A red fast lane withdraws every backend build behind it** (issue 0952).
  `check::build` DOES list `c cpp rmw-cyclonedds rmw-xrce rmw-uorb`, so tier 1
  covers them — but `check::fast` runs first and `just` stops at the first
  failure, and `check-abi-bindings` is red for as long as regenerated bindings
  sit uncommitted, which is the normal state mid-break. The cyclonedds lane was
  red from W1's commit to the end of the phase. During an ABI break, run
  `just check rmw-cyclonedds` / `c` / `cpp` explicitly rather than trusting a
  red tier 1 to have reached them.

## Deliberately NOT spanned

Two byte-range slots keep their flat arguments, and the reason is that they are
not byte ranges:

* **`take_sequence`** — `buf` / `per_msg_cap` / `max_msgs` / `out_lens` is a
  2-D buffer with a per-message length array. A span describes one contiguous
  range; this is N of them.
* **`publish_streamed`** — `size_cb` / `chunk_cb`'s `(out_buf, cap,
  out_written)` is a chunk PROTOCOL between the runtime and the backend, not a
  message payload handed across once.
