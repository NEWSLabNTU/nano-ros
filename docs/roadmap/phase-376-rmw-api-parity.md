# Phase 376 — the RMW ABI campaign: generic naming, feature completeness, RTOS correctness

**Status (2026-08-25). ALL FIVE WAVES COMPLETE.** W1/W2 (measurement),
W3 (naming, argument declarations, the return-code question), W4 (feature
completeness), W5 (the RTOS-correctness audit) have all landed. What is left is
FOLLOW-UP work, filed as issues and listed at the bottom of this doc — not
campaign waves.

Measured, by the two checkers on the `just check` line:

| | |
| --- | ---: |
| contract symbols to mirror | 72 |
| slots identical to upstream | 20 |
| name matches, arguments DECLARED | 41 |
| answered by a grouped slot | 8 |
| plain ABI functions | 2 |
| answered at another layer | 2 |
| **slots present but args differ, undeclared** | **0** |
| **undeclared return-type differences** | **0** |
| **contract symbols with no slot** | **0** |
| deferred, with a tracked issue | 1 (0776) |
| declared RTOS additions | 9 |
| **undeclared extra slots** | **0** |
| **vendor-named types in signatures** | **0** |

`rmw-api-parity`: 88 contract symbols — vtable 69, layer 4, declined 14, gap 1.

Every number above is re-derived per run from the committed contract snapshot
and the header; none is a constant in a doc. The three zeroes in bold are what
"generic RMW ABI" means operationally: nothing differs from upstream that is
not written down where the difference is.

**This status block was itself a casualty of the campaign it describes.** Until
2026-08-25 it carried four contradictory paragraphs appended over successive
waves — "W4 IS COMPLETE" above "what is left of the campaign is W4", and a
closing "Today: 0 of 79 slots match name and args ... W3+ is not started" that
had been false for three waves. A status line that is appended to rather than
rewritten stops being a status line.

## The campaign, in one rule

> Our vtable ABI is a **generic RMW ABI**. It looks like upstream's — same
> names, same arguments, no vendor prefix — and every difference exists because
> an RTOS target requires it, is written down where the difference is, and is
> checked automatically.

Three properties follow, and they are what the waves below deliver:

1. **Naming** — a backend author implements RMW, not nano-ros. The slot is
   `take`, not `try_recv_raw`; the parameter is `rmw_subscription_t *`, not
   `rmw_subscription_t *`.
2. **Feature completeness** — every function upstream requires of an
   implementation is a slot, generic over all backends.
3. **RTOS correctness** — a deviation is a *decision*: it is declared, it names
   the target constraint that forces it, and nothing else deviates.

None of this is a matter of taste, which is why all three are checkable.

## What "the contract" is, and why it is not the header

Comparing `rmw.h` to `rmw_vtable.h` is wrong in both directions.

**Upstream's headers overstate it.** The `rmw` package declares **177**
`RMW_PUBLIC` functions across 40 headers; ~89 are utilities rmw itself DEFINES —
allocators, error handling, `names_and_types` init/fini, qos string conversions,
`validate_*`. An implementation links those. Comparing against 177 manufactures
~89 phantom gaps.

**Our header understates ours.** `rmw_vtable.h` is the backend seam, while
`rmw_wait` is `Executor::spin_once` one layer up, graph queries live in the
Cyclone backend one layer down, and serialize/deserialize is codegen.

So the contract is taken empirically — the `rmw_*` symbols a real implementation
DEFINES:

| library | defined |
| --- | ---: |
| `librmw_cyclonedds_cpp.so` | 88 |
| `librmw_fastrtps_cpp.so` | 88 |
| `librmw_zenoh_cpp.so` | 88 |
| **intersection** | **88** |

Three implementations, three transports, the same 88, not one private extra.
Recorded at `docs/reference/rmw-implementation-{contract,signatures}.txt` so the
comparison runs on a host with no ROS; regenerate in the distrobox.

**Scope note.** The 89 non-implementation utilities are deliberately out of
scope as *vtable slots* — they are pure functions and library helpers, and a
dispatch slot for something that cannot vary by backend is a null decision. If
feature completeness is later wanted for them too, they belong as plain C
functions in the ABI headers, tracked by the same tooling.

## The three checks (landed, W1-W2)

| command | question | today |
| --- | --- | --- |
| `just check-rmw-api-parity` | is every contract symbol *classified* — a slot, another layer, or declined with a reason? | **passes**; fails the moment upstream grows an unclassified symbol |
| `just rmw-abi-shape` | does each one have a slot with upstream's **name** and **args**, and are the signatures vendor-free? | **0/79 name+args**, 10 vendor types |
| `scripts/rmw-api-inventory.py` | what does upstream actually declare (177), and with what signatures? | input to the two above |

`rmw-abi-shape --check` is deliberately NOT on the `just check` line yet: it
fails by construction until the migration lands, and a gate that cannot pass is
a gate people learn to skip. It joins `check` at the end of W5.

## W3 — naming: the vtable becomes a generic RMW ABI

### W3.a — types lose the vendor prefix (10 types, 45 uses)

> **Note (2026-08-24):** this table's left column had been clobbered — every
> row read `` `rmw_x_t` -> `rmw_x_t` ``, because the tree-wide W3.a rename ran
> over the doc as well as the code and rewrote the "ours" column into the
> "becomes" column. A rename table whose two columns are equal records nothing.
> The old spellings are restored below from `c7fdc1eb1~1`.

| ours (pre-W3.a) | becomes | uses |
| --- | --- | ---: |
| `nros_rmw_session_t` | `rmw_session_t` | 10 |
| `nros_rmw_subscription_t` | `rmw_subscription_t` | 10 |
| `nros_rmw_publisher_t` | `rmw_publisher_t` | 9 |
| `nros_rmw_service_t` | `rmw_service_t` | 5 |
| `nros_rmw_client_t` | `rmw_client_t` | 5 |
| `nros_rmw_qos_t` | `rmw_qos_profile_t` | 4 |
| `nros_rmw_event_kind_t` | `rmw_event_type_t` | 2 |
| `nros_rmw_event_callback_t` | `rmw_event_callback_t` | 2 |
| `nros_rmw_publisher_options_t` | `rmw_publisher_options_t` | 1 |
| `nros_rmw_subscription_options_t` | `rmw_subscription_options_t` | 1 |
| `nros_rmw_ret_t` | `rmw_ret_t` | every slot |

