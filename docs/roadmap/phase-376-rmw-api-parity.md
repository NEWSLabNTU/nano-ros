# Phase 376 — the RMW ABI campaign: generic naming, feature completeness, RTOS correctness

**Status (2026-08-23). W3 IS COMPLETE — 24 of 79 slots match upstream's name
AND argument list, 0 undeclared extras, 0 vendor-named types, every remaining
difference declared with its RTOS reason. What is left of the campaign is W4
(55 slots upstream has that we do not) and W5 (auditing the deviations).
W3.d is COMPLETE — no vtable slot multiplexes a count
with a status, and the return VALUES are upstream's. W1 and W2 landed as
MEASUREMENT: the contract is derived
from real implementations, and three automated checks report how far the vtable
is from mirroring it. Today: 0 of 79 slots match name and args, 10 vendor-named
types in signatures, 71 contract symbols with no slot. W3+ (the migration) is
not started.**

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

| ours | becomes | uses |
| --- | --- | ---: |
| `rmw_session_t` | `rmw_session_t` | 10 |
| `rmw_subscription_t` | `rmw_subscription_t` | 10 |
| `rmw_publisher_t` | `rmw_publisher_t` | 9 |
| `rmw_service_t` | `rmw_service_t` | 5 |
| `rmw_client_t` | `rmw_client_t` | 5 |
| `rmw_qos_profile_t` | `rmw_qos_profile_t` | 4 |
| `rmw_event_type_t` | `rmw_event_type_t` | 2 |
| `rmw_event_callback_t` | `rmw_event_callback_t` | 2 |
| `rmw_publisher_options_t` | `rmw_publisher_options_t` | 1 |
| `rmw_subscription_options_t` | `rmw_subscription_options_t` | 1 |
| `rmw_ret_t` | `rmw_ret_t` | every slot |

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
| returns `rmw_publisher_t *` | returns `rmw_ret_t`, entity is an OUT param | no runtime allocation: the caller owns the storage |
| `rmw_publisher_allocation_t *` | absent | pools are baked; nothing to pre-size |

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

### `rmw_node_t *` on the four `create_*` slots

`create_node` / `destroy_node` landed in W4 on the strength of a finding: the
W3.c deviation reason "an image opens ONE session; upstream's context/node split
has no target-side meaning" is HALF FALSE. The session half holds. The node half
is contradicted by our own code — `Executor` keeps a node table, and
`CffiSession::entity_view` exists solely to fabricate a per-call session
carrying the entity's owning-node identity ("one session can host N graph
nodes", its own comment), from which the zenoh backend re-derives a node
registry by linear-scanning declared tokens.

So node identity ALREADY reaches the backend, by a side channel. The end state
is `create_publisher` / `create_subscription` / `create_service` /
`create_client` taking `rmw_node_t *` as upstream does, which RETIRES the
`entity_view` fabrication and retires the W3.c deviation rather than adding to
it. Until then node identity arrives two ways at once, which is the shape that
has cost this tree three FFI-mirror bugs.

**Prerequisite, not a tidy-up:** plain `Executor::create_node(name)` registers
nothing in `self.nodes` (only `create_node_on_with_domain` dedups), so it can
hand out two handles of one name. The `create_node` slot's contract says the
runtime calls it once per distinct `(name, namespace_)`; without this the
backend gets duplicate declarations and must keep the per-backend dedup registry
the slot exists to delete.

### `rmw_qos_profile_t` needs an UNKNOWN encoding

The six `*_get_actual_qos` slots are ALL-OR-NOTHING: a backend that can
determine four policies and not the fifth returns `UNSUPPORTED` and writes
nothing. That is the honest contract given the struct we have — there is no
`UNKNOWN` / `SYSTEM_DEFAULT` value, so a partial answer would be
indistinguishable from a confident one. Adding those sentinels would let a
backend report what it knows, and would also make upstream's
`RMW_QOS_COMPATIBILITY_WARNING` reachable.

