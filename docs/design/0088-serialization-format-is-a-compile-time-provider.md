---
rfc: 0088
title: "Serialization format is a compile-time provider"
status: Draft
since: 2026-09
last-reviewed: 2026-09
implements-tracked-by: [phase-421]
supersedes: []
superseded-by: null
---

# RFC-0088 — Serialization format is a compile-time provider

## Summary

nano-ros keeps ROS 2's semantics — **one backend, one encoding, not chosen by the
user per topic** — and changes the mechanism, because ROS 2's mechanism exists to
serve `dlopen` and we do not `dlopen`. A serialization format becomes a
**compile-time constant**: a message declares one, a backend declares one, and a
mismatch is a compile error rather than a runtime string comparison. (The first
implementation made it an associated type; D1 records why a defaulted const on
the universal message trait is the shape that actually covers every backend.) The format's
canonical representation is a `u8` discriminant assigned per image; the string
name survives only where an identity must cross image boundaries — the bridge
config, tooling output, and the parity vtable slot.

A user-supplied serialization library is then an ordinary provider package
(RFC-0087), family `serdes`, discovered by package name and selected
declaratively. The default implementation strategy is **schema-driven**: it walks
the `&'static` field schema codegen already emits for every message, so a custom
format costs no codegen work.

## Motivation / problem

### What ROS 2 actually does

Measured against `/opt/ros/humble` on 2026-09-04.

`rmw/rmw.h` states the semantic outright:

> Get the unique serialization format for this middleware. … **One middleware can
> only have one encoding.**

`rmw_get_serialization_format()` takes no handle, because one process links one
middleware. Measured: `RMW_IMPLEMENTATION=rmw_fastrtps_cpp` answers `cdr`.

Format is therefore an RMW property, discoverable at runtime, and **not
configurable by the user** at the pub/sub layer. The one real serializer plugin
interface in the stack is rosbag2's
(`converter_interfaces/serialization_format_{serializer,deserializer,converter}.hpp`),
and it lives where data leaves the live system, not where it crosses the wire.

Serialization and transport are separated *structurally*: `rosidl` typesupport
owns how a type is serialized, keyed by a `typesupport_identifier` string; the
RMW owns transport and asks for the identifier it wants;
`rosidl_typesupport_c__get_message_typesupport_handle_function` resolves it —
*"if the identifier is the same as this handle's typesupport_identifier, then the
handle is simply returned, **otherwise it's loaded from a shared library**"*.
That sentence is the reason for the string: **it is a dynamic-linker key.** The
separation is genuine — `librmw_cyclonedds_cpp.so` references only
`rosidl_typesupport_introspection_*` and serializes by walking the type
description, while rmw_fastrtps uses generated `rosidl_typesupport_fastrtps_*`
code — two strategies, one wire format.

### Why we cannot copy the ABI

Neither ROS 2 typesupport ABI is a stable ABI:

- **fastrtps.** `message_type_support_callbacks_t` declares
  `bool (*cdr_serialize)(const void *, eprosima::fastcdr::Cdr &)` — a third-party
  **C++ type, by reference, in a struct crossed by `dlopen`**. It binds the ABI to
  FastCDR's C++ ABI, the compiler and the standard library at once. Evolution is
  handled by `#define ROSIDL_TYPESUPPORT_FASTRTPS_HAS_PLAIN_TYPES`, i.e.
  compile-time feature detection, which works only because everything is rebuilt
  together.
- **introspection.** `rosidl_typesupport_introspection_c__MessageMember` /
  `…MessageMembers` are clean C POD — and carry **no version field and no
  struct-size field**, while members have been appended across distros. That is
  precisely the append-drift class `check-ffi-struct-mirrors` exists for; it hit
  this tree three times through QoS (`tx_express`, `callback_group`).

So ROS 2's typesupport ABI is a **source contract recompiled per distro**.
Cross-distro interop happens on the wire, never through the ABI.

### Where nano-ros stands

- `EmbeddedRawPublisher::publish_raw` is documented as *"Publish raw CDR-encoded
  data (must include CDR header)"* — an assumption in prose, checked by nothing.
- `nros-bridge` documents *"both sides must use ROS-CDR … Cross-encoding bridges
  would need an explicit translator and are out of scope"* — also a comment.
- uORB already speaks something that is not CDR (the PX4 struct, verbatim), and
  is treated as a special case rather than as a backend with a different format.
- The vtable slot is **already reserved**. `rmw_vtable.h` declares
  `const char *(*get_serialization_format)(void)` and
  `check-rmw-slot-producers.py` files it under the `identity` inert family with
  the reason: *"reserved so a bridge image linking two backends can [ask], since
  that is the case where a per-backend answer stops being decoration"*. Both
  preconditions — a backend that speaks something else, and a bridge image — now
  exist.