Struct **tags** may stay ours; the typedef names are the surface a backend sees.

**The hazard this creates, and the answer.** A translation unit including both
our header and upstream `rmw/rmw.h` would then define each name twice. No TU in
this repo does — a target image never links real rmw, and every host-side
consumer reaches the backend through Rust — but "nobody does it today" is not a
guarantee, and the failure mode without a guard is two types of one name whose
layouts differ. So the header gets:

```c
#if defined(RMW_RMW_H_)
#  error "nros/rmw_vtable.h defines the generic RMW ABI and cannot share a \
translation unit with upstream <rmw/rmw.h>. Include one or the other."
#endif
```

An `#error` because the alternative — silently winning the redefinition race —
is the class of bug this repo has paid for three times in FFI struct mirrors.

### W3.b — slots take upstream names (16 renames)

`try_recv_raw` -> `take`, `publish_raw` -> `publish`, `pub_loan` ->
`borrow_loaned_message`, `sub_borrow` -> `take_loaned_message`, `send_reply` ->
`send_response`, `send_request_raw` -> `send_request`, `try_recv_request` ->
`take_request`, `try_recv_reply_raw` -> `take_response`, `try_recv_sequence` ->
`take_sequence`, `service_server_available` -> `service_server_is_available`,
`assert_publisher_liveliness` -> `publisher_assert_liveliness`,
`register_{publisher,subscription}_event` -> `{publisher,subscription}_event_init`,
`pub_commit` -> `publish_loaned_message`, `pub_discard` ->
`return_loaned_message_from_publisher`, `sub_release` ->
`return_loaned_message_from_subscription`.

The rule is mechanical — upstream's name minus its `rmw_` prefix — so the check
needs no authored name mapping. An 88-entry mapping is a place for a mistake to
hide.

Both halves of W3 touch every backend and the committed bindgen output
(RFC-0054: the header is the SSoT, `scripts/gen-abi-bindings.sh` regenerates,
`check-abi-bindings` gates staleness).

### W3.c — the argument deviations get declared

The differences are systematic rather than incidental, and each is a real target
constraint. They stay; they get written down per slot in `ARG_DEVIATIONS`:

| upstream | ours | the constraint |
| --- | --- | --- |
| `const rmw_node_t *` | `rmw_session_t *` | an image opens ONE session; upstream's context/node split has no target-side meaning |
| `const rosidl_message_type_support_t *` | `const char *` pkg + `const char *` type | no typesupport indirection on target — codegen bakes the type |
| returns `rmw_publisher_t *` | returns `rmw_ret_t`, entity is an OUT param | the caller owns the entity STRUCT's storage, so there is nothing to return a pointer to — and a status is more informative than NULL. Note this is not the same claim as "no runtime allocation": the backend still heap-allocates its `backend_data` in the same call |
| `rmw_publisher_allocation_t *` | absent | upstream's first two parameters (typesupport, sequence bound) are already declined ABI-wide, so the symbol cannot cross this seam whatever the third holds. **Not** "pools are baked" — issue 0777 measured that clause false for four backends of five |

### W3.d — the return-code question, narrower AND sharper than it looked

`RMW_RET_OK` and ours are both `0`, so the common path already agrees.
Everything else differs in sign and value: upstream `ERROR 1 / TIMEOUT 2 /
UNSUPPORTED 3 / BAD_ALLOC 10 / INVALID_ARGUMENT 11`, ours `-1 / -2 / -5 / -3 /
-4`. We also carry 12 codes upstream has no name for (`NO_DATA`, `WOULD_BLOCK`,
`BUFFER_TOO_SMALL`, `MESSAGE_TOO_LARGE`, `INCOMPATIBLE_ABI`, `NO_BACKEND`,
`CONNECTION_FAILED`, ...), and upstream carries `INCORRECT_RMW_IMPLEMENTATION`,
which cannot arise in a single-backend image.

**The sign is load-bearing, and that is the finding.** Several slots return
`int32_t` where a NON-NEGATIVE value is a count and a negative one is a status:
`take` returns bytes taken, `take_sequence` returns messages drained, `has_data`
returns 0/1. Adopting upstream's positive codes makes `1` ambiguous between "one
message" and `RMW_RET_ERROR`. So this is not a free rename; it is a choice:

* **(a) keep the negative codes**, declared as an RTOS deviation whose reason is
  the count-returning slots — cheap and honest, but a caller who knows upstream
  reads `-1` where they expect `1`;
* **(b) adopt upstream's values and split count from status**, making every
  count an OUT parameter — more churn, but then a value means the same thing on
  both sides of the seam.

**DECIDED 2026-08-23: (b).** Upstream already solved this the same way — every
one of its count-or-flag functions returns `rmw_ret_t` and passes the count as an
out-parameter, which is *why* its positive error codes work. So (b) is not our
invention competing with theirs; it is the reason their numbering is coherent,
and taking it removes a declared arg deviation on all 11 slots rather than adding
one.

Our 12 extra codes (`NO_DATA`, `WOULD_BLOCK`, `BUFFER_TOO_SMALL`,
`MESSAGE_TOO_LARGE`, `INCOMPATIBLE_ABI`, `NO_BACKEND`, `CONNECTION_FAILED`, …)
take an explicit **extension range at 1000+**, documented in `rmw_ret.h` as the
one place we knowingly add to upstream's namespace, so a future upstream code can
never collide. `NO_DATA` largely disappears: "nothing to take" becomes
`taken = false` with `RMW_RET_OK`, which is upstream's semantics.

### W3.d order — signatures first, values second

The two halves must not land together, and the order is forced. While a slot
still multiplexes count and status, flipping the values makes `1` ambiguous
between "one message" and `RMW_RET_ERROR`; there is no green intermediate state
in that direction. So:

* **Step A** — the 11 slots take upstream's shape: status in the return, count
  or flag in an out-parameter. Values stay negative. The tree is green at every
  point, and each slot's `< 0` callers move to `!= RMW_RET_OK` as it converts.
* **Step B** — flip the values and add the 1000+ range. Safe only because no
  slot multiplexes any more.

### The sweep, done before either step