### Residue, so it is not mistaken for decided

`rmw_init_options_t` also carries `security_options` and `discovery_options`. We
answer neither. "Init options: declined" is about the init/copy/fini trio only.

## W5 — RTOS correctness: audit the declarations

Feature completeness makes the deviations the only thing left, so the last wave
is about them being *true*, not merely present. For each declared deviation:

* Does the stated constraint still hold? (`init_publisher_allocation` is
  declined because pools are baked — is that still true of every backend?)
* Is it declared at the narrowest scope? A deviation that applies to one backend
  should be a NULL slot on that backend, not an ABI-wide difference.
* Is the reason about the TARGET, or about our convenience? Only the first is a
  reason.

Two are already suspect and should be re-decided rather than inherited:

* **`set_log_severity`** — declined because the log level is a build-time
  constant. That is a policy choice, not a constraint; a runtime setter is
  possible and the reason as written does not carry.
* **`subscription_{set,get}_content_filter`** — declined as DDS-only. True, but
  a NULL slot returning `UNSUPPORTED` costs one pointer and lets a DDS backend
  answer, which is strictly better than absence from the ABI.

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
| `rmw_ret_t` -> `rmw_ret_t` | open |
| `rmw_session_t` -> `rmw_session_t` (10 uses) | open |
| `rmw_subscription_t` -> `rmw_subscription_t` (10) | open |
| `rmw_publisher_t` -> `rmw_publisher_t` (9) | open |
| `rmw_service_t` -> `rmw_service_t` (5) | open |
| `rmw_client_t` -> `rmw_client_t` (5) | open |
| `rmw_qos_profile_t` -> `rmw_qos_profile_t` (4) | open |
| `rmw_event_type_t` -> `rmw_event_type_t` (2) | open |
| `rmw_event_callback_t` -> `rmw_event_callback_t` (2) | open |
| `nros_rmw_{publisher,subscription}_options_t` -> upstream names (2) | open |
| `#error` guard on `RMW_RMW_H_` so our header and upstream's cannot share a TU | open |
| the `NROS_RMW_RET_*` constant names follow their type | open |

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

### W4 — feature completeness (open, 70 slots with no slot today)

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

### W5 — RTOS correctness audit (open)

| item | state |
| --- | --- |
| every `ADDED` slot's reason re-checked against the target constraint | open |
| every `ARG_DEVIATIONS` reason re-checked | open |
| every `declined` reason re-checked; narrowest scope preferred (a per-backend NULL slot beats an ABI-wide absence) | open |
| re-decide `set_log_severity` — declined for a policy choice dressed as a constraint | open |
| re-decide `subscription_{set,get}_content_filter` — a NULL slot costs one pointer and lets a DDS backend answer | open |
| `rmw-abi-shape --check` joins the `just check` line | open |

### Cross-cutting, every wave

| item | why |
| --- | --- |
| `scripts/gen-abi-bindings.sh` + commit both halves | RFC-0054: the header is the SSoT; `check-abi-bindings` compares the COMMITTED blob, so it stays red until the regenerated file is committed |
| update `MAP` / `ADDED` / `ARG_DEVIATIONS` in the tools | the counters are how the Status line stays honest |
| run `just check`, not a per-crate command | `check-test-targets` runs clippy over test targets with `-D warnings`; `cargo test -p <crate>` does not, and that is how `(0) != 0` reached a commit |
| per-site edits for stub bodies | a regex pass mangled a multi-statement stub into a file that would not parse, and emitted `(0) != 0` in 13 places |

## Running it



```
just check-rmw-api-parity                  # classification (gates today)
just rmw-abi-shape                         # name / args / vendor-prefix report
scripts/rmw-abi-shape.py --check           # the future gate
scripts/rmw-api-inventory.py --signatures  # regenerate the recorded upstream data (in the box)
```
