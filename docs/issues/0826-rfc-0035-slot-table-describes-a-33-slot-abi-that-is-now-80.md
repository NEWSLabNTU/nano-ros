---
id: 826
title: "RFC-0035's slot table documents a 33-slot vtable that is now 80 slots:
  17 of the 36 names it lists no longer exist, and 61 real slots are absent"
status: open
type: bug
area: rmw, docs
related: [rfc-0035, phase-376, issue-0800, phase-379]
---

## Problem

`docs/design/0035-rmw-vtable-abi.md` carries a numbered slot table — "slot 1–3
`create_session`, `destroy_session`, `drive_io`", and so on to slot 33. It is
the document a backend author reads to learn the ABI's shape.

Measured against `packages/core/nros-rmw-abi/include/nros/rmw_vtable.h`:

| | |
| --- | ---: |
| slots the header declares | **80** |
| rows in the table | 17 |
| distinct slot names the table lists | 36 |
| of those, **names that do not exist** | **17** |
| header slots the table **never mentions** | **61** |

The seventeen dead names, with what they became:

```
publish_raw                  -> publish
try_recv_raw                 -> take
try_recv_request             -> take_request
send_reply                   -> send_response
send_request_raw             -> send_request
try_recv_reply_raw           -> take_response
try_recv_sequence            -> take_sequence
service_server_available     -> service_server_is_available
pub_loan                     -> borrow_loaned_message
pub_commit                   -> publish_loaned_message
pub_discard                  -> return_loaned_message_from_publisher
sub_borrow                   -> take_loaned_message
sub_release                  -> return_loaned_message_from_subscription
register_publisher_event     -> publisher_event_init
register_subscription_event  -> subscription_event_init
assert_publisher_liveliness  -> publisher_assert_liveliness
call_raw                     -> DELETED (the table itself says so)
```

Most of that renaming happened in **phase-376 W3.b**, which moved the vtable to
upstream's `rmw_*` vocabulary. The header moved; this table did not.

## Why it is filed rather than swept

The obvious fix — rename the seventeen — **would make it worse.** A table that
names only current slots reads as authoritative while still being silent about
**61 of the 80**. Today it is visibly stale, which at least warns the reader.
Half-fixing it removes the warning and keeps the defect.

The same reasoning kept the `send_reply` -> `send_response` sweep from touching
it: fixing one of six stale names in a copy-this document is the
fix-the-site-not-the-class antipattern, and this table needs regenerating from
the header rather than editing.

## Related, and already fixed

`book/src/porting/custom-rmw.md` had the sharper version of this — its C vtable
initialiser assigned **6 slots that do not exist**, so the example could not
compile. Fixed in `9b574e974`, along with `book/src/design/rmw-vs-upstream.md`
and `nros-rmw-abi/docs/mainpage.md`. This RFC was left because its problem is
different in kind: not wrong names in working prose, but a table whose whole
structure predates the ABI it documents.

Note RFC-0035 is `status: Draft`, so nothing has promoted it against the
header.

## Direction

1. **Generate the table, do not maintain it.** `rmw_vtable.h` is the SSoT and
   already carries per-slot doc comments; the repo has precedent for generated
   reference tables (`scripts/gen-board-support-table.py`,
   `gen-pool-inventory.py`, `gen-rmw-feature-matrix.py`), each with a
   `--check` mode wired into a lane so drift fails rather than accumulates.
2. If it stays hand-written, it needs a gate — the lesson of issue 0800, where
   "the slot exists" was being read as "the capability works" until something
   checked.
3. Decide what the table is FOR. A 80-row list may be less useful to a porting
   author than the required/optional split the current one attempts; if so, say
   that and generate the split, rather than listing every slot.