`just rmw-ret-sign` lists every call site that tests a status by its sign — the
failure mode that is not a compile error, not a test failure, just error handling
that stops running. Today: **6 on status-only results** (fix before the flip) and
**9 on dual-return results** (fix with the slot).

Building it was itself instructive. The first version required the call and the
test to be near each other in a form it could parse, and reported **zero** — a
clean bill of health for the exact sweep it exists to produce. Real sites look
like a five-line `let rc = unsafe { (self.vtable.take.expect(…))( … ) };` followed
nine lines later by `if rc < 0`, behind a `let Some(f) = … else` guard. Every
widening was driven by a site verified BY HAND first; tuning the window until the
output looked tidy would have optimised for a quiet report rather than a complete
one. Two false-positive classes were removed the same way: bindgen's
`#[doc = "…"]` strings (which quote the `< 0` contract in prose) and bit-shifts
(`Self(1 << 0)` contains the characters `< 0`).

## W4 — feature completeness (71 symbols with no slot)

Ordered cheapest-and-most-useful first. Each wave moves the counter, so the
Status line above stays a measurement rather than a claim.

1. **The two pure functions** — `qos_profile_check_compatible`,
   `compare_gids_equal`. No transport, no allocation, no discovery; there is no
   RTOS argument for their absence.
2. **Service/client wake callbacks** (2) — `service_set_on_new_request_callback`,
   `client_set_on_new_response_callback`. We have the primitive for
   subscriptions only, so a service-heavy image polls where a
   subscription-heavy one sleeps.
3. **QoS read-back** (6 `*_get_actual_qos`) — one slot serving six upstream
   entry points. We bake the REQUESTED profile and never read back the GRANTED
   one, which on DDS is exactly what a consumer needs to answer "why is nothing
   arriving".
4. **The `layer` set moves into the vtable** (~23) — `wait`, guard conditions,
   serialize/deserialize, node create/destroy, init/shutdown. Needs a decision
   per item about what "generic over all backends" means for something
   currently answered above the seam; `wait` in particular, since
   `Executor::spin_once` IS our wait and a vtable `wait` would sit under it.
5. **Graph and matched counts** (15) — the big one. Every backend already tracks
   the state (`service_server_available`'s own doc comment says so: zenoh via
   matched queryables, Cyclone via built-in topic readers, XRCE not at all), and
   Cyclone's `graph.cpp` already holds node names and GIDs with no portable seam
   to reach them. A full graph cache costs RAM a 128 KiB target does not have —
   which argues for a NULL slot meaning `UNSUPPORTED`, the convention the vtable
   already has, not for absence from the ABI.
6. **`publisher_wait_for_all_acked`** (1) — an image that publishes and halts
   cannot currently know whether anything left the box.

## Booked by W4, owed by W5

Two commitments made when a slot landed. Both are the kind of debt that is only
visible at the moment it is incurred, so they are written down here rather than
left to be rediscovered.

### `rmw_node_t *` on the four `create_*` slots — **LANDED 2026-08-24**

`create_publisher` / `create_subscription` / `create_service` / `create_client`
take `const rmw_node_t *` as upstream does. Three things had to move, and the
order was forced:

**B1.a — `Executor::create_node` registered nothing.** It built a `NodeHandle`
and returned it without touching `self.nodes`, so two calls with one name gave
two nodes the executor had never heard of. `create_node_on_with_domain` had had
the dedup since phase-267; the plain path never got it. Without a registry the
`create_node` SLOT's contract ("once per distinct `(name, namespace)`") has
nothing making it true.

**B1.b — the slots were dead.** W4 landed `create_node` / `destroy_node` and
nothing ever called them. The shim now owns a node table and fires `create_node`
once per distinct pair.

**B1.c — `rmw_node_t` gained `session`.** This was the precondition, not a
convenience: upstream's node reaches its context that way and every
`rmw_create_*` depends on it. A node with no route to its session cannot be the
only argument those slots get. Checked against Humble's `rmw/types.h` in the
distrobox rather than assumed.

`CffiSession::entity_view` is GONE, with `session_node_name` and
`session_namespace` in the adapter. The W3.c "session not node" deviation is
retired rather than re-declared.

**Where the node table lives, and why that is not a detail.** The first attempt
put it in `CffiSession`. That grew every C and C++ `_opaque` buffer by ~544
bytes and tripped `nros-c`'s compile-time size guards (issue 0472) — the
machinery working exactly as designed and saying the bookkeeping did not belong
on an ABI surface. It moved to a static side table keyed by the session's
`backend_data`, which is the same shape `MESSAGE_INFO_TABLE` already uses for
the same reason, with the slots released on `close`. Bounded by
`NROS_RMW_MAX_NODES` (default 4, mirroring the executor).

**Still owed:** zenoh's `ensure_node_liveliness` still linear-scans its own
`per_node_liveliness`. Retiring it needs a `create_node` method on the Rust
`Rmw`/`Session` trait plus an adapter trampoline, so a Rust backend can be TOLD
about a node the way a C one now is. The slot and the table both exist; only
that trait hop is missing.

### `rmw_qos_profile_t` needs an UNKNOWN encoding — **LANDED 2026-08-24**

The booked item was a sentinel so the six `*_get_actual_qos` could stop being
all-or-nothing. Adding one meant looking at what the policy values actually
are, and they were **not upstream's**:

| value | upstream | ours (before) |
| --- | --- | --- |
| 0 | `*_SYSTEM_DEFAULT` | BEST_EFFORT / VOLATILE / KEEP_LAST |
| 1 | RELIABLE / TRANSIENT_LOCAL / **KEEP_LAST** | RELIABLE / TRANSIENT_LOCAL / **KEEP_ALL** |

`history == 1` meant KEEP_LAST to upstream and KEEP_ALL to us — the two
opposite answers to one question. Liveliness was worse: `MANUAL_BY_NODE` and
`MANUAL_BY_TOPIC` were **swapped** (2 and 3).

These values cross the ABI. `rmw_qos_profile_check_compatible` is a name
upstream owns and we export, and cyclonedds translates this struct into real
DDS QoS that a ROS peer matches against — so the swap was visible on the wire
and nowhere else. Same argument as W3.d step B made for the return codes:
where a value crosses the boundary, upstream's numbering is the only one that
cannot be wrong.

So B2 adopted upstream's numbering, which yields `SYSTEM_DEFAULT` and
`UNKNOWN` for free, and then spent them:

