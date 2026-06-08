---
rfc: 0030
title: "Phase 212.K.7.4.c — sequence-of-nested design proposal"
status: Draft
since: 2026-06
last-reviewed: 2026-06
implements-tracked-by: []
supersedes: []
superseded-by: null
---

# Phase 212.K.7.4.c — sequence-of-nested design proposal

## Summary

**Recommend Path A** — extend `dynamic_type_builder.cpp` to emit a Cyclone
4-word `DDS_OP_ADR | DDS_OP_TYPE_SEQ | DDS_OP_SUBTYPE_STU` instruction
(plus 5-word BSQ and ARR variants) with a packed
`(next-insn << 16) | jsr-delta` link word backfilled to point at the
nested element's existing ops sub-table.

Research found that Cyclone 0.10.5 does **NOT** use `DDS_OP_JEQ` for
sequence-of-struct (JEQ is for **union case dispatch only**); it uses a
plain `SEQ|SUBTYPE_STU` opcode with an embedded jsr-link word. The
roadmap text describing a "JEQ chain" is wrong about the opcode name —
the mechanism is the same conceptually (a packed jsr-delta) but the
instruction format is simpler than the roadmap implies. This makes
Path A strictly easier than originally scoped: we don't need a new
"JEQ chain" infrastructure, just a new variant on the SEQ/BSQ/ARR
emitter that consumes the existing JSR-patch table.

Estimated cost: ~150 LOC in `dynamic_type_builder.cpp`, two helper
tweaks, +~80 LOC of tests. No Rust-side schema changes.

Path B (vendor-bake `action_msgs`) remains a viable 1-day tactical
fallback if implementation surfaces a Cyclone-internal blocker we
can't reproduce against the public op walker.

---

## Research findings

### Idlc-emitted ops for sequence&lt;NestedT&gt;

Two live witnesses in this tree, both produced by
`build/cyclonedds/bin/idlc` (Cyclone 0.10.5):

**Witness 1 —
`examples/threadx-linux/cpp/service-client/build-cyclonedds/cyclonedds-ts/_genroot/action_msgs/msg/CancelGoal_Response.c`**
(this is the exact type that blocks native rust action e2e):

```c
static const uint32_t action_msgs_msg_dds__CancelGoal_Response__ops [] =
{
  /* CancelGoal_Response_ */
  DDS_OP_ADR | DDS_OP_TYPE_1BY | DDS_OP_FLAG_SGN,
      offsetof(action_msgs_msg_dds__CancelGoal_Response_, return_code),
  DDS_OP_ADR | DDS_OP_TYPE_SEQ | DDS_OP_SUBTYPE_STU,
      offsetof(action_msgs_msg_dds__CancelGoal_Response_, goals_canceling),
      sizeof(action_msgs_msg_dds__GoalInfo_),
      (4u << 16u) + 5u,                       // ← link word
  DDS_OP_RTS,

  /* GoalInfo_ */
  DDS_OP_ADR | DDS_OP_TYPE_EXT,
      offsetof(action_msgs_msg_dds__GoalInfo_, goal_id),
      (3u << 16u) + 7u,                       // EXT link → UUID_
  DDS_OP_ADR | DDS_OP_TYPE_EXT,
      offsetof(action_msgs_msg_dds__GoalInfo_, stamp),
      (3u << 16u) + 8u,                       // EXT link → Time_
  DDS_OP_RTS,

  /* UUID_ */
  DDS_OP_ADR | DDS_OP_TYPE_ARR | DDS_OP_SUBTYPE_1BY,
      offsetof(unique_identifier_msgs_msg_dds__UUID_, uuid), 16u,
  DDS_OP_RTS,

  /* Time_ */
  DDS_OP_ADR | DDS_OP_TYPE_4BY | DDS_OP_FLAG_SGN,
      offsetof(builtin_interfaces_msg_dds__Time_, sec),
  DDS_OP_ADR | DDS_OP_TYPE_4BY,
      offsetof(builtin_interfaces_msg_dds__Time_, nanosec),
  DDS_OP_RTS
};
```

Annotated word indices:

