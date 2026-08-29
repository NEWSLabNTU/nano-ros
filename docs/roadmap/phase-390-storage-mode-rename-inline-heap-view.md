# Phase 390 — `owned`/`borrowed` name the wrong things; rename to `inline`/`heap`/`view`

**Status (2026-08-30). W1-W5 landed; W6 (emitter internals) recorded, not started.** Opened from a
memory-allocation review that settled the message allocation strategy before
touching layout. Renames the RFC-0033 storage modes and amends the RFC with a
guarantee column. No behaviour change; the modes keep their current semantics and
support matrix.

## The two names that are wrong

`packages/cli/rosidl-lower/src/config.rs:35` defines three modes. Two are
misnamed, for different reasons.

**`owned` does not distinguish anything.** A `mode = "heap"` field is *also*
owned by the message — it is `alloc::Vec<T>`, dropped with the struct. What
actually separates the two is **where the bytes live**: inside the struct
(`heapless::Vec<T, N>`, a fixed `[N]`) or outside it. `owned` names the
ownership, which both share, instead of the location, which they do not.

**`borrowed` names the lifetime and hides the cost.** The distinguishing fact
for a user is not that the data is borrowed — it is that **nothing was
deserialized**. The field is a `&'a [T]` into the CDR receive buffer and the
user owns the decode. That cost does not disappear; it moves to the caller, and
the name does not say so.

There is also a collision. `packages/core/nros-rmw/src/traits.rs:2018` already
has `SlotBorrowing`, whose `try_borrow()` returns a `View<'a>` over the
backend's receive buffer — the *same idea at whole-message granularity*. Two
words for one concept across two layers.

## The names

| now | becomes | names |
| --- | --- | --- |
| `owned` | **`inline`** | data lives in the struct |
| `heap` | `heap` | outside the struct, still owned |
| `borrowed` | **`view`** | not materialised; a view into the receive buffer |

### Why `view` and not `deferred`

`deferred` was the first candidate and was rejected on two counts:

1. **It over-promises.** "Deferred" implies the decode happens later. Often it
   never happens — the consumer memcpys the bytes to a DMA descriptor, or
   forwards them to another topic. Nothing is deferred; something is *declined*.
2. **It discards the lifetime warning.** `borrowed`'s one virtue was saying
   "this is not yours to keep". In Rust the `View<'a>` type enforces that; in C,
   `nros_borrowed_str_t` is a bare `{ptr, len}` and the *name* was the only
   guard. `nros_deferred_str_t` does not warn. `view` keeps the word that the
   generated types already carry.

`view` also maps 1:1 onto artefacts that already exist — `{Msg}View<'a>`,
`nros::StringView`, `Span<T>`, `nros/borrowed.h`'s view structs, and
`SlotBorrowing::View`. A user writing `mode = "view"` gets a `View` type. The
config token *is* the type name, which needs no explanation, and the
`SlotBorrowing` collision resolves into one vocabulary instead of two.

### The axis is not perfectly uniform, deliberately

`inline` and `heap` name storage location; `view` names access. A fully uniform
scheme (`inline`/`heap`/`inplace`) was considered and dropped: `inline` and
`inplace` differ by two letters in the middle of a word, in a config file, where
the failure mode is silent. Legibility beats taxonomy. The RFC will say this in
a sentence rather than the names contorting to avoid saying it.

## Blast radius (measured)

| surface | planned | actual |
| --- | --- | --- |
| config-token strings (`"owned"` / `"borrowed"`) in Rust | 37 | 37 |
| generated C type names (`nros_borrowed_*`) | 26 | 2 types + 29 `_deserialize_borrowed` |
| doc/book mentions of `borrowed` | 314 | **~10 that mean the MODE** |
| public header | `borrowed.h` | renamed to `view.h` |
| Rust type/trait names naming the mode | not counted | **~70, and they collide — see W5** |

**Two of those estimates were wrong, in opposite directions.** The 314 doc
mentions counted the ordinary English word: every "borrowed pointer", "borrowed
string; caller owns the storage" and `SlotBorrowing` reference in the book is
prose about borrowing, not about the mode. All five book hits were left alone
deliberately. Meanwhile the Rust surface — `DeserializeBorrowed`,
`BorrowedMessage`, `CppBorrow`, `{Msg}Borrow`, `create_subscription_borrowed` —
was not counted at all and is larger than the C surface that was.

`borrowed.h` is public ABI. Renaming its types (`nros_borrowed_str_t`,
`nros_borrowed_bytes_t`) breaks C consumers **at compile time** — loud, not
silent, and accepted deliberately for consistency rather than leaving the C
surface speaking the old vocabulary.

## Waves

