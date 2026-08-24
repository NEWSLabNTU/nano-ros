---
id: 776
title: "Nothing computes a message's serialized size bound, so a dropped take
  can say the buffer was too small but never how large it needed to be"
status: open
type: gap
area: rmw, codegen
related: [issue-0757, rfc-0023, rfc-0054]
---

## Problem

Upstream has `rmw_get_serialized_message_size(typesupport, bounds, size_t *out)`.
We have no equivalent, and — the part that makes this a gap rather than a
declined feature — **nothing else computes the bound either.**

Checked, because the parity table claimed otherwise:

* `packages/core/nros-serdes/src/traits.rs` declares `serialize`,
  `deserialize` and `deserialize_borrowed`. No size, no bound.
* No generated message crate emits a size constant (`git grep MAX_SERIALIZED`
  over `packages/interfaces` finds nothing).
* Buffers are sized by environment knobs — `NROS_SUBSCRIPTION_BUFFER_SIZE`,
  `NROS_PARAM_SERVICE_BUFFER_SIZE` — which are a GUESS the integrator makes,
  not a bound derived from the types in play.

The parity map recorded this as answered at another layer, with the reason
"generated per type; the bound is baked". That was false in both clauses, and it
is corrected to `gap` in the same change that files this.

## What it costs, concretely

`report_dropped_take` (`packages/core/nros-node/src/executor/arena.rs`) is the
place a consumer meets this. It can say:

```
subscription take DROPPED (BufferTooSmall); buffer is 512 bytes. The sample was
received and ACKed, then discarded — raise the subscription buffer knob if this
is BufferTooSmall.
```

"Raise the knob" is the only advice available, because the runtime does not know
what value would have worked. The integrator raises it, guesses again, and
retries — on a target where the knob is static RAM they cannot spare. Issue 0757
is the same shape one layer over: a sample received, ACKed by the transport, and
discarded above it.

A bound would let that message name the size, and would let a build FAIL at
codegen time when a subscription's declared buffer cannot hold its own message
type — which is the check worth having, since it turns a runtime drop into a
compile error.

## Why it is not simply "add the slot"

The upstream signature takes a `rosidl_message_type_support_t *` and a
`rosidl_runtime_c__Sequence__bound *`, neither of which crosses this ABI: there
is no typesupport indirection on target, and codegen bakes the type. So this is
NOT a vtable slot — nothing about a size bound varies by backend, and the same
argument that made `qos_profile_check_compatible` a plain ABI function applies
here.

It is a CODEGEN capability: the generator knows every field of every message it
emits, so it can emit a `MAX_SERIALIZED_SIZE` const beside the type — bounded
for fixed-size messages, and for unbounded strings and sequences a bound derived
from the declared upper limits (`string<=N`, `sequence<T, N>`) or an explicit
"unbounded" marker where the IDL gives none.

Sketch, not a design:

* `nros-serdes` grows an associated `const MAX_SERIALIZED_SIZE: Option<usize>`.
* Codegen computes it per type; `None` for genuinely unbounded messages.
* The executor's buffer sizing can then assert against it, and
  `report_dropped_take` can name the number.

## Scope

Filed from phase-376 W4, which is where the false claim was found. Not part of
that campaign's remaining work: the ABI question ("is there a slot?") is
answered — there is not, and should not be. This is the capability the answer
leaves open.

## Upstream is not a reference implementation — it is a stub (checked 2026-08-24)

Before designing ours against upstream's, I went and looked at what ROS 2 Humble
actually ships. It is worth knowing, because it inverts the "just copy upstream"
instinct:

| implementation | what `rmw_get_serialized_message_size` does |
| --- | --- |
| `librmw_cyclonedds_cpp.so` | sets the error string `"rmw_get_serialized_message_size: unimplemented"` and returns `RMW_RET_UNSUPPORTED` (`mov $0x3`) |
| `librmw_fastrtps_cpp.so` | same shape, same `"unimplemented"`, same `RMW_RET_UNSUPPORTED` |
| `librmw_zenoh_cpp.so` | has a real body |

And, more telling: **nothing in the whole installation calls it.** Scanning every
`lib*.so` under `/opt/ros/humble/lib` for an undefined reference to the symbol
finds zero callers — not rcl, not rclcpp, not the typesupport libraries.

### Why upstream can afford that, and we cannot

Upstream's serialized take writes into an `rmw_serialized_message_t`, which is
an `rcutils_uint8_array_t` — and it RESIZES. `rmw_take_serialized_message`'s own
documentation describes `rmw_get_serialized_message_size` as a way to pre-size
the buffer "to prevent byte stream resizing on take". So upstream's bound is a
PERFORMANCE HINT: skipping it costs a realloc, and that is why two of three
implementations never bothered and nobody calls it.

We have no resize. A subscription buffer is baked, and a sample that does not fit
is DROPPED after the transport has already ACKed it. The same number that is an
optimisation upstream is load-bearing here — which is the actual argument for
building this, and it is stronger than "upstream has it and we do not".

### What that means for the design

* **Do not mirror the signature.** It takes a
  `rosidl_message_type_support_t *` and a `rosidl_runtime_c__Sequence__bound *`,
  neither of which crosses this ABI — and mirroring an interface whose reference
  implementations return `UNSUPPORTED` would buy nothing anyway.
* **Do take one idea from it:** the `message_bounds` parameter exists because an
  unbounded sequence has no size until someone names a limit. Upstream makes
  that the caller's problem at runtime. Ours can be better placed — codegen sees
  the IDL, so `string<=N` and `sequence<T, N>` give a real bound, and only a
  genuinely unbounded field needs an explicit marker.
* **The verdict for phase-376 stands and is now better supported:** this is not
  a vtable slot. Two of three upstream implementations prove it does not vary by
  backend in any way worth dispatching on — they do not vary at all.