```
word | content                                    | role
-----+--------------------------------------------+--------------------------
  0  | ADR | TYPE_1BY | FLAG_SGN                  | return_code opcode
  1  | offsetof(.., return_code)                  |   offset
  2  | ADR | TYPE_SEQ | SUBTYPE_STU               | goals_canceling opcode ←
  3  | offsetof(.., goals_canceling)              |   offset
  4  | sizeof(GoalInfo_)                          |   elem-size
  5  | (4 << 16) | 5  = next-insn=4, jsr=5        |   LINK WORD
  6  | RTS                                        | end of top-level
  7  | ADR | TYPE_EXT                             | GoalInfo.goal_id opcode
  8  | offsetof(GoalInfo_, goal_id)               |   offset
  9  | (3 << 16) | 7  = next-insn=3, jsr=7        |   LINK WORD
 10  | ADR | TYPE_EXT                             | GoalInfo.stamp opcode
 11  | offsetof(GoalInfo_, stamp)                 |   offset
 12  | (3 << 16) | 8  = next-insn=3, jsr=8        |   LINK WORD
 13  | RTS                                        | end of GoalInfo_
 14  | ADR | TYPE_ARR | SUBTYPE_1BY               | UUID.uuid opcode
 15  | offsetof(UUID_, uuid)                      |   offset
 16  | 16u                                        |   alen
 17  | RTS                                        | end of UUID_
 18  | ADR | TYPE_4BY | FLAG_SGN                  | Time.sec opcode
 19  | offsetof(Time_, sec)                       |   offset
 20  | ADR | TYPE_4BY                             | Time.nanosec opcode
 21  | offsetof(Time_, nanosec)                   |   offset
 22  | RTS                                        | end of Time_
```

* SEQ|STU opcode at word 2. The element's first op (`GoalInfo_` body)
  is at word 7. `jsr = 7 - 2 = 5`. ✓
* `next-insn = 4` is the **width of the SEQ insn itself**: walker
  steps `ops += 4` to land on word 6 (the RTS).
* Element ops table for `GoalInfo_` (words 7–13) is appended after
  the top-level `RTS` — same trick the existing K.7.4.b nested-struct
  emitter uses (`ctx.queue` BFS, JSR backfill).

**Witness 2 — `GoalStatusArray.c`** (top-level sequence-of-struct):

```c
DDS_OP_ADR | DDS_OP_TYPE_SEQ | DDS_OP_SUBTYPE_STU,
    offsetof(.., status_list),
    sizeof(GoalStatus_),
    (4u << 16u) + 5u,                      // GoalStatus_ body at word 0+5=5
DDS_OP_RTS,

/* GoalStatus_ */
DDS_OP_ADR | DDS_OP_TYPE_EXT,
    offsetof(GoalStatus_, goal_info),
    (3u << 16u) + 6u,                      // GoalInfo_ body at 5+6=11
...
```

Same `(4 << 16) | jsr` shape. The `4` is invariant for unbounded SEQ|STU.

**Witness 3 — array-of-struct (`cdrstream.c`):**

```c
DDS_OP_ADR | DDS_OP_TYPE_ARR | DDS_OP_SUBTYPE_STU,
    offsetof(TestIdl_MsgArr, msg_field2),
    2u,                                    // alen
    (5u << 16) + 11u,                      // link
    sizeof(TestIdl_SubMsgArr),
```

ARR adds `alen` at word 2 and reshuffles: link at word 3, `elem-size`
at word 4. Total insn width = 5. Confirmed by `dds_stream_countops_arr`
in `ddsi_cdrstream.c:674-706`.

(No live witness for bounded sequence-of-struct in tree. Shape derived
from `dds_opcodes.h:243` doc-comment and `dds_stream_countops_seq` at
line 641: `bound_op = 1`, link at `ops[4]`, walker steps `5`. Same
algebra as SEQ but with sbound at word 2.)

### Cyclone ops walker behaviour

Confirmed from `third-party/dds/cyclonedds/src/core/ddsi/src/ddsi_cdrstream.c`:

* **`dds_stream_countops_seq`** (lines 639–672) — for
  `subtype ∈ {SEQ, BSQ, ARR, UNI, STU}` extracts
  `jsr_ops = ops + DDS_OP_ADR_JSR(ops[3 + bound_op])` and recursively
  walks the element body. **`STU` is a valid sequence subtype** —
  there is no separate JEQ-chain detour for sequence-of-struct.
* **`dds_stream_countops_arr`** (lines 674–706) — identical pattern
  for arrays, link at `ops[3]`, `elem-size` at `ops[4]`, walker steps
  `jmp ? jmp : 5`.