* the six `*_get_actual_qos` may now write `*_UNKNOWN` for a policy they cannot
  determine and return OK. `UNSUPPORTED` narrows to its literal meaning — no
  read-back at all.
* `RMW_QOS_COMPATIBILITY_WARNING` becomes REACHABLE. It was defined and
  unreachable because there was no undetermined policy to trigger it. An
  unknown is an absence, not a value, so it warns; a real clash still ERRORs,
  because softening that would hide something the caller can act on.

**What the renumbering broke, and how it was found.** Nothing failed to
compile. The Rust conversions were bare integers with the policy name in a
trailing comment (`reliability: 1, // RELIABLE`), the `qos_from_cffi` decoders
were `== 0` tests against a dense 0/1 encoding, and
`nros_orchestration_ir::qos_override` encoded liveliness as literals under a
comment reading "Discriminants of `nros_rmw::QosLivelinessPolicy`" — a comment
is not a binding. Every one of those kept compiling and started meaning a
different policy. They all name the constant or the variant now, and both ends
of the IR wire name the variant so they move together.

**Found on the way:** `node_metadata`'s `liveliness_json` mapped
`ManualByNode` to the string `"manual_by_topic"` — both variants produced the
same JSON, so node-level liveliness was reported as topic-level. Unrelated to
the renumbering, fixed with it.

**Deliberately NOT changed:** the user-facing C/C++ QoS enums
(`nros_qos_liveliness_t`, `QoS::Liveliness`) keep `MANUAL_BY_TOPIC = 2`. They
are our own API, not the RMW ABI, and `nros-c/src/qos.rs` maps them by NAME, so
the renumbering passes through them untouched. It does mean the repo now has
two enums where `2` names different policies — a trap, recorded here rather
than half-fixed.

### The init-options residue — **DECIDED 2026-08-24 (B3)**

This section used to read: *"`rmw_init_options_t` also carries
`security_options` and `discovery_options`. We answer neither. 'Init options:
declined' is about the init/copy/fini trio only."*

Both halves needed correcting before the question could be answered.
**`discovery_options` is not a Humble field** — it arrived in Iron, and our
recorded contract is Humble. And the real list is eight fields, not two. Two
header comments carried the same wrong list; both fixed.

Audited against Humble's `rmw/init_options.h`, read in the distrobox:

| upstream field | ours | verdict |
| --- | --- | --- |
| `domain_id` | `create_session`'s `domain_id` | carried |
| `implementation_identifier` | `get_implementation_identifier` slot + descriptor | answered elsewhere |
| `impl` | `rmw_session_t::backend_data` | answered elsewhere |
| `allocator` | — | declined ABI-wide; no allocator at this seam |
| `instance_id` | — | rcl-side process identity, not middleware behaviour |
| `security_options` | — | **declined**, on a target reason |
| `localhost_only` | — | **gap** → issue 0785 |
| `enclave` | — | **gap**, and it makes a grouping hollow → issue 0785 |

`security_options` is `{enforce_security, security_root_path}` — a DDS-SROS2
keystore FILESYSTEM path plus an enforce/permissive switch. Neither the
filesystem nor the DDS security plugin exists on these targets, and the one
backend that could honour it is the one where a caller can configure the
participant out of band. That reason is about the target, so the decline holds.

The `enclave` finding is the sharp one. `rmw_get_node_names_with_enclaves` is
counted as ANSWERED, grouped onto `get_node_names`. The grouping is right in
shape — upstream split the two names only because appending to a fixed
out-parameter list would break its ABI, and a visitor has no such list. It is
hollow in content: nothing in this ABI accepts an enclave, so the visitor's
`enclave` argument is structurally always NULL. A symbol we can only ever
answer with "nothing" sits in the report's answered column, and no gate can see
it.

## W5 — RTOS correctness: audit the declarations

Feature completeness makes the deviations the only thing left, so the last wave
is about them being *true*, not merely present. For each declared deviation:

* Does the stated constraint still hold? (`init_publisher_allocation` was
  declined because "pools are baked" — issue 0777 measured that false for four
  backends of five, so the decline needed a different reason, which it now has.)
* Is it declared at the narrowest scope? A deviation that applies to one backend
  should be a NULL slot on that backend, not an ABI-wide difference.
* Is the reason about the TARGET, or about our convenience? Only the first is a
  reason.

Two are already suspect and should be re-decided rather than inherited:

* **`set_log_severity`** — LANDED as a vtable slot (2026-08-24). The decline
  said "log level is a build-time constant (`nros_log`); a runtime setter
  implies a mutable global". Both clauses were false against the code as it
  actually stands: `nros_log::Logger::level` is already an `AtomicU8` with a
  public `set_level`, and the compile-time part is a CEILING that defaults open
  (`Severity::Trace` when no feature selects one). So the setter needed no new
  mutable state and no new global — it needed the slot. A probe of the three
  upstream backends found all three implement `rmw_set_log_severity` with a real
  body, unlike `rmw_get_serialized_message_size` which two of them stub, so the
  behaviour genuinely varies per backend and it is correctly a SLOT rather than
  a plain ABI function. Runtime half: `set_backend_log_severity()` applies to
  EVERY registered backend (an image can carry more than one, which upstream
  never has to handle) and reports `Unsupported` only when none exposes the
  slot.
* **`subscription_{set,get}_content_filter`** — LANDED as slots (2026-08-24).
  Declined as DDS-only, which is true and is an argument for a NULL SLOT rather
  than for absence: a declined symbol is missing from the ABI for the backend
  that CAN answer too. The **network-flow pair** turned out to have the same
  shape — "zenoh-pico/XRCE have no such notion" is true of those two and silent
  about Cyclone, so the reason was scoped to the ABI when it belonged on a
  backend. All four take a VISITOR where upstream takes an
  `rcutils_allocator_t *` plus an allocating array/options struct, matching what
  the graph slots already do for the same reason. Contract 67 → 71,
  declined 21 → 17.

At the end of W5, `rmw-abi-shape --check` joins the `just check` line and the
claim "feature complete against RMW, modulo declared RTOS deviations" becomes
something CI re-proves on every commit rather than a sentence in a README.

## The complete work inventory

Every item needed to call the RMW API done. Nothing below is implicit: if a
thing must happen, it has a row, and the counters that verify it are named. The
Status line at the top is derived from these, so it stays a measurement.

