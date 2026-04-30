# Phase 102 — RMW API alignment: named return codes + visible entity structs

**Goal:** bring the nano-ros RMW C surface closer to upstream
`rmw.h`'s shape where the divergence costs more than it saves —
specifically, named `nros_rmw_ret_t` constants instead of bare
"negative int = error", and visible entity-struct fields
(`topic_name`, `qos`, lending caps) instead of fully opaque
`nros_rmw_handle_t = void *`. Improves diagnosability and
introspection without giving up no-alloc / no-std.

**Status:** Not Started.
**Priority:** Medium. Quality-of-life for anyone writing a backend or
debugging one. Bumps the C vtable major version — no backward
compatibility shim, every consumer recompiles.
**Depends on:** none. Both changes are mechanical sweeps across the
existing 4 backends + the `nros-rmw-cffi` shim.

## Background

Phase 101's RMW comparison page
([`design/rmw-vs-upstream.md`](../../book/src/design/rmw-vs-upstream.md))
called out two places where the divergence from `rmw.h` looked sharper
than it needed to be:

1. **Error returns.** Two conventions today — pointer-returning calls
   use `NULL` for failure; integer-returning calls use a bare negative
   `int32_t`. A caller can't `switch` on TIMEOUT vs UNSUPPORTED vs
   INVALID_ARGUMENT; everything is "negative." Upstream `rmw_ret_t`
   has named constants, which is genuinely useful for diagnostics
   without costing anything but a header.
2. **Entity handles.** All entities are
   `nros_rmw_handle_t = void *` today. The runtime can't introspect
   `topic_name`, `qos`, or lending capabilities without a vtable
   roundtrip — and none of those are backend-private state.

This phase fixes both. Both changes break the C vtable ABI; bundling
them into one major version bump is cheaper than two consecutive
breaks.

## Design

### Named return codes (`nros_rmw_ret_t`)

```c
typedef int32_t nros_rmw_ret_t;
#define NROS_RMW_RET_OK                       0
#define NROS_RMW_RET_ERROR                   -1
#define NROS_RMW_RET_TIMEOUT                 -2
#define NROS_RMW_RET_BAD_ALLOC               -3
#define NROS_RMW_RET_INVALID_ARGUMENT        -4
#define NROS_RMW_RET_UNSUPPORTED             -5
#define NROS_RMW_RET_INCOMPATIBLE_QOS        -6
#define NROS_RMW_RET_TOPIC_NAME_INVALID      -7
#define NROS_RMW_RET_NODE_NAME_NON_EXISTENT  -8
#define NROS_RMW_RET_LOAN_NOT_SUPPORTED      -9
#define NROS_RMW_RET_NO_DATA                -10
```

Two return-shape conventions retained because byte-count returns are
real:

| Returns | Success | Failure |
|---------|---------|---------|
| Pointer (`open`, `create_publisher`, …) | non-NULL | `NULL` |
| `nros_rmw_ret_t` (`close`, `publish_raw`, `commit_slot`, …) | `NROS_RMW_RET_OK` | negative constant |
| `int32_t` byte count (`try_recv_raw`, `try_recv_request`, …) | `>= 0` (bytes received) | negative `nros_rmw_ret_t` |

**Drop `rmw_set_error_string`-equivalent.** No thread-local error
buffer. Backends log diagnostic strings at the failure site through
the platform's printk equivalent — embedded code paths cannot pay for
thread-local heap storage.

**Rust side.** `nros_rmw::TransportError` already enum-shaped; add the
missing variants (`IncompatibleQos`, `TopicNameInvalid`, …) and the
mapping table to / from `nros_rmw_ret_t`.

### Visible entity-struct fields

Each entity gains a small fixed-shape C struct with the metadata the
runtime actually reads, plus an opaque `void * backend_data` for
backend-private state:

```c
typedef struct nros_rmw_qos_t {
    uint8_t  reliability;   /* RELIABLE | BEST_EFFORT */
    uint8_t  durability;    /* VOLATILE | TRANSIENT_LOCAL */
    uint8_t  history;       /* KEEP_LAST | KEEP_ALL */
    uint8_t  _pad;
    uint16_t depth;
    uint16_t _pad2;
} nros_rmw_qos_t;

typedef struct nros_rmw_loan_caps_t {
    uint8_t supports_cdr_loan   : 1;
    uint8_t supports_typed_loan : 1;     /* Phase 103 */
    uint8_t reserved            : 6;
} nros_rmw_loan_caps_t;

typedef struct nros_rmw_publisher_t {
    const char           *topic_name;       /* not owned; caller storage */
    const char           *type_name;        /* not owned */
    nros_rmw_qos_t        qos;
    nros_rmw_loan_caps_t  loan_caps;
    void                 *backend_data;     /* opaque */
} nros_rmw_publisher_t;

/* Same shape for nros_rmw_subscriber_t / nros_rmw_service_*_t /
 * nros_rmw_session_t. */
```

**Rules.**

- Strings are **borrowed pointers**; caller storage must outlive the
  publisher. Documented in the Doxygen of every `create_*` function.
- The struct is filled by the vtable's `create_*` function; the
  runtime never writes to fields after creation.
- Adding a field is a major version bump. The struct is part of the
  ABI.

### Why no node back-pointer

Considered adding `node *` to each entity (upstream-style). Rejected:

- Adds a lifetime constraint that pins node memory for the entity's
  lifetime. The implicit rule already exists, but a back-pointer
  makes it stricter and more error-prone (e.g., reordering teardown).