* `DDS_OP_ADR_JSR(o) = (int16_t)(o & 0xffff)` — signed 16-bit delta in
  WORDS, relative to the **opcode word** (not the link word).
* `DDS_OP_ADR_JMP(o) = o >> 16` — unsigned 16-bit `next-insn` width
  in WORDS. Walker steps `ops += jmp ? jmp : <fallback>` where
  fallback is 4/5 depending on shape.
* Element ops MUST end in their own `RTS` — the walker recurses,
  treats the sub-table as a self-contained mini-program.

**JEQ is for unions only.** `dds_stream_countops_uni` (line 708) reads
`numcases = ops[2]` and walks an inline JEQ table — completely separate
mechanism. The roadmap's "JEQ chain" wording for sequence-of-nested is
inaccurate; the actual mechanism is the simpler `SEQ|SUBTYPE_STU`
opcode with a single jsr-delta link word.

**`m_nops` semantics.** Surprise discovery: idlc's
`descriptor.c:151` increments `n_opcodes` **only on opcode stashes**
(not argument-word stashes) — `m_nops` is the **number of opcode
instructions**, not the total ops-array word count. BUT
`ddsi_sertype_default.c:327` recomputes the actual word count via
`dds_stream_countops`. So `m_nops` is functionally unused at runtime
inside the stream walker. The existing K.7.4.b sets
`m_nops = total_words` (a value that happens to be ≥ true opcode
count) — this works in practice. **Not a blocker for K.7.4.c**, but
worth tightening alongside the change.

### K.7.4.b builder primitives we reuse

From `packages/dds/nros-rmw-cyclonedds/bridge/dynamic_type_builder.cpp`:

* `OpsBuilder::push(uint32_t w)` — append one word, bounded 4096.
* `PatchTable` — `JsrPatch{ops_word, target_kind_idx}` ledger,
  backfilled after BFS completes.
* `NestedTable` — `{kind_idx, ops_word}` map of "where did the body
  for this nested kind land in the ops stream".
* `BuildContext::enqueue(kind_idx)` — BFS work queue with dedup
  against `NestedTable`.
* `emit_nested_body()` (currently line 420) — walks a nested kind's
  child kinds in `kinds[]` order, synthesises per-field offsets,
  terminates with `RTS`.
* `compute_struct_size()` — already treats SEQUENCE / BOUNDED_SEQUENCE
  as `kSeqSize` (24 bytes on 64-bit, the `{u32, u32, void*, bool}`
  Cyclone header). **No change needed** for the parent's m_size.

The patch-backfill loop (lines 791–823) computes
`delta = target_word - opcode_word` and writes the link word. This is
exactly the geometry we need — just extend `pat.ops_word` to point at
the SEQ insn's link slot (relative to its own opcode word).

---

## Proposed implementation (Path A)

### Ops emission walk

Add three new branches to `emit_kind_block` (the existing per-field
emitter, lines 540–664) for the four cases:

1. `SEQUENCE` with `kinds[inner].kind == NESTED` → 4-word SEQ|STU
2. `BOUNDED_SEQUENCE` with `kinds[inner].kind == NESTED` → 5-word BSQ|STU
3. `ARRAY` with `kinds[inner].kind == NESTED` → 5-word ARR|STU
4. (existing branches stay unchanged for primitive subtype)

Each branch:

* Push the opcode word (with `(DDS_OP_VAL_STU << 8)` in the subtype slot).
* Push `offset` (field offset within parent struct).
* For BSQ/ARR: push `bound` / `alen`.
* For SEQ/BSQ: push `elem-size` (a synthesised value derived from the
  same `emit_nested_body` size walk we already do; or push a
  placeholder we fix up after the nested body has been laid out).
  For ARR: link word **before** elem-size (per `dds_opcodes.h:253`
  shape).
* Push the link-word placeholder (initial value `(width << 16) | 0`,
  jsr backfilled later).
* Record a `JsrPatch{ ops_word = link_slot, target_kind_idx = inner_kind }`.
* `ctx.enqueue(inner_kind)` to make the BFS layer emit the nested
  body if it hasn't already.

Pseudo-code (drop into the SEQ branch, mirrors lines 603–617):