Legend: **done** / **open**. Counts in parentheses are what the tools report
today.

### W3.a — types lose the vendor prefix (**COMPLETE**)

| item | state |
| --- | --- |
| `nros_rmw_ret_t` -> `rmw_ret_t` | **done** |
| `nros_rmw_session_t` -> `rmw_session_t` (10 uses) | **done** |
| `nros_rmw_subscription_t` -> `rmw_subscription_t` (10) | **done** |
| `nros_rmw_publisher_t` -> `rmw_publisher_t` (9) | **done** |
| `nros_rmw_service_t` -> `rmw_service_t` (5) | **done** |
| `nros_rmw_client_t` -> `rmw_client_t` (5) | **done** |
| `nros_rmw_qos_t` -> `rmw_qos_profile_t` (4) | **done** |
| `nros_rmw_event_kind_t` -> `rmw_event_type_t` (2) | **done** |
| `nros_rmw_event_callback_t` -> `rmw_event_callback_t` (2) | **done** |
| `nros_rmw_{publisher,subscription}_options_t` -> upstream names (2) | **done** |
| `#error` guard on `RMW_RMW_H_` so our header and upstream's cannot share a TU | **done** |
| the `NROS_RMW_RET_*` constant names follow their type | **done** — the CONSTANTS keep the `NROS_` prefix deliberately: `rmw_ret_t` is upstream's type, but W3.d step B gave the values upstream's numbering plus 13 of our own above `NROS_RMW_RET_EXTENSION_BASE`, so an unprefixed spelling would claim a name upstream owns |

Every row here read `` `rmw_x_t` -> `rmw_x_t` | open `` until 2026-08-24: the
tree-wide rename edited this doc too, collapsing both columns onto the new
spelling, and the `open` states were never flipped. The section header has said
**COMPLETE** since the work landed, so the table contradicted its own heading —
which is why nobody reading the heading noticed the rows.

Verified by: `just rmw-abi-shape` -> `vendor-named types in sigs: 0`.

### W3.b — slots take upstream names (**COMPLETE**)

`take`, `take_request`, `take_response`, `take_sequence`,
`take_loaned_message`, `borrow_loaned_message`, `publish_loaned_message`,
`return_loaned_message_from_{publisher,subscription}`, `publish`,
`send_request`, `send_response`, `publisher_assert_liveliness`,
`{publisher,subscription}_event_init` — plus
`service_server_is_available`, **done**.

Verified by: `UNDECLARED extra slots: 0`.

### W3.c — argument deviations declared (**COMPLETE**)

One `ARG_DEVIATIONS` entry per slot whose parameters differ from upstream, each
naming the target constraint. `service_server_is_available` **done**; the rest
land with their rename in W3.b. The four systematic classes (session vs node,
baked type strings vs typesupport, OUT parameter vs returned pointer, no
allocation argument) are described above.

Verified by: `slots present, args differ: 0` with every difference in the table.

### W3.d step A — no slot multiplexes a count with a status (**COMPLETE, 11 of 11**)

| slot | state |
| --- | --- |
| `service_server_available` -> `service_server_is_available` | **done** |
| `has_data` | **done** |
| `has_request` | **done** |
| `subscription_supports_in_place` | **done** |
| `process_raw_in_place` | **done** — first slot to retire `NO_DATA` |
| `try_recv_raw` -> `take` | **done** — second slot to retire `NO_DATA` |
| `try_recv_request` -> `take_request` | **done** |
| `try_recv_reply_raw` -> `take_response` | **done** |
| `try_recv_sequence` -> `take_sequence` | **done** |
| `sub_borrow` -> `take_loaned_message` | **done** |
| `next_deadline_ms` | **done** — took the out-parameter; see the note below |

Verified by: no `int32_t (*slot)` remains in `rmw_vtable.h` — checked, none do —
and `just rmw-ret-sign` reports 0 sign tests in BOTH classes, which is step B's
precondition.

`next_deadline_ms` was the one member step B did not force: its negative return
was a "no deadline" SENTINEL rather than an error code, so nothing would have
collided. Converted anyway, because it had the shape every other conversion
found a silent failure in — a backend that FAILED to compute its deadline
returned `-1` and was read as "quiet link", which is precisely the reading that
makes the executor sleep longer. It now has an error channel it never had.

### W3.d step B — the values flip (**COMPLETE 2026-08-23**)

| item | state |
| --- | --- |
| adopt `RMW_RET_OK 0 / ERROR 1 / TIMEOUT 2 / UNSUPPORTED 3 / BAD_ALLOC 10 / INVALID_ARGUMENT 11` | **done** |
| our 13 extra codes move to the 1000+ extension range, via `NROS_RMW_RET_EXTENSION_BASE` | **done** |
| retire `NO_DATA` where `taken = false` + OK now says it | **done** — the constant stays for the backends' own internal helpers, but no vtable slot returns it |
| fix the status-only sign tests the audit lists | **done** — `just rmw-ret-sign` is 0/0 |
| decide `INCORRECT_RMW_IMPLEMENTATION` | **done** — DEFINED at upstream's 12 though unreachable here, so the value can never be reused for one of ours; that is the point of pinning to upstream's numbering |

Verified by: `just rmw-ret-sign` -> both counts 0 (it is).

### W4 — feature completeness (**COMPLETE 2026-08-24**; the table below is the PLAN, 70 slots missing when it was written)

| group | count | slots |
| --- | ---: | --- |
| pure functions | 2 | `qos_profile_check_compatible`, `compare_gids_equal` |
| service/client wake callbacks | 2 | `service_set_on_new_request_callback`, `client_set_on_new_response_callback` |
| QoS read-back | 6 | the `*_get_actual_qos` family |
| data-plane names still to add | 13 | `publish`, `take*`, loaned family, `*_event_init`, `take_event`, `event_set_callback`, `subscription_set_on_new_message_callback` — these ARRIVE with W3.b's renames rather than as new work |
| session / context / node lifecycle | 8 | `init`, `shutdown`, `context_fini`, `init_options_{init,copy,fini}`, `create_node`, `destroy_node` |
| wait set + guard conditions | 6 | `wait`, `create_wait_set`, `destroy_wait_set`, `{create,destroy,trigger}_guard_condition` |
| serialization | 6 | `serialize`, `deserialize`, `get_serialized_message_size`, `publish_serialized_message`, `take_serialized_message{,_with_info}` |
| identity / introspection of the impl | 3 | `get_implementation_identifier`, `get_serialization_format`, `feature_supported` |
| graph + matched counts | 15 | `get_node_names{,_with_enclaves}`, the four `*_by_node`, `get_{publishers,subscriptions}_info_by_topic`, `get_{topic,service}_names_and_types`, `count_{publishers,subscribers}`, `node_get_graph_guard_condition`, `{publisher,subscription}_count_matched_*`, `get_gid_for_publisher` |
| clean shutdown | 1 | `publisher_wait_for_all_acked` |
| with-info takes | 3 | `take_with_info`, `take_loaned_message_with_info`, `take_serialized_message_with_info` |