**W1 — config tokens + Rust enum.** `StorageMode::{Owned,Heap,Borrowed}` ->
`{Inline,Heap,View}`, `as_str()`, and the `.toml` parser. Old tokens continue
to parse, with a deprecation diagnostic naming the replacement. This is the
wave that unblocks prose in phases 391 and 392.

**W2 — generated type names and the public header.** `borrowed.h` -> the view
header, `nros_borrowed_str_t` -> `nros_view_str_t`, and the 26 generated names.
Release-noted as a breaking change for C consumers.

**W3 — RFC-0033 amendment.** Add a **guarantee** column, which the RFC does not
currently have:

| mode | guarantee |
| --- | --- |
| `inline` | bounded, statically provable, analysable — the default |
| `heap` | opt-in, unguaranteed; the consumer owns a fragmentation bound |
| `view` | opt-in, unguaranteed; the consumer owns the decode and the lifetime |

Also records the relationship RFC-0010 already implies but does not state: the
typed per-field `view` mode and the raw whole-message `SlotBorrowing` path are
separate mechanisms for the same idea at different granularity, and loan/borrow
are raw-only because CDR length is not known before encoding.

**W4 — docs.** Landed, and much smaller than planned: RFC-0033 and RFC-0038
token spellings, two `phase-303` references, and two api-parity ledger `why`
paragraphs. `book/src/` needed NO changes — every one of its five `borrowed`
mentions is the English word.

**W5 — the Rust surface.** LANDED. Measured after W2:

| identifier | sites | note |
| --- | ---: | --- |
| `DeserializeBorrowed` | 28 | trait |
| `CppBorrow` / `CppBorrowKind` | 24 | emitter enum |
| `BorrowedMessage` | 21 | trait |
| `{Msg}Borrow` (`ImageBorrow`, `ShapesBorrow`, `FrameBorrow`) | 17 | GENERATED marker types |
| `SubBufferedBorrowedEntry` | 8 | executor arena entry |
| `create_subscription_borrowed` | 4 | public Rust API |

**The blocker was a collision, not volume, and it is resolved: `{Msg}Viewable`.**
`{Msg}Borrow` is the marker a user names to subscribe, and its obvious new name
`{Msg}View` is ALREADY TAKEN by the view struct the marker points at. The marker
is zero-sized and carries NO lifetime — that is why it exists at all, since
`B::View<'a>` does and a generic parameter cannot be the lifetime-carrying type
itself. `Viewable` names the CAPABILITY rather than the mechanism, which is
exactly what a lifetime-free marker asserts, and cannot be mistaken for the view.

| old | new |
| --- | --- |
| `{Msg}Borrow` | `{Msg}Viewable` |
| `BorrowedMessage` | `ViewableMessage` |
| `DeserializeBorrowed` | `DeserializeView` |
| `CppBorrow` / `CppBorrowKind` | `CppView` / `CppViewKind` |
| `SubBufferedBorrowedEntry` | `SubBufferedViewEntry` |
| `create_subscription_borrowed` | `create_subscription_viewable` |

`create_subscription_borrowed` keeps a `#[deprecated]` forwarder — it is a
public Rust API and the rename is cosmetic, where W2's C break was accepted only
because a C type name cannot carry a deprecation.

`SlotBorrowing`, `nros_cdr_borrow_*` and `borrow_loaned_message` stay as they
are — those spell a VERB, and RFC-0033 now records why.

**W6 — the emitter's OWN vocabulary.** NOT started; found while rebasing W5.
The renames above cover the types, traits and APIs a user names. The codegen's
internal predicates and template-context keys still say `borrowed`:

| identifier | sites |
| --- | ---: |
| `is_borrowed` | 59 |
| `has_borrowed` | 19 |
| `borrowed_c_type` | 8 |
| `has_borrowed_{goal,feedback,request,response,result}` | 35 |
| `borrowed_read_fn` | 7 |

**Why this is not another sweep.** Most of these are minijinja CONTEXT KEYS,
which exist twice — once as a field on a Rust context struct, once as a name
inside a `.jinja` template. minijinja renders an unknown key as EMPTY rather
than failing, so renaming one side and not the other produces a template that
still emits, just without the branch. That is a silent wrong-output bug in a
generator, and the golden corpus only catches it for shapes the corpus covers
(which phase-390 W2 already found were fewer than advertised).

Do it as one commit per key, with the golden corpus re-recorded and DIFFED each
time, or not at all. The user-facing vocabulary is already consistent; this is
internal tidiness with a real footgun attached.

## Explicitly not in this phase

The support matrix does not change. `heap` and `view` remain unsupported for
srv/action payloads and shape-limited in C/C++ exactly as
`config.rs:45-65` records today. Whether `heap` survives at all is a question
for [phase 391](phase-391-allocation-unification-and-tier-model.md), which owns
the tier model; this phase renames what exists.
