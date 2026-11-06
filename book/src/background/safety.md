# Safety

<!-- type: explanation · level: 3 -->

The safety example workspace turns on end-to-end message integrity with one
line in its bringup:

```toml
# src/demo_bringup/system.toml
[system]
features = ["safety"]
```

Its talker is a plain talker. Its listener declares a validated subscription
and reads a verdict with every message:

```rust,ignore
if let Some(status) = ctx.integrity() {
    if status.is_valid() {
        // CRC matched, no sequence gap, not a duplicate
    } else {
        state.integrity_faults += 1;
    }
}
```

Where does that verdict come from, what does it protect against, and what
does it not? This page explains the end-to-end (E2E) safety protocol: a
CRC-32 and a sequence check on every sample, added by the runtime, with no
code in the node that publishes.

## The transport is a black channel

A ROS 2 transport is trusted: a message is serialized, sent and deserialized
with no check that it arrived intact, in order and once. A safety-related
deployment cannot assume that. EN 50159, the railway standard for
communication over an untrusted channel, treats the transport as a **black
channel**: whatever is in between may corrupt, repeat, drop, insert, reorder
or delay a message, or pretend to be another sender. The defence lives at the
two ends, not in the channel.

nano-ros already sends most of what the ends need. Every zenoh sample carries
a 33-byte attachment, for interoperability with `rmw_zenoh_cpp`:

| Bytes | Field | Purpose |
| --- | --- | --- |
| 0–7 | sequence number (i64) | increases by one per publisher |
| 8–15 | timestamp (i64, ns) | publication time |
| 16 | length prefix (always 16) | |
| 17–32 | `rmw_gid` | random per-publisher identifier |

Without the feature, the subscriber parses this into the message info and
checks nothing. With it, the publisher adds one field and the subscriber
validates.

## What the feature adds

`features = ["safety"]` turns on the `safety-e2e` feature of the runtime, and
`NROS_SAFETY_E2E` for a C or C++ build. Then:

- **The publisher** computes a CRC-32 over the serialized CDR payload and
  appends it to the attachment, which grows to 37 bytes. The payload format
  does not change. The CRC travels in the attachment, out of band from the
  payload it covers, so it is checked over a different data path.
- **The subscriber** recomputes the CRC over the payload it received and
  compares, and tracks the sequence number it expects next. Each message gets
  an `IntegrityStatus`:

| Field | Meaning |
| --- | --- |
| `gap` | how many messages were lost before this one (0 is normal) |
| `duplicate` | the sequence number is below the expected one |
| `crc_valid` | `Some(true)` matched, `Some(false)` corrupted, `None` no CRC sent |

`is_valid()` is true when there is no gap, no duplicate, and the CRC is
correct or absent. The first message a subscriber sees sets the expected
sequence, so joining late is not a gap.

The CRC is CRC-32/ISO-HDLC, the Ethernet polynomial, through a lookup table
generated at compile time. Its execution time does not depend on the data,
only on its length, which suits timing analysis. The cost is a 1 KB table in
read-only memory, four more attachment bytes per subscriber buffer, and a
16-byte validator per subscriber.

The plain `take()` path is unchanged. Only a subscription declared with
safety (`create_subscription_for_callback_name_with_safety` for a declarative
node, `take_validated` for a polled one, and the matching C and C++ forms)
returns a verdict.

## What it protects against, and what it does not

| Threat (EN 50159) | Defence | In nano-ros |
| --- | --- | --- |
| Corruption | CRC | yes: `crc_valid` |
| Repetition | sequence number | yes: `duplicate` |
| Deletion | sequence number | yes: `gap` |
| Resequencing | sequence number | yes: a late message reads as a duplicate |
| Insertion | sequence and source | partly: an unexpected sequence number shows as a gap or a duplicate |
| Delay | timestamp and timeout | **no**: the timestamp is sent but not checked |
| Masquerade | source authentication | **no**: the GID is sent but not checked |

Freshness needs a clock both ends agree on, and source authentication needs
the subscriber to know which publishers to expect. Neither is built. The
protocol also has no state machine across messages: each message is valid or
not on its own, and what the application does after a run of faults is the
application's decision. A watchdog that supervises liveness and deadlines is
separate from this protocol.

## Interoperability

The protocol never breaks a peer that lacks it. `rmw_zenoh_cpp` and a
nano-ros image without the feature read the first 33 bytes of the attachment
and ignore the rest.

| Sender | Receiver | CRC | Sequence | Result |
| --- | --- | --- | --- | --- |
| nano-ros with safety | nano-ros with safety | checked | tracked | full protection |
| nano-ros with safety | nano-ros without, or ROS 2 | ignored | not tracked | works, no protection |
| nano-ros without, or ROS 2 | nano-ros with safety | `None` | tracked | sequence only |

So mixing protected and unprotected nodes degrades protection; it never
stops traffic. A safety argument has to check that every link it relies on is
protected at both ends, because nothing on the wire will refuse an
unprotected one.

The CRC path lives in the zenoh backend, which is why the safety workspace
also states `rmw = "zenoh"`: the protocol is zenoh-only.

## Compared with AUTOSAR E2E

| | AUTOSAR E2E profiles 1 and 2 | nano-ros |
| --- | --- | --- |
| CRC | 8-bit | 32-bit |
| Counter | 4-bit | 64-bit, no rollover in practice |
| Data ID | 16-bit | 128-bit GID, sent but not checked |
| Timeout | configurable | not built |
| State machine | yes | no: a verdict per message |

The CRC and the counter are wider than those AUTOSAR profiles. The timeout
and the state machine are missing.

## How it is checked

The CRC and the validator have Verus proofs: the CRC is deterministic and
detects a single flipped bit, the expected sequence advances on a normal
message, a gap is reported as exactly the number of missing messages, and
`is_valid` holds exactly when there is no gap, no duplicate and no known-bad
CRC. The proofs run against a model of the validator's state, which the
production tests tie to the real type.

## Where nano-ros differs from ROS 2

ROS 2's client libraries and `rmw_zenoh` do not check a message end to end;
a ROS 2 subscriber gets the message and its info, and nothing else. nano-ros
adds the check below the client API, so it is a system feature you declare
in the bringup, not code in a node. A ROS 2 peer still talks to a protected
nano-ros node, but its messages carry no CRC, and a protected nano-ros
publisher's CRC is invisible to it.

## Design background

- [RFC-0028](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0028-e2e-safety-protocol-integration.md):
  the E2E safety protocol, the threat model and the future extensions
  (freshness, source authentication, watchdog supervision).