```cpp
case NROS_FIELD_KIND_SEQUENCE: {
    if (k.inner >= kind_count) { /* err */ }
    const auto& elem = kinds[k.inner];
    uint32_t st = 0;
    if (primitive_subtype(elem.kind, st)) {
        // Existing 2-word SEQ|<primitive> path.
        if (!ops.push(DDS_OP_ADR | DDS_OP_TYPE_SEQ | (st << 8))) return false;
        if (!ops.push(offset)) return false;
        return true;
    }
    if (elem.kind == NROS_FIELD_KIND_NESTED) {
        // 4-word SEQ|SUBTYPE_STU.
        if (!ops.push(DDS_OP_ADR | DDS_OP_TYPE_SEQ | (DDS_OP_VAL_STU << 8))) return false;
        if (!ops.push(offset)) return false;
        uint32_t elem_size = compute_nested_size(k.inner, kinds, kind_count);
        if (elem_size == 0) { /* err */ }
        if (!ops.push(elem_size)) return false;
        size_t link_slot = ops.len;
        // next-insn = 4 for SEQ. jsr placeholder = 0.
        if (!ops.push((4u << 16) | 0u)) return false;
        if (!t_ctx->patches.push(link_slot, k.inner)) return false;
        if (!t_ctx->enqueue(k.inner)) return false;
        return true;
    }
    // Other element kinds (string/array/sequence-of-...) — surface
    // unsupported for now (out of K.7.4.c scope).
    *out_err = NROS_BRIDGE_ERR_UNSUPPORTED_FIELD_TYPE;
    return false;
}
```

`BOUNDED_SEQUENCE` and `ARRAY` follow the same shape with the extra
sbound/alen word and (for ARR) the elem-size-after-link reshuffle.

### Nested element ops sub-table layout

The element body is emitted by the existing BFS layer at lines 780–789:
each enqueued kind_idx is laid out by `emit_nested_body` (existing
helper) at `ctx.ops.len`, then `ctx.nested.push(kind_idx, ops_word)`
records the start word. Sub-table ends in `RTS` (already emitted at
line 521).

This is **identical** to what nested-EXT fields already do — the
sequence/array element points at the same kind of body. No new
emission code; only a new caller (the SEQ/BSQ/ARR branches above).

Layout for `sequence<GoalInfo>`:

```
top-level field N opcode             ← parent's SEQ insn lives here
top-level field N offset
top-level field N elem-size
top-level field N link = (4<<16)|delta   ← patched later
top-level field N+1 ...
...
DDS_OP_RTS                            ← end top-level
                                          ↓ delta words ↓
GoalInfo first child opcode (EXT for goal_id)
GoalInfo first child offset
GoalInfo first child link (3<<16)|delta2  → UUID body
GoalInfo second child opcode (EXT for stamp)
...
DDS_OP_RTS                            ← end GoalInfo body
UUID body...
DDS_OP_RTS
Time body...
DDS_OP_RTS
```

### Size / align calculation

**Parent struct (m_size).** Unchanged: `compute_struct_size()` already
treats `SEQUENCE` / `BOUNDED_SEQUENCE` as
`kSeqSize = sizeof({u32, u32, void*, bool}) = 24` on 64-bit and
`alignof = 8`. No edit needed.

**Element size (the new word).** We need a synthetic `elem-size` for
the SEQ link triple. It must equal whatever in-host-memory size the
nested element type occupies — used by the walker to step
`buffer + i * elem_size` during stream traversal of the
`dds_sequence_t._buffer` array.

The existing `emit_nested_body` already does a synth-offset walk over
the nested kind's children. **Lift that loop into a sizer helper**:

```cpp
uint32_t compute_nested_size(uint32_t kind_idx,
                             const NrosFieldKindDescriptor* kinds,
                             uint32_t kind_count) {
    // Identical body walk to emit_nested_body's offset-rounding loop,
    // but only summing sizes — returns rounded-up final offset.
    // Handles primitive / string / sequence (=kSeqSize) / array /
    // nested children (recursive, bounded by kind graph depth).
    // Aligns final to max child alignment, like compute_struct_size.
}
```

This is ~30 LOC of code lift-out, no new logic.

**Critical cross-check vs idlc.** Idlc uses `sizeof(GoalInfo_)` from
the C struct definition — which the Rust side does NOT know. We must
synthesise a number that matches **what Cyclone's walker uses**, not
what idlc would have written. Two scenarios:

1. **Rust-only producer / Rust-only consumer.** Both sides use our
   synthesised `elem_size`. As long as the value matches the in-Rust
   stride of `heapless::Vec<NestedT, N>` element layout, walking works.