- The reflective description **never crosses our C ABI**: `nros_serdes::schema`
  is `&'static`, `Copy`, allocation-free, entirely in `.rodata`, and appears
  nowhere in `nros_generated.h`. We already avoided ROS 2's most fragile piece,
  by not having a plugin boundary at all.

## Design

### D1 — Format is a type, not a string

```rust
/// ABI discriminant. Image-local (see D2).
#[repr(u8)]
pub enum SerializationFormatId { Cdr = 1, Uorb = 2 }

pub trait SerializationFormat {
    const ID:   SerializationFormatId;
    const NAME: &'static str;      // presentation and cross-image identity only
}
pub struct Cdr;
pub struct Uorb;
```

The message's format is a **defaulted const on the universal message trait**,
and the backend's is a const on `Session`:

```rust
// nros_core::RosMessage
const SERIALIZATION_FORMAT_ID: SerializationFormatId = SerializationFormatId::Cdr;
// nros_rmw::Session
const SERIALIZATION_FORMAT: &'static str = "cdr";
const SERIALIZATION_FORMAT_ID: SerializationFormatId = SerializationFormatId::Cdr;
```

**Three implementation findings amended this section after W1 measured them
(2026-09-04); the shape survived, the spelling did not.**

*A bound is impossible.* `NodeHandle` is not generic over the backend — it holds
`&mut session::ConcreteSession`, a type alias resolved by cargo feature — so
there is no `B` to bind `M::Format` against. The check is an inline `const {}`
in the generic entity creators, comparing the message's const against
`session::IMAGE_SERIALIZATION_FORMAT_ID`. Same guarantee, no generics:

```rust
const {
    assert!(M::SERIALIZATION_FORMAT_ID.as_u8() == IMAGE_SERIALIZATION_FORMAT_ID.as_u8(),
            "message serialization format does not match the linked backend");
}
```

*It must be a const, not an associated type, and it must live on `RosMessage`.*
An associated type on `nros_serdes::schema::Message` was implemented first and
reverted: `MessageForRmw` requires a schema **only** under
`cfg(rmw_needs_type_descriptors)` (Cyclone), so that check was absent under
zenoh, XRCE — and under uORB, the one backend whose format differs and the
reason the check exists. `RosMessage` is universal. Rust has no stable
associated-type defaults but does have const defaults, so a defaulted const also
costs the 142 existing implementors nothing; phase-380 W4 had already tried
tightening the message contract to serve a build assertion and reverted it when
`examples/native/rust/custom-msg` stopped compiling.

*The raw path cannot carry a defaulted type parameter.* `create_publisher_raw<F
= Cdr>` is rejected by rustc (`invalid_type_param_default`, deny-by-default), and
a non-defaulted `F` breaks every existing call site. So the raw constructors keep
their signatures and a caller states its claim explicitly with
`assert_raw_format::<F>()`.

*It is a `cargo build` error, not a `cargo check` one.* An inline `const {}` in a
generic function is evaluated by the monomorphisation collector, which runs only
during codegen. `just ci gate` catches it because `test-unit` builds.

There is **no runtime per-sample cost and no runtime constructor variant**. An
earlier sketch proposed `create_publisher_raw_in_format(…, format: &str)`
checked at entity creation; it is rejected, because a compile-time fact does not
need a runtime constructor.

### D2 — The `u8` is image-local; the string is the cross-image identity

nano-ros cannot allocate a globally unique discriminant to a third party. So:

- the **string name is the identity** across images, in `nros-bridge.toml`, in
  tooling output, and in the vtable slot;
- the **`u8` is assigned by the build** from the set of formats declared in that
  image, and is meaningful only within it. In-tree formats keep low reserved
  values for readability, nothing more.

Getting this backwards would let two independently built images disagree about
what `3` means, which is a wire-visible bug with a compile-time-looking cause.

### D3 — The bridge is the only runtime site

`Executor::open_multi` is where the type-level answer stops existing: one image,
two backends, two formats. There the tag becomes a value.

```rust
impl RawSubscription { pub fn format(&self) -> SerializationFormatId; }

impl PubSubBridge {
    /// Errs unless ingress and egress agree.
    pub fn new(sub, pubr, origin) -> Result<Self, BridgeError>;
    /// Cross-format, opted into by name — rosbag2's converter, one layer down.
    pub fn with_converter(sub, pubr, origin, conv: &'static dyn SerializationFormatConverter)
        -> Result<Self, BridgeError>;
}
```

