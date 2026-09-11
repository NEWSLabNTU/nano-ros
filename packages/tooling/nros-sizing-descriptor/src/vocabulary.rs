//! The closed vocabularies the descriptor writes.
//!
//! Every one of these is a CLOSED SET, and an unrecognised spelling is a parse
//! ERROR rather than a row a reader skips — `EntityKind`'s rule, restated here
//! because this file is a wire format and the consequence is the same:
//!
//! > The set is closed on purpose: an unrecognised spelling is a REFUSAL, never
//! > a row this module skips. A skipped row is exactly an under-report.
//!
//! The spellings are the ones the contract, the entity inventory and ROS 2
//! already use. They are not re-chosen here; a second spelling of one fact is
//! how two of them drift.

use core::fmt;

/// Build one closed vocabulary: the variants, their spellings, `tag`, `parse`,
/// `ALL`, `Display` and serde in one place.
///
/// A macro because five of these differ only in their rows, and five
/// hand-written copies is five places a variant can be forgotten in the parse
/// while being present in the render — which reads as a round-trip that works
/// until somebody writes the new value.
macro_rules! vocabulary {
    (
        $(#[$meta:meta])*
        $name:ident { $( $(#[$vmeta:meta])* $variant:ident => $tag:literal ),+ $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum $name {
            $( $(#[$vmeta])* $variant ),+
        }

        impl $name {
            /// Every variant, in emission order. The ONE list.
            pub const ALL: &'static [$name] = &[ $( $name::$variant ),+ ];

            /// The canonical spelling, as it appears in the descriptor.
            pub const fn tag(self) -> &'static str {
                match self { $( $name::$variant => $tag ),+ }
            }

            /// Parse one spelling. An unknown one is an error naming the legal
            /// set — never a `None` the caller can drop.
            pub fn parse(s: &str) -> Result<Self, String> {
                match s {
                    $( $tag => Ok($name::$variant), )+
                    other => Err(format!(
                        "unknown {} `{other}` -- expected one of: {}",
                        stringify!($name),
                        Self::ALL.iter().map(|v| v.tag()).collect::<Vec<_>>().join(", "),
                    )),
                }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.tag())
            }
        }

        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                s.serialize_str(self.tag())
            }
        }

        impl<'de> serde::Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                let raw = <std::borrow::Cow<'de, str> as serde::Deserialize>::deserialize(d)?;
                $name::parse(raw.as_ref()).map_err(serde::de::Error::custom)
            }
        }
    };
}

vocabulary! {
    /// `[meta] status` — how much of this descriptor a consumer may size from.
    ///
    /// A SUMMARY of the per-field statuses, never a substitute for them: a
    /// consumer still reads the field it needs. It exists so a human running
    /// `nros ws sizing-descriptor` sees at a glance whether declaring more would
    /// buy anything, and so a lane can assert "this image derives".
    Status {
        /// Every field this image could answer is stated.
        Derived => "derived",
        /// Some fields are stated and some refused. The common case for an
        /// image mid-way through declaring.
        Partial => "partial",
        /// Nothing may be sized from this image's declarations.
        Refused => "refused",
    }
}

vocabulary! {
    /// `[meta] basis` — what the numbers describe.
    ///
    /// D6: *"Never silently widens the basis. A refused `subscribed` basis does
    /// not fall back to `closure` — that publishes the wrong row while every
    /// status still reads 'derived', which is the shape that looks like it
    /// worked."* So the basis is STATED beside the rows and a consumer that
    /// wanted one and got the other refuses rather than reading on.
    Basis {
        /// The rows describe what this image's contract says it creates.
        Contract => "contract",
        /// The rows describe the whole link closure — every type reachable,
        /// not the subscribed set. The worst case, and honest about it.
        Closure => "closure",
    }
}

vocabulary! {
    /// `[[endpoint]] kind` — mirror of `EntityKind`'s endpoint arms.
    ///
    /// Only the kinds that HAVE a `(kind, type, topic)` identity are here. A
    /// timer and a guard condition carry no type and no topic, so they are not
    /// rows in this table; they are counted, and the count is a different fact.
    /// `EntityKind::carries_qos_depth` already draws exactly this line.
    EndpointKind {
        Publisher => "publisher",
        Subscription => "subscription",
        ServiceServer => "service_server",
        ServiceClient => "service_client",
        ActionServer => "action_server",
        ActionClient => "action_client",
    }
}

impl EndpointKind {
    /// Does an endpoint of this kind draw from the topic PAYLOAD pools?
    ///
    /// Mirror of `EntityKind::receives_topic_sample`, and narrow for the reason
    /// that one is: a service server's request buffer and an action client's
    /// feedback buffer are real receive buffers and neither is one of these
    /// blocks.
    pub const fn receives_topic_sample(self) -> bool {
        matches!(self, EndpointKind::Subscription)
    }
}

