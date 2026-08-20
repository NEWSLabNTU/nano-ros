---
id: 741
title: "`test_xrce_service_ros2_client` fails on main — Fast-DDS refuses the
  28-byte reply into a 15-byte history payload"
status: open
type: bug
area: rmw, testing
related: [issue-0736]
---

## Symptom

```
cargo nextest run -p nros-tests --test xrce_ros2_interop \
    -E 'test(test_xrce_service_ros2_client)' --retries 0

ROS 2 service client did not get sum=8 from the nano-ros XRCE service server
— XRCE-DDS service interop regression (233.6).
--- server startup ---
[INFO] nros: session open
[INFO] Waiting for service requests

--- ros2 client output ---
requester: making request: example_interfaces.srv.AddTwoInts_Request(a=5, b=3)
[RTPS_READER_HISTORY Error] Change payload size of '28' bytes is larger than
the history payload size of '15' bytes and cannot be resized.
    -> Function can_change_be_added_nts
```

Deterministic: 3/3 in-sweep retries and 1/1 solo.

## What the error says, and what it does not

The server came up and listened — its startup output is in the assert for
exactly this reason, and it rules out "no reply because the server never
started". The failure is on the **ROS 2 client's reader for the reply topic**:
Fast-DDS sized that reader's history from a max-serialized-size it learned at
discovery, got 15, and then refused a 28-byte sample rather than resizing.

15 bytes is not a plausible `AddTwoInts_Response` (8-byte `sum` + a 4-byte CDR
header is 12; a request is 16 + 4). So the reader was created against a type
whose advertised max size is wrong — a type-registration/discovery defect on
the XRCE side, not a serialization one. The reply is almost certainly being
built correctly and then dropped by the peer's history.

Note the direction: the sibling `test_xrce_action_ros2_client` and
`test_xrce_to_ros2_pubsub` both PASS in the same run, so whatever is wrong is
specific to the service reply type registration, not to the XRCE transport or
the agent as a whole.

## Not caused by phase-359 W10

Checked the way #0736 and #0737 were: `git checkout origin/main --
packages/core packages/api scripts/check-std-census.py`, `just setup-cli`,
rebuild native fixtures, run — fails identically. Upstream `main` is red here.

## Second host, 2026-08-21 — does not reproduce

Ran the issue's own sequence on the other host, after `just build-test-fixtures
lane=native` (RC=0) and `just setup-cli`:

| arm | result |
| --- | --- |
| `-E 'test(test_xrce_service_ros2_client)' --retries 0`, five separate runs | 5 pass / 0 fail |
| the whole `xrce_ros2_interop` binary, in-sweep, `--retries 0` | **9 tests run: 9 passed, 0 skipped** |

The in-sweep arm matters because this issue reports the failure as deterministic
in-sweep (3/3) as well as solo; the siblings that pass for you
(`test_xrce_action_ros2_client`, `test_xrce_to_ros2_pubsub`) pass here too, in
the same run as the one that fails for you.

Host: `ROS_DOMAIN_ID` / `CYCLONEDDS_URI` / `RMW_IMPLEMENTATION` unset, ROS 2
Humble at `/opt/ros/humble`.

**A hypothesis worth killing before anyone spends time on it.** Issue 0740 (the
config-header mirror invisible to Makefiles) lands consumer TUs against the
in-tree STUB header, and issue 0268 records that exact stale-sizes path
producing wrong `*_OPAQUE_U64S` and "memory corruption that surfaces as
unrelated runtime failures". A 15-byte history for a type that cannot be 15
bytes looks exactly like that. It is NOT that here: this fixture
(`build/cargo-fixtures/linux-*/service-server`) is a CARGO binary, and 0740 is a
cmake-generator defect that never touches it.

So the difference is the host, not the tree — which is the same split #0737 hit,
and worth stating so the two are not investigated as one thing.

## Reproduce

```
just build-test-fixtures lane=native
cargo nextest run -p nros-tests --test xrce_ros2_interop \
    -E 'test(test_xrce_service_ros2_client)' --retries 0
```