2. **Cyclone interop with idlc-generated subscribers.** The
   subscriber uses the idlc `sizeof(GoalInfo_)`. Our publisher's
   on-wire CDR doesn't carry elem-size (CDR is just a length prefix
   + concatenated elements). So `elem_size` is **walker-internal**
   and never crosses the wire — interop is unaffected.

So `elem_size` need only be **self-consistent** with the in-host
layout the producer's sample uses. The Rust shim already arranges
host-layout-matching `offset` values for top-level fields; the same
discipline applied to synthesised nested-child offsets is what
`emit_nested_body` already does. The number we feed into the link
triple must equal `compute_nested_size(child_kind)` computed with the
same alignment rules `emit_nested_body` uses for its synth-offset
walk. Same walker, same numbers.

Open risk: if `nros-serdes` (the K.7.4 producer) writes elements with
a stride that doesn't match our `compute_nested_size`, free-sample
walks will read garbage. Mitigation: a smoke test (below) does a
publish-from-Rust → take-back-into-Rust round-trip and checks payload
equality. If that holds, elem-size is correct.

### Bridge code integration

Concrete edit list for `packages/dds/nros-rmw-cyclonedds/bridge/dynamic_type_builder.cpp`:

1. **New helper `compute_nested_size`** (~30 LOC) — extracts the
   synth-offset / sizeof-children loop from `emit_nested_body` lines
   447–520 into a pure sizing function. `emit_nested_body` then calls
   it for its own size accounting too (no behavioural change for
   existing nested-struct cases).

2. **Edit `emit_kind_block` SEQUENCE branch** (lines 603–617):
   - Keep primitive fast-path.
   - Add `NESTED` element branch (~25 LOC) per the pseudo-code above.
   - Continue to reject other non-primitive element kinds.

3. **Edit `emit_kind_block` BOUNDED_SEQUENCE branch** (lines 618–633):
   - Symmetric — 5-word emission (extra sbound at word 2), link at
     word 4 has `next-insn = 5`.

4. **Edit `emit_kind_block` ARRAY branch** (lines 584–602):
   - Add `NESTED` element branch — 5-word emission with the ARR
     reshuffle (link at word 3, elem-size at word 4). `next-insn = 5`.

5. **Extend `JsrPatch` semantics** (lines 344–360):
   - Today `pat.ops_word` is the slot at `opcode_word + 2` (EXT
     layout). Backfill computes
     `opcode_word = pat.ops_word - 2`.
   - For SEQ/BSQ/ARR the link slot is at different offsets
     (`+3`, `+4`, `+3` respectively).
   - Cleanest fix: add a `uint8_t kind` field to `JsrPatch` recording
     the opcode shape (`EXT|SEQ|BSQ|ARR`), then the backfill loop
     looks up the opcode-word-to-link-slot delta per kind. Or
     simpler: store `opcode_word` directly in `JsrPatch` and have
     emit sites pass it explicitly (cleaner, ~10 LOC change to the
     existing EXT call site too).

6. **Backfill loop** (lines 791–823): rewrite as

   ```cpp
   for (size_t i = 0; i < ctx.patches.count; ++i) {
       const auto& pat = ctx.patches.entries[i];
       size_t target_word = ctx.nested.find(pat.target_kind_idx);
       if (target_word == size_t(-1)) { /* err */ }
       int32_t delta = int32_t(target_word) - int32_t(pat.opcode_word);
       uint32_t link = (uint32_t(pat.next_insn) << 16) | (uint32_t(delta) & 0xffff);
       ctx.ops.buf[pat.link_word] = link;
   }
   ```

   where `pat.next_insn` ∈ {3, 4, 5} per shape recorded at emit-time.

7. **Fix existing EXT emission alongside** (cleanup, not required
   for K.7.4.c but trivial to land in the same commit since we're
   touching the patch table): EXT is **3 words**, not 4. Drop the
   spurious extra placeholder word at line 644 and have the patch
   carry `next_insn = 3`. Avoids the latent
   `walker-sees-stray-zero-word` issue noted in the K.7.4.b code
   comment at line 819. (Audit pass: confirm no existing EXT
   regression first.)