vocabulary! {
    /// `[[endpoint]] history`.
    ///
    /// The one policy that can REFUSE another field. `keep_all` bounds a queue at
    /// nothing, so a `depth` beside it prices nothing and the depth-derived
    /// fields are refused naming the endpoint (RFC-0100 D6; phase-454 W3 already
    /// implements the refusal in `EntityInventory::declared_depths`).
    History {
        KeepLast => "keep_last",
        KeepAll => "keep_all",
    }
}

vocabulary! {
    /// `[[endpoint]] reliability`. Gates XRCE's two 64 KiB `*_reliable_buf`.
    Reliability {
        Reliable => "reliable",
        BestEffort => "best_effort",
    }
}

vocabulary! {
    /// `[[endpoint]] durability`. `transient_local` is publisher-side retention,
    /// which is why phase-454 W2 had to make a publisher's depth travel first.
    Durability {
        Volatile => "volatile",
        TransientLocal => "transient_local",
    }
}

vocabulary! {
    /// `[[endpoint]] registration_path` — issue 1319, RFC-0100 D1.
    ///
    /// **A subscription's SLOT SIZE is a function of this and nothing else can
    /// supply it.** The four rows are measured in issue 1319:
    ///
    /// | path | slot size |
    /// | --- | --- |
    /// | `c_typed_hint` | the type's own `_RX` — matches the model |
    /// | `rust_typed_descriptors` | `min(framed(bound), RX_BUF)` — at or below |
    /// | `rust_typed_schemaless` | **`RX_BUF`** |
    /// | `c_raw_no_hint` | **`RX_BUF`** |
    ///
    /// The last two are 1,848 bytes per subscription the model does not hold on
    /// the reference island at depth 1 — an UNDER-size, the direction that ships
    /// `BufferTooSmall`. The field is REQUIRED for that reason: W5 closes 1319
    /// from it, and a descriptor that omitted it would leave W5 with the same
    /// blindness the build has today.
    ///
    /// It is an IMAGE fact, not a backend fact: which of the four a registration
    /// takes is decided by the entry's language, whether it passes a bound hint,
    /// and whether the linked backend carries type descriptors — all three known
    /// where this descriptor is written and none of them known to the build
    /// script that prices the arena.
    RegistrationPath {
        /// C or C++, typed, `nros::rx_size_bound<M>` supplied.
        CTypedHint => "c_typed_hint",
        /// Rust, typed, against a backend WITH type descriptors (Cyclone).
        RustTypedDescriptors => "rust_typed_descriptors",
        /// Rust, typed, against a SCHEMALESS backend (zenoh, XRCE).
        RustTypedSchemaless => "rust_typed_schemaless",
        /// C or C++, raw, no hint.
        CRawNoHint => "c_raw_no_hint",
    }
}

impl RegistrationPath {
    /// Does this path claim the CLOSURE buffer (`RX_BUF`) rather than the type's
    /// own bound?
    ///
    /// The two `true` rows are exactly issue 1319's gap. Stated as a predicate
    /// here so W5's arena term asks the question once instead of matching four
    /// variants in each of its call sites — and so adding a fifth path has to
    /// answer it.
    pub const fn claims_closure_buffer(self) -> bool {
        matches!(
            self,
            RegistrationPath::RustTypedSchemaless | RegistrationPath::CRawNoHint
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_round_trips_through_its_spelling() {
        // The macro's whole point. A variant present in `tag` and missing from
        // `parse` is the drift it exists to make impossible; this asserts the
        // macro delivered on both halves for every vocabulary.
        macro_rules! round_trip {
            ($t:ty) => {
                for v in <$t>::ALL {
                    assert_eq!(
                        <$t>::parse(v.tag()).unwrap(),
                        *v,
                        "{} did not round-trip",
                        v.tag()
                    );
                }
            };
        }
        round_trip!(Status);
        round_trip!(Basis);
        round_trip!(EndpointKind);
        round_trip!(History);
        round_trip!(Reliability);
        round_trip!(Durability);
        round_trip!(RegistrationPath);
    }

    #[test]
    fn an_unknown_spelling_is_an_error_naming_the_legal_set() {
        let err = History::parse("keep_some").unwrap_err();
        assert!(err.contains("keep_some"), "{err}");
        assert!(err.contains("keep_last"), "{err}");
        assert!(err.contains("keep_all"), "{err}");
    }

    #[test]
    fn the_two_closure_buffer_paths_are_the_two_issue_1319_measured() {
        assert!(RegistrationPath::RustTypedSchemaless.claims_closure_buffer());
        assert!(RegistrationPath::CRawNoHint.claims_closure_buffer());
        assert!(!RegistrationPath::CTypedHint.claims_closure_buffer());
        assert!(!RegistrationPath::RustTypedDescriptors.claims_closure_buffer());
    }
}