- The single use case is introspection; node identity is already
  encoded in the wire-level topic key (namespace + name). Apps that
  need it can read it from there.
- Cost: an extra pointer per entity, multiplied by every entity in
  the system.

Net: skip.

## Work Items

- [ ] **102.1 — Define `nros_rmw_ret_t` + named constants.**
      Header file `<nros/rmw_ret.h>` (new). Constants laid out above.
      `<nros/rmw_vtable.h>` includes it. Rust side: extend
      `nros_rmw::TransportError` with the missing variants
      (`IncompatibleQos`, `TopicNameInvalid`,
      `NodeNameNonExistent`, `LoanNotSupported`, `NoData`) and add
      `From<TransportError> for nros_rmw_ret_t` plus the inverse for
      the cffi shim.
      **Files:** `packages/core/nros-rmw-cffi/include/nros/rmw_ret.h`
      (new), `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`,
      `packages/core/nros-rmw/src/error.rs`,
      `packages/core/nros-rmw-cffi/src/lib.rs`.

- [ ] **102.2 — Sweep all 4 backend impls to return named codes.**
      Mechanical: every `Err(TransportError::Generic)` /
      `Err(TransportError::Timeout)` site mapped to a specific
      named code. Sites: `nros-rmw-zenoh`, `nros-rmw-xrce`,
      `nros-rmw-dds`, `nros-rmw-uorb`. ~150 sites total per
      audit. Backend tests in `nros-tests` rerun green.
      **Files:** `packages/zpico/nros-rmw-zenoh/src/`,
      `packages/xrce/nros-rmw-xrce/src/`,
      `packages/dds/nros-rmw-dds/src/`,
      `packages/px4/nros-rmw-uorb/src/`.

- [ ] **102.3 — Define visible entity structs.**
      Headers for `nros_rmw_publisher_t`, `nros_rmw_subscriber_t`,
      `nros_rmw_service_server_t`, `nros_rmw_service_client_t`,
      `nros_rmw_session_t`. Each carries the metadata fields above
      plus a `void *backend_data` slot. cbindgen config updated to
      emit the typed structs (drop `nros_rmw_handle_t = void *` for
      these).
      **Files:** `packages/core/nros-rmw-cffi/include/nros/`
      (new per-entity headers),
      `packages/core/nros-rmw-cffi/src/lib.rs`,
      `packages/core/nros-rmw-cffi/cbindgen.toml`.

- [ ] **102.4 — Vtable signature update.**
      Every `create_*` function pointer changes from
      `nros_rmw_handle_t (*create_publisher)(session, topic_name,
      type_name, type_hash, qos)` to
      `nros_rmw_ret_t (*create_publisher)(session, topic_name,
      type_name, type_hash, qos, nros_rmw_publisher_t *out)`. The
      runtime owns the `nros_rmw_publisher_t` storage; the vtable
      fills it. Same for subscriber / service-server /
      service-client.
      **Files:** `packages/core/nros-rmw-cffi/include/nros/rmw_vtable.h`,
      `packages/core/nros-rmw-cffi/src/lib.rs`,
      every backend's create-entity path.

- [ ] **102.5 — Runtime-side reads of `topic_name` / `qos` /
      `loan_caps` go through the visible struct, not vtable
      callbacks.** Removes the `get_topic_name` callback (now a
      plain field read). Removes the runtime probe for lending
      capability (now `pub->loan_caps.supports_cdr_loan`).
      **Files:** `packages/core/nros-node/src/`.

- [ ] **102.6 — Update Doxygen mainpages + book.**
      `nros-rmw-cffi/docs/mainpage.md` Quick Start example shows
      the new struct-out signature. `book/src/design/rmw-vs-upstream.md`
      "Error returns" + "Entity handles" sections updated to
      reflect the post-Phase-102 state. `book/src/porting/custom-rmw.md`
      examples switched.

- [ ] **102.7 — Bump the cffi major version.**
      `Cargo.toml` of `nros-rmw-cffi` from `0.1.x` to `0.2.0`. Note
      in CHANGELOG (or commit message body since we have no
      CHANGELOG yet) that this is a hard ABI break — no migration
      shim.

## Acceptance Criteria

- [ ] `<nros/rmw_ret.h>` exists with all named constants + Doxygen.
- [ ] Every backend test in `nros-tests` passes after the sweep.
- [ ] `nros_rmw_handle_t` is gone from the public C surface; replaced
      by the typed entity structs.
- [ ] `cargo build -p nros-rmw-cffi` shows no `void *` parameters in
      the cbindgen output for create-entity calls.
- [ ] `book/src/design/rmw-vs-upstream.md` updated; book builds clean.
- [ ] Manual smoke test: zenoh-pico talker / listener still talk
      after the rebuild on POSIX + at least one embedded slice
      (FreeRTOS or NuttX).

## Notes

- **No backward compat shim.** Embedded users rebuild from source on
  every change anyway; a source-compat shim is not worth the
  carrying cost.
- **Why not also tackle the Rust trait surface?** The Rust traits
  already use named `TransportError` variants and typed
  `Publisher` / `Subscriber` types. The phase is C-shape work; the
  Rust side gets the new error variants but the trait shape is
  unchanged.
- **Future Phase 103 hook.** The `loan_caps.supports_typed_loan`
  bit is already laid out so Phase 103's typed-loan path doesn't
  break the struct layout again.