Verified by: `no slot at all: 0`.

Each group needs a decision recorded before its slots land, and two are not
mechanical: the **wait set** group (our `Executor::spin_once` IS the wait, so a
vtable `wait` sits UNDER it and the executor becomes its caller) and the
**graph** group (a full cache costs RAM a 128 KiB target does not have, so the
answer is an optional slot with `UNSUPPORTED`, not an unconditional one).

### W4 decisions as landed (2026-08-23/24)

The table above is the PLAN. What follows is what was decided, per group, once
each was actually looked at — the two differ, and where they differ the reason
is recorded rather than the table quietly edited.

| group | planned | landed |
| --- | --- | --- |
| identity / introspection | 3 slots | **3 slots**, exact parity |
| graph + matched counts | 15 slots | **12 slots** — `get_node_names` answers two upstream names |
| QoS read-back | 6 slots | **6 slots**, exact parity |
| clean shutdown | 1 slot | **1 slot** |
| with-info takes | 3 slots | **2 slots** + 1 grouped |
| wake callbacks | 2 slots | **3 slots** — the subscription one was recorded as covered and was not |
| session / node lifecycle | 8 slots | **2 slots**, 3 grouped, 3 declined |
| wait set + guard conditions | 6 slots | **0 slots**, all 6 declined |
| serialization | 6 slots | **0 slots**, 2 declined, 3 grouped, 1 reclassified as a gap |
| pure functions | 2 slots | **open** — see below |

#### Serialization: no slots, and the reason is that our seam already carries CDR

`publish` and `take` take and yield BYTES, and those bytes are CDR — written by
`nros-serdes` above the vtable. So:

* **`serialize` / `deserialize` are declined.** CDR for an IDL type is fixed by
  ROS interop, so a per-backend answer would be a DEFECT rather than a feature —
  two backends disagreeing about how a `std_msgs/String` encodes is a bug with
  three places to fix it. Both upstream signatures also name two things this ABI
  has already declined: a `rosidl_message_type_support_t *`, and
  `rmw_serialized_message_t`, which is an `rcutils_uint8_array_t` carrying an
  ALLOCATOR, at a seam with no allocator.

* **`publish_serialized_message`, `take_serialized_message` and
  `take_serialized_message_with_info` are GROUPED** onto `publish`, `take` and
  `take_with_info`. A separate slot could only ever forward to the same one, and
  a slot whose only possible body is a forward is a null decision. The
  mechanical name rule attached our slots to the wrong namesake — semantically
  ours ARE the serialized variants — and renaming them would be worse than
  recording the grouping.

* **`get_serialized_message_size` moved from `layer` to `gap`.** Its stated
  reason, "generated per type; the bound is baked", was false and was checked:
  `nros-serdes` declares only `serialize` / `deserialize` /
  `deserialize_borrowed`, no generated crate emits a size constant, and buffers
  are sized by env knobs (`NROS_SUBSCRIPTION_BUFFER_SIZE`). `report_dropped_take`
  says outright that it cannot name the size that would have worked. A real
  bound would let a dropped take say how much room it needed.

The ABI-wide deviation this exposes, previously written down nowhere: **a
backend cannot choose its own wire representation**, because the encapsulation
header is written above the seam. That is a genuine constraint of this design,
not an oversight, and it is why the grouping above is honest rather than a
convenience.

#### The pure functions: plain C, not slots

`rmw_qos_profile_check_compatible` and `rmw_compare_gids_equal` are declared in
`nros/rmw_entity.h` and defined once in `nros-rmw-cffi`. They are NOT vtable
slots, which contradicts the campaign's "all RMW functions go into the vtable"
and is worth the exception because the rule's own purpose says so: a slot is the
mechanism for letting backends DIFFER, and these two must not.

Maintainer's framing, which is the constraint recorded here: *they must be
independent from RMW choices and behave the same regardless of the backends.*

Four reasons, strongest first:

1. **A per-backend answer would be a defect.** Both compute over types this ABI
   defines. Two backends disagreeing about whether a QoS pair is compatible, or
   whether two gids name the same entity, is a bug with as many places to fix it
   as there are backends.
2. **The useful call sites have no vtable.** QoS compatibility is wanted at
   `create_*` time — that is what produces `INCOMPATIBLE_QOS` — and in codegen'd
   validation and host tooling with no session. Neither function takes an
   entity, so a slot would force a caller to invent a session, and neither could
   be called BEFORE a backend registers, which is when create-time validation
   runs.
3. **Upstream is not evidence for a slot.** `rmw_qos_profile_check_compatible`
   lives in `rmw/qos_profiles.h` and is defined by librmw itself; it is in the
   implementation contract only because each `librmw_*_cpp.so` statically links
   librmw and re-exports it. Plugin packaging, not semantics — and we load no
   plugin.
4. **Precedent:** `nros_rmw_cffi_register_named` is declared here and defined in
   Rust with `#[no_mangle]`, so `nros-rmw-abi` stays a header-only INTERFACE
   target. (`static inline` is worse: bindgen does not emit it, so RFC-0054
   would force a SECOND implementation — reason 1 again.)

**The reason string, with no allocator.** Upstream solved ownership (caller's
`char *` plus size, copied verbatim, so no arg deviation) but not FORMATTING:
its implementations `snprintf`, which drags the printf engine into images that
excluded it. So the reason is SELECTED, never formatted — one
`static const char[]` per clash bit, appended by a bounded copy. It splits in
two so the flash cost is opt-in: `nros_rmw_qos_incompatibility_mask` returns the
verdict plus a machine-readable bitmask and references no strings;
`rmw_qos_profile_check_compatible` is mask plus render. Truncation is NOT
failure — always NUL-terminate and still write the verdict, because
`BUFFER_TOO_SMALL` would cost a small-buffer caller the half of the answer that
matters.

