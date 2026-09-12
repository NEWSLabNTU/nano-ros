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
    /// supply it.** Issue 1319 measured four rows from the source; phase-454 W5
    /// ran them and found a FIFTH, which is why the vocabulary has five:
    ///
    /// | path | slot size |
    /// | --- | --- |
    /// | `c_typed_hint` | the type's own `_RX` — matches the model |
    /// | `rust_typed_descriptors` | `min(framed(bound), RX_BUF)` — at or below |
    /// | `rust_typed_in_place` | **no receive region at all** |
    /// | `rust_typed_schemaless` | **`RX_BUF`** |
    /// | `c_raw_no_hint` | **`RX_BUF`** |
    ///
    /// The last two are 1,848 bytes per subscription the model does not hold on
    /// the reference island at depth 1 — an UNDER-size, the direction that ships
    /// `BufferTooSmall`. The field is REQUIRED for that reason: W5 closes 1319
    /// from it, and a descriptor that omitted it would leave W5 with the same
    /// blindness the build has today.
    ///
    /// It is an IMAGE fact, not a backend fact: which row a registration takes
    /// is decided by the entry's language, whether it passes a bound hint, and
    /// two properties of the linked backend — all known where this descriptor is
    /// written and none of them known to the build script that prices the arena.
    RegistrationPath {
        /// C or C++, typed, `nros::rx_size_bound<M>` supplied.
        CTypedHint => "c_typed_hint",
        /// Rust, typed, against a backend WITH type descriptors (Cyclone).
        RustTypedDescriptors => "rust_typed_descriptors",
        /// Rust, typed, against a backend that dispatches IN PLACE — zenoh and
        /// XRCE, whose `supports_process_in_place` is an unconditional `true`.
        ///
        /// **MEASURED, phase-454 W5, and it is the row issue 1319's analysis
        /// did not have.** `register_subscription_buffered_on` tests the
        /// capability BEFORE it computes a slot size and returns through
        /// `SubInplaceEntry` when it holds, so no receive region is allocated at
        /// all — an explicit `.rx_buffer::<N>()` on such a backend is not even
        /// read. Measured on `contract-monitor-sub` over zenoh: a
        /// `std_msgs/Header` subscription claims **672 bytes** of arena, against
        /// the 9,768-byte region the model budgets it at `KEEP_LAST(10)`.
        ///
        /// So this row is an OVER-statement, not an under-size, and its price is
        /// deliberately left where it was: the same type's-bound term
        /// [`Self::RustTypedDescriptors`] takes. Lowering it is worth ~9.7 KiB a
        /// subscription and is issue 1340, because the Rust GENERIC
        /// (`.generic(ty, hash)`) registration on the SAME backend does not
        /// reach that capability test and does claim `RX_BUF` — and nothing in
        /// this descriptor distinguishes a typed endpoint from a generic one.
        RustTypedInPlace => "rust_typed_in_place",
        /// Rust, typed, against a SCHEMALESS backend that BUFFERS — no schema on
        /// `MessageForRmw`, so no bound is reachable at the type-erased site and
        /// the registration takes `RX_BUF`.
        ///
        /// Issue 1319 attributes this row to zenoh and XRCE. They are
        /// [`Self::RustTypedInPlace`] instead, measured; this row stands for a
        /// schemaless backend that does not dispatch in place, which is a
        /// configuration the tree admits and does not currently ship.
        RustTypedSchemaless => "rust_typed_schemaless",
        /// C or C++, raw, no hint. The C registration path never consults the
        /// in-place capability, so this row is live on every backend.
        CRawNoHint => "c_raw_no_hint",
    }
}

impl RegistrationPath {
    /// Does this path claim the CLOSURE buffer (`RX_BUF`) rather than the type's
    /// own bound?
    ///
    /// The two `true` rows are exactly issue 1319's gap. Stated as a predicate
    /// here so W5's arena term asks the question once instead of matching every
    /// variant in each of its call sites — and so adding a path has to answer
    /// it. Phase-454 W5 added one and this is where it had to.
    ///
    /// `false` is NOT "claims the type's bound" for every row: it is "does not
    /// claim `RX_BUF`". [`Self::RustTypedInPlace`] claims no region at all and
    /// is priced at the bound anyway, which is an over-statement its own doc
    /// argues for.
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
        // phase-454 W5 — and the row 1319's analysis did not have. It claims no
        // region at all, which is the opposite direction from the two above.
        assert!(!RegistrationPath::RustTypedInPlace.claims_closure_buffer());
    }

    /// Every path must ANSWER the predicate, and the five spellings must stay
    /// distinct. A sixth row added without a decision is what this catches:
    /// the macro gives it a spelling for free, and `claims_closure_buffer`'s
    /// `matches!` would silently answer `false` for it — which is the
    /// under-sizing direction (issue 1319).
    #[test]
    fn every_registration_path_is_classified_and_spelled_once() {
        assert_eq!(RegistrationPath::ALL.len(), 5);
        let mut tags: Vec<&str> = RegistrationPath::ALL.iter().map(|p| p.tag()).collect();
        tags.sort_unstable();
        tags.dedup();
        assert_eq!(tags.len(), RegistrationPath::ALL.len());
        assert_eq!(
            RegistrationPath::ALL
                .iter()
                .filter(|p| p.claims_closure_buffer())
                .count(),
            2,
            "a new registration path changed which rows claim RX_BUF -- that is \
             a sizing decision, not a spelling (issue 1319)"
        );
    }
}