One comparison of one byte, at construction. This turns `nros-bridge`'s "both
sides must use ROS-CDR" comment into a return value, and it makes phase-325's
uORB→ROS 2 bridge a checked configuration rather than a thing to remember.

### D4 — The vtable slot gets a body

`get_serialization_format` leaves the `identity` inert family and returns the
backend's constant. It costs one function per backend, satisfies
`check-rmw-api-parity`, and is the only mechanism that can answer **per session**
in the bridge image where a compile-time constant cannot.

### D5 — C and C++ assert at compile time

C — the format is a generated macro, and the message headers assert against it:

```c
#define NROS_SERIALIZATION_FORMAT_ID  1
#define NROS_SERIALIZATION_FORMAT     "cdr"

_Static_assert(NROS_MSG_FORMAT_ID_geometry_msgs__PoseStamped
                 == NROS_SERIALIZATION_FORMAT_ID,
               "message format does not match the linked backend");
```

C++ — same enum, `static_assert`, and `const char*` rather than `std::string_view`
so the header stays usable against Zephyr's minimal libcpp:

```cpp
enum class SerializationFormat : uint8_t { Cdr = 1, Uorb = 2 };
template <typename M> struct format_of;              // codegen specializes
inline constexpr SerializationFormat linked_format = /* from the macro */;
```

**The macro is only meaningful in a single-backend image.** A bridge image links
two and must use the per-session accessor; a bridge-linked image referencing the
macro is a build error, not a subtle wrong answer.

### D6 — A serialization library is a provider package

Family `serdes` under RFC-0087. Announcement carries the name; the descriptor
carries only what cannot be derived.

```xml
<export>
  <build_type>nros_cargo</build_type>
  <nano_ros_provides kind="serdes" name="flatbuf"/>
</export>
```

```toml
# nros-serdes.toml — optional; absent means every default applies
[serdes]
impl = "schema"        # "schema" | "codegen"
```

Consumption uses the general form, so no parser learns a new attribute:

```xml
<nano_ros_uses kind="serdes" name="flatbuf"/>
```

and the system config may set it per image:

```toml
[image.orin-spe]
serdes = "flatbuf"
```

**Amended 2026-09-04 from implementation (W4).** This RFC first specified
`[deploy.<t>] serdes = "…"`, and that key cannot exist: `DeployBlock` comes from
the upstream `ros-launch-manifest` crate (a git-tag dependency) and carries
`deny_unknown_fields`, so an unknown key in a deploy block is a parse *error*,
not an ignored line. Adding it would need an upstream release and a dependency
bump. The ladder therefore mirrors `rmw`'s, minus that rung: CLI override →
`[image.<t>]` folded over `[image_defaults]` → `[system]` → `"cdr"`, with
`[package.metadata.nros.deploy.<t>].serdes` — the deploy block nano-ros *does*
own — carried into the synthesized image block exactly as `rmw` already is.
`rmw` on `[deploy.*]` is itself deprecated (`image::DEPRECATED_DEPLOY_FIELDS`,
RFC-0065 D6 / issue 0951), so the rung this loses is one the tree is already
retiring.

A custom format that wraps a C codec is two ordinary packages: the provider, and
a vendor package it `<depend>`s on (RFC-0087 D5).

### D7 — Schema-driven is the default strategy

```rust
pub trait SchemaSerializer {
    const FORMAT_NAME: &'static str;   // cross-image identity (D2)
    const FORMAT_ID:   u8;             // image-local discriminant
    fn serialize(msg: &mut CdrReader<'_>, type_name: &str,
                 schema: &'static [Field], out: &mut [u8]) -> Result<usize, SchemaError>;
    fn deserialize(bytes: &[u8], type_name: &str,
                   schema: &'static [Field], msg: &mut CdrWriter<'_>) -> Result<usize, SchemaError>;
}
```

A provider implements this once and works for every message, with no codegen
plugin and no generated code.

**Amended 2026-09-04 from implementation (W5): the message side is the CDR byte
stream, not a `*const u8`.** This RFC first specified a raw pointer to the
message struct, and that is not implementable over today's schema, for three
independent measured reasons:

1. **A `String` member's host type is `heapless::String<N>` and the schema
   carries no `N`.** Codegen emits `FieldType::String`, the IDL type;
   `BoundedString(n)` is the IDL bound, a *different* number from the host
   capacity.
2. **`heapless::String` / `heapless::Vec` are `repr(Rust)`.** `Field::offset`
   soundly reaches the *start* of the container and nothing inside it.
3. **`NestedType` records `type_name` and `fields` but no size**, so
   `Array(N, Nested(..))` and sequences of structs have no stride.