**`ABI_FUNCTIONS` verifies rather than records.** `rmw-abi-shape.py` greps the
ABI headers for each declaration, so an entry whose function was never declared
is reported as MISSING — a table that only recorded intent would be the
vacuous-test failure one level up. Mutation-checked: renaming an entry to a
function nobody declares puts the symbol straight back in the gap list.

#### The grouping mechanism, and its guard

`GROUPED_SYMBOLS` in `scripts/rmw-abi-shape.py` is the one deliberate exception
to the mechanical name rule, so it carries the same burden as any declared
deviation: a reason per entry, and a `--self-test` assertion that every alias
TARGET is really a slot. Without that last part an alias is a way to make a
MISSING slot invisible, which is the opposite of what the tool exists for.
Mutation-checked: pointing one alias at a non-existent slot fails the self-test.

### W5 — RTOS correctness audit (**COMPLETE 2026-08-25**)

| item | state |
| --- | --- |
| every `ADDED` slot's reason re-checked against the target constraint | **landed** — 11 → 9 (two were never additions), 3 reasons rewritten, 2 slots filed as redundant (0781) |
| every `ARG_DEVIATIONS` reason re-checked | **landed** — the `const` class (15 + 3 the first sweep missed) and the `void`-return class (6) FIXED not re-declared; 10 false claims rewritten |
| every `declined` reason re-checked; narrowest scope preferred (a per-backend NULL slot beats an ABI-wide absence) | **landed** — 21 → 14: 4 became slots, `event_set_callback` was a grouping, `serialize`/`deserialize` were `layer`, `take_event` filed as 0780 |
| re-decide `set_log_severity` — declined for a policy choice dressed as a constraint | **landed** — slot + `set_backend_log_severity()`; both clauses of the decline were false (`Logger::level` is an `AtomicU8`, the compile-time part is an open ceiling) |
| re-decide `subscription_{set,get}_content_filter` — a NULL slot costs one pointer and lets a DDS backend answer | **landed** — both slots, plus the network-flow pair, whose reason had the same shape |
| `rmw-abi-shape --check` joins the `just check` line | **landed** — `just check-rmw-abi-shape`, self-test + check, on the fast line |
| parity MAP cross-checked against the header, both directions | **landed** — the MAP was stale in two ways at once (see below) |

### W5's audit, run in full (2026-08-24)

Three parallel read-only audits over the three declaration tables. The headline
is not how many reasons were wrong — it is that **every one of them passed
`rmw-abi-shape --check`**, because that gate asks whether a difference is
DECLARED and cannot ask whether the declaration is true. That is the whole
argument for a reason-by-reason pass.

What it found, beyond the rewordings:

* **The const class had recurred within a week.** The 15-slot sweep compared
  arguments POSITIONALLY, and three slots put the handle at a different index
  than upstream (`{publisher,subscription}_event_init` lead with an
  `rmw_event_t *`, `service_server_is_available` with a `const rmw_node_t *`),
  so `zip()` lined the handle up against something else and reported nothing.
  The class fix stopped one short of the class for a reason unrelated to the
  class. Now matched BY TYPE, and gated.
* **`create_session` / `destroy_session` were in `ADDED` and in
  `GROUPED_SYMBOLS` at once** — their own reasons said they had upstream
  equivalents, under a comment saying additions have none. They were parked
  there because `expected` did not union the grouped targets, so removing them
  made both report as undeclared extras. Fixed at the union. 11 additions → 9.
* **`rmw_subscription_set_on_new_message_callback` still pointed at
  `set_wake_callback`**, a record the header itself calls untrue. The MAP
  cross-check passed it because it verified the detail names *a* slot, not the
  *right* one — the W3.b drift class surviving inside the gate built to stop
  it. Now checked against the mechanical name.
* **Issue 0777's replacement reason was ALSO false.** It said upstream
  "pre-sizes a per-entity `rcutils_allocator_t` the caller owns". Humble's
  `rmw/types.h` says `rmw_publisher_allocation_t` is
  `{const char *implementation_identifier; void *data;}` — no allocator. Two
  wrong reasons for one parameter in one week, both plausible, both unchecked.
  The one that survives: nothing here can PRODUCE that handle, because
  `rmw_init_publisher_allocation`'s other two parameters are declined
  ABI-wide.
* **Three findings too large to fold in**, filed: 0780 (`take_event` declined
  on a premise this ABI contradicts; cyclone cannot deliver a status event at
  all), 0781 (one in-place capability over five slots), 0782 (`publish_streamed`
  `malloc`s the whole payload on the target class it exists to help).

### Two declared deviations were fixed rather than re-justified

**`const` on the handle — 15 slots.** Ours took `rmw_publisher_t *` where
upstream takes `const rmw_publisher_t *`, on `publish`, `take`,
`take_request`, `take_response`, `take_sequence`, `take_with_info`,
`send_request`, `send_response`, `publisher_assert_liveliness`, both loan
borrows, both loan returns and `take_loaned_message{,_with_info}`. The
deviation table already called this "the cheapest deviation to REMOVE rather
than declare"; W5 checked whether any backend actually writes through that
pointer. **None does** — every `*_data_mut` use in the Rust adapter is inside a
`destroy_*` trampoline, and no C or C++ backend touches the struct. So the
deviation described nothing. Now `const`, which is also what says out loud that
the handle is RUNTIME-owned: a backend that writes it is corrupting state it
does not own, and the compiler now says so.

**`void` returns — 6 slots.** `destroy_{publisher,subscription,service,client}`
and both `return_loaned_message_from_*` returned nothing. The recorded reason
was "cleanup is best-effort", which describes the behaviour without justifying
it; upstream returns `rmw_ret_t` from all six. A backend that cannot release a
handle — a double destroy, a token from another publisher, a Cyclone entity
that refuses to delete — had no way to say so, and the runtime reported a
success it had not verified. All six now return a status, and the backends
report a real one: Cyclone propagates `dds_delete` failures (they were
`(void)`-cast), uORB propagates `orb_unadvertise` / `orb_unsubscribe`, and XRCE
reports a request it could not BUFFER while documenting that the agent's own
verdict is deliberately not awaited at close time. `Drop` cannot propagate, so
the shim logs through `nros_log` — a leak with no message resurfaces as an
allocation failure with no provenance.