Total: ~150 LOC delta in `dynamic_type_builder.cpp`. No Rust changes
— `bridge.rs` already produces `FieldKind::Sequence{inner=NESTED}` for
`heapless::Vec<NestedT, N>` (this is exactly the input shape that
currently surfaces `UnsupportedFieldType`).

### Array-of-nested coverage

**Yes, Path A covers Array(N, &Nested) for the same delta cost** —
the ARR opcode shape `[ADR, ARR, STU, f] [offset] [alen] [link] [elem-size]`
is structurally identical to SEQ|STU: one extra word for `alen`, link
re-positioned by one word, `next-insn = 5` instead of 4. The
backfill machinery, BFS queue, and `compute_nested_size` helper are
all shared.

The only ROS message currently in tree exercising array-of-nested is
… none that I can find in nros-* test fixtures. (Most ROS schemas
prefer `sequence<>` over fixed-length nested arrays.) Cover it
defensively because the cost is two extra LOC over SEQ.

ARR|STU **does** appear in upstream Cyclone tests
(`cdrstream.c:940, 1209, 1336, 1405`) so the walker is well-exercised
even if our tree has no in-vivo case.

---

## Tests

Land alongside the bridge change in
`packages/dds/nros-rmw-cyclonedds/tests/`:

### Test 1 — bridge unit test: sequence-of-nested op-word audit

New `tests/dynamic_bridge_seq_nested.cpp` (model after
`dynamic_bridge_smoke.cpp`):

* Construct a `kinds[]` table mirroring the action_msgs `CancelGoal_Response`
  shape:
  - kind[0] = INT8 (return_code element)
  - kind[1] = NESTED("GoalInfo", child_idx=2, n=2)
  - kind[2] = NESTED("Time", child_idx=4, n=2)  (used inside GoalInfo)
  - kind[3] = ARRAY(16, &INT8) (UUID inner)
  - kind[4] = INT32 (Time.sec)
  - kind[5] = SEQUENCE(inner=1)  ← THE NEW CASE
  - …etc
* fields[] = {return_code → kind[0], goals_canceling → kind[5]}
* Call `nros_cyclonedds_build_descriptor_from_schema(...)`.
* Assert `desc != nullptr`, no error.
* Walk `desc->m_ops` byte-by-byte:
  - `ops[0]` == `DDS_OP_ADR | DDS_OP_TYPE_1BY | DDS_OP_FLAG_SGN` (or
    plain INT8 form per current emitter)
  - `ops[2]` == `DDS_OP_ADR | DDS_OP_TYPE_SEQ | (DDS_OP_VAL_STU << 8)`
  - `ops[5]` high16 == 4 (next-insn) and low16 == 5 (jsr-delta)
  - Walk to `ops[7]` and assert it begins the GoalInfo body
  - End-to-end RTS chain terminates at expected positions.
* Validate that
  `dds_create_topic(participant, desc, "test", NULL, NULL) >= 0` — i.e.
  Cyclone's own descriptor parser accepts our hand-emitted ops table
  without aborting on `dds_stream_countops` recursion. This is the
  high-value live-cyclone check.

### Test 2 — Rust round-trip: register + publish + take CancelGoalResponse

New `tests/registry_seq_nested.rs`:

```rust
#[test]
fn register_cancel_goal_response_round_trip() {
    let ptr = register_or_lookup::<CancelGoalResponse>().expect("register");
    // Publish a CancelGoalResponse with 2 elements in goals_canceling,
    // subscribe in the same process, take it back, assert equality.
    // Uses the same loopback fixture as registry_smoke.rs.
}
```

This is the **acceptance-grade** test — it exercises the path from
Rust schema → bridge → Cyclone topic → CDR write → CDR read → Rust
deserialize. If this passes, native-rust action e2e will pass too.

### Test 3 — action e2e smoke

Wire the existing `examples/native/rust/action-server` +
`examples/native/rust/action-client` (already feature-gated on
`rmw-cyclonedds`) into the existing `tests/registry_smoke.rs` runner
or a new harness, exchange Fibonacci goal→accept→feedback→result on
`ROS_DOMAIN_ID=80`, assert client receives result. This is the
roadmap acceptance criterion verbatim.

(Existing 13/13 cyclonedds tests must continue to pass — covered by
the regular `just cyclonedds test` run.)

---

## Risks / open questions