So `impl = "schema"` is a **transcoder** strategy in v1: CDR bytes in, the
provider's bytes out. That is a smaller claim than "reads the message struct
directly", and it is the claim the schema can actually support. The walk
therefore lives in `nros-serdes` (`walk::{SchemaSink, SchemaSource}`) rather
than being re-implemented per provider, and `type_name` is a parameter because a
`&'static [Field]` slice has no name of its own.

Making the pointer form implementable is a change to the *schema*, not to this
trait: it needs the host capacity, a stable container layout, and a nested
type's size. That is worth doing when a provider needs to skip the CDR hop, and
it is not free.

**One hole the walk inherits rather than introduces:** `WString` /
`BoundedWString` return `Unsupported`, because `CdrReader`/`CdrWriter` have no
wide-string primitive in either direction. Nothing in tree uses a `wstring`. It is slower than the per-type `serialize` we emit
for CDR, and that trade is deliberate for v1: adequate for a control topic,
inadequate for a high-rate sensor stream. `impl = "codegen"` is the answer when
someone hits the wall, and it is the point at which a codegen plugin ABI must
exist — which is why both are out of the first implementation together.

### D8 — ABI rules, chosen against ROS 2's two counter-examples

| Rule | Counter-example | Gate |
| --- | --- | --- |
| Pure C across the ABI — no C++, no third-party types | `fastcdr::Cdr&` in a `dlopen`ed vtable | `check-c` cross-include TU |
| `uint8_t` discriminant, values never reused | strings, needed only for `dlopen` | `check-serdes-descriptors` |
| Append-only; nullity means "unsupported" | introspection appends silently | `check-rmw-slot-producers` |
| A growable struct leads with version + size | neither ROS 2 struct has either | `check-ffi-struct-mirrors` |
| Header is SSoT, bindgen output committed | headers regenerated per distro | `check-abi-bindings` (RFC-0054) |

nano-ros can therefore offer what ROS 2 structurally cannot — a versioned pure-C
serialization and transport ABI that survives a rebuild of one side — because the
format is decided at compile time and the description never leaves Rust.

## Non-goals

- **Per-topic or per-publisher serializer selection.** ROS 2 does not offer it;
  neither will we. One image, one format per backend.
- **A codegen plugin ABI** in the first implementation (D7).
- **A new matrix coordinate.** Tier 2 is 1-wise over platform × language × rmw ×
  kind at 258 rows; a fifth axis multiplies a cover already being paid for. One
  native cell per shipped format, and a custom format is covered by its own
  provider's tests.

## Gates

| Gate | Asserts |
| --- | --- |
| `check-serdes-descriptors` | S1–S4 of the family, plus discriminant allocation |
| `check-format-macro-scope` | a bridge-linked image does not reference `NROS_SERIALIZATION_FORMAT` |
| `check-rmw-slot-producers` | `get_serialization_format` has left the inert family and every backend produces it |

## Open questions

- Whether `SerializationFormatConverter` implementations are themselves provider
  packages (family `serdes-converter`) or plain crates a bridge links. Plain
  crates for v1.
- Whether services and actions need a per-endpoint format or inherit the
  backend's. Inherit for v1; ROS 2 has no per-endpoint answer either.
- Where the discriminant allocation lives when two independently built images
  bridge to one another — currently the string, which is sufficient, but a
  registry may become desirable if converters proliferate.

## Changelog

- 2026-09-04 — D7 amended from implementation (phase-421 W5): the schema-driven
  strategy transcodes from the CDR byte stream rather than reading the message
  struct through a `*const u8`. The schema carries neither the host string
  capacity, nor a stable container layout, nor a nested type's size, so the
  pointer form cannot be written against it today.
- 2026-09-04 — D6 amended from implementation (phase-421 W4): the per-target key
  is `[image.<t>].serdes`, not `[deploy.<t>].serdes`. The upstream `DeployBlock`
  denies unknown fields, so the specified key would have been a parse error.
- 2026-09-04 — D1 amended from implementation (phase-421 W1–W3): the message's
  format is a **defaulted const on `nros_core::RosMessage`**, not an associated
  type on `nros_serdes::schema::Message` — the schema is required only under
  Cyclone, so keying on it left the check absent under the very backend it
  exists for. The check is an inline `const {}` in the entity creators rather
  than a where-clause bound, because `NodeHandle` is not generic over the
  backend. Recorded that the raw path cannot take a defaulted type parameter,
  and that the failure is a `build` error rather than a `check` error.
- 2026-09-04 — initial draft. Records the ROS 2 study (one middleware one
  encoding; the identifier string as a `dlopen` key; neither typesupport ABI
  stable), decides format-as-a-type with an image-local discriminant, makes the
  bridge the only runtime site, activates the reserved vtable slot, and defines
  the `serdes` provider family over RFC-0087.