Slots identical to upstream: **17 → 20**. The headline moves less than the work
does, because most of these 15 still differ on the payload axis (bytes, not a
typed `void *`) — a real deviation that stays declared.

### The parity MAP had drifted 45 entries, in two directions

Wiring `--check` to `just check` surfaced this rather than the campaign
noticing it: `rmw-api-parity` reported **26 gaps** while `rmw-abi-shape`
reported **one symbol with no slot**. Two tools over one question, both green,
disagreeing by 25 symbols.

* **W3.b renamed 17 slots** (`try_recv_raw` → `take`, `send_reply` →
  `send_response`, `pub_loan` → `borrow_loaned_message`, …). Buckets stayed
  correct; the details named slots that no longer existed.
* **W4 landed 28 slots** — the entire graph/introspection family, all six
  `*_get_actual_qos`, `publisher_wait_for_all_acked`, both service-side
  callbacks, `feature_supported`, `get_implementation_identifier`,
  `get_serialization_format` — and the MAP still read `("gap", "no vtable
  slot")` for every one.

The report is the artifact people quote for "what do we answer?", so a stale
one is worse than no table. Structural fix: `check_against_vtable()` in
`rmw-api-parity.py` imports `rmw-abi-shape`'s own header parser (a SECOND
parser for one header is how they would drift a third time) and fails both
directions — a `vtable` detail naming no real slot, and a non-`vtable` bucket
for a symbol whose slot exists. Runs in `--self-test` as well as `--check`,
against the real header rather than a fixture, because both drifts WERE the
fixture and the header diverging.

Post-fix: vtable 64, layer 2, declined 21, gap 1.

### A found bug, filed: issue 0779

Building the `lending` feature to verify the loan slots turned up two test
files that **had not compiled since W3.d** — `noop_hasd` still had the old
one-argument shape and `NROS_RMW_RET_ERROR` was never imported. Nothing noticed
because nothing built them: they are gated by a crate-level
`#![cfg(feature = "lending")]`, which cargo happily compiles to an EMPTY test
binary when the feature is off. That is a stronger false signal than issue
0652's `required-features` (which at least does not build): nextest runs the
binary and reports it green over zero cases.

Six features, fifteen files, none in a lane.
`check-required-features-reachable` now scans both mechanisms; `lending` is
wired and green (48 tests vs 44 in `nros-rmw-cffi`, plus 2 in `nros-rmw-zenoh`);
the other five are a dated backlog. → issue 0779.

### Deferral needs a name on it

`--check` could not join the `just check` line while
`get_serialized_message_size` counted as a hard miss — and "just exempt it"
would have made the gate worthless. Instead a `gap` whose reason names a
**tracked issue id** (`issue 0776`) is reported as `deferred, issue tracked`
and does not fail. A bare "not yet" still reds. Self-tested both ways.

### Cross-cutting, every wave

| item | why |
| --- | --- |
| `scripts/gen-abi-bindings.sh` + commit both halves | RFC-0054: the header is the SSoT; `check-abi-bindings` compares the COMMITTED blob, so it stays red until the regenerated file is committed |
| update `MAP` / `ADDED` / `ARG_DEVIATIONS` in the tools | the counters are how the Status line stays honest |
| run `just check`, not a per-crate command | `check-test-targets` runs clippy over test targets with `-D warnings`; `cargo test -p <crate>` does not, and that is how `(0) != 0` reached a commit |
| per-site edits for stub bodies | a regex pass mangled a multi-statement stub into a file that would not parse, and emitted `(0) != 0` in 13 places |

## What the campaign left behind, and who owns it

All five waves are complete. These are follow-ups, filed as issues so they do
not live only in a phase doc nobody re-reads. None of them blocks the parity
claim; each is a thing the audit FOUND while checking it.

| issue | what | size |
| --- | --- | --- |
| 0776 | no serialized-size bound — the one contract symbol with no slot, deferred with its issue id so `--check` can be honest about it. Design + work items in phase-380 | large |
| 0778 | cyclonedds still holds ONE outstanding request; the abandon is now visible rather than silent. Needs a pending TABLE mirroring the server's `slots`. Also: `take_request`/`send_response`'s `int64_t` is a slot INDEX there, and an unanswered request leaks one | medium |
| 0780 | `take_event` declined on two clauses that both fail; cyclonedds cannot deliver a QoS status event at all | medium |
| 0781 | one in-place-dispatch capability spread over five slots; a probe that re-encodes what slot nullity already says | medium |
| 0782 | `publish_streamed` exists to avoid a `.bss` staging buffer and XRCE `malloc`s the whole payload | medium |
| 0785 | `create_session` carries one of Humble's eight `rmw_init_options_t` fields; `localhost_only` and `enclave` are gaps, and the second makes a GROUPED answer hollow | medium |
| 0777 | the "pools are baked" clause and its first replacement were both false; the CAPABILITY question (cyclone allocates twice per message with a knowable size) stays open | small + a design question |
| 0779 | fifteen test files behind a `#![cfg(feature)]` no lane enables; `lending` wired, five features baselined | testing |
| 0767 | `publish_streamed`'s two tests share process globals | testing |

Owed on top of those, and not an issue because it is a process step: **the xrce
and uORB backends have no host lane.** Several W5 changes edit their C sources
and were verified by reading and by the C ABI's own type checking, never by a
compiler. They land in tier 2.

## Running it

```
just check-rmw-api-parity     # is every contract symbol CLASSIFIED?
just check-rmw-abi-shape      # does the vtable MIRROR it — name, args, return?
just check-rmw-ret-sign       # does anything still multiplex a length with a status?
just rmw-abi-shape            # the same report, without the gate

# Regenerate the recorded upstream data (needs ROS — run in the distrobox):
scripts/rmw-api-inventory.py --signatures > docs/reference/rmw-implementation-signatures.txt
scripts/rmw-api-parity.py --contract      > docs/reference/rmw-implementation-contract.txt
```

All three checks are on the `just check` fast line and all three self-test.