1. **`elem_size` semantic mismatch (med risk).** Cyclone's walker uses
   the `elem-size` word to step through the in-memory element array
   (`buffer + i * elem_size`). Our synthesised number must match the
   Rust-side stride of the producer's element layout. If
   `compute_nested_size` and the Rust serializer disagree, take-back
   reads garbage. Mitigation: Test 2 (round-trip) is designed to
   catch this. If it fails, the fix is to make the Rust-side schema
   emit explicit child-offset + struct-size hints (one new field on
   `NrosFieldKindDescriptor::Nested` carrying `struct_size`).

2. **Existing K.7.4.b EXT 4-word emission (low risk, pre-existing).**
   EXT is documented as 3 words and walker steps
   `ops += jmp ? jmp : 3`. The current builder writes 4 words but
   doesn't set `jmp`, so walker reads the 4th word as the next
   opcode (a zero = `DDS_OP_RTS`, which terminates the stream). This
   happens to work iff EXT is the LAST thing in the top-level stream
   (true for all current tested cases) or right before an RTS. Risk
   of regression appears the moment an EXT field is followed by
   another field. **Recommend folding the fix into K.7.4.c**
   (one-word emit reduction plus `next_insn = 3` recorded in patch).
   No additional design needed.

3. **`m_nops` value (cosmetic).** Currently set to total words, idlc
   sets it to opcode count. Cyclone reads `m_nops` only in test
   harnesses and `ddsi_xt_typeinfo` (XTypes paths we don't enable).
   Leave as-is; document the divergence in a comment.

4. **`m_flagset` (low risk).** Existing builder sets
   `DDS_TOPIC_FIXED_SIZE` flag iff no string/sequence/nested in the
   parent. Sequence-of-nested correctly clears `fixed=false`, no
   change needed.

5. **Cycle / depth limits (low risk).** `kMaxNestedBlocks = 64`,
   `kMaxPatches = 256`, `kMaxOpsWords = 4096`. For action_msgs the
   nested graph has depth 3 (`CancelGoal_Response → GoalInfo → Time`
   + `GoalInfo → UUID`). Well below limits. Verify nothing
   user-supplied could blow the queue (unlikely — bounded by
   `kinds[]` table size, which is itself bounded by the Rust schema).

6. **Cannot test bounded-sequence-of-struct (BSQ|STU) without a
   real ROS msg using it (low risk).** No live idlc witness in
   tree. Mitigation: generate one with `idlc` against a hand-rolled
   `.idl` containing `sequence<NestedT, 4>` (idlc syntax for bounded
   sequence) — could add this as a one-off check pre-commit, but
   the shape is unambiguously documented in `dds_opcodes.h:243`.

---

## Path B fallback (if A blocked)

If Path A surfaces an unexpected walker behaviour that requires a
Cyclone-internal opcode we can't reach from `dds_opcodes.h` (extremely
unlikely given the walker analysis above), Path B unblocks action
e2e in ~½ day:

Mirror the existing `rmw_dds_common_graph` precedent at
`packages/dds/nros-rmw-cyclonedds-sys/build.rs:60-99`. Concretely:

1. Drop two .idl files into
   `packages/dds/nros-rmw-cyclonedds/idl/`:
   - `action_msgs/srv/CancelGoal.idl`
   - `action_msgs/msg/GoalStatusArray.idl`
2. In `build.rs`, after the existing `bake_descriptor` call for
   `rmw_dds_common_graph`, add two more `bake_descriptor(&idlc,
   &cancel_idl, &gen_dir, "action_msgs_cancel_goal", &[("action_msgs::srv::dds_::CancelGoal_Request_",
   "action_msgs_srv_dds__CancelGoal_Request__desc"), …], &mut cc_c);` calls.
3. The whole-archive link
   (`cargo:rustc-link-lib=static:+whole-archive,-bundle=nros_rmw_cyclonedds_descriptors`)
   already pulls in the new register TUs unchanged.
4. Have the Rust `register_or_lookup::<CancelGoalResponse>()` path
   detect "already registered statically" by looking up
   `find_descriptor(mangled_name)` in
   `descriptors.cpp` first — if it returns non-null, short-circuit
   to it instead of calling the dynamic builder.

Cost: ~40 LOC + 2 vendored .idl files. Sidesteps the dynamic
builder entirely for these two types. **Doesn't fix the general
sequence-of-nested gap**, so a user message of the same shape would
still fail — Path A remains a tracked follow-up.

Total tactical unblock time if Path A explodes: ~4 hours.
