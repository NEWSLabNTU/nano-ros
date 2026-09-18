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
    /// `[[endpoint]] registration_path` — issue 1319, RFC-0100 D1, phase-456 W8.
    ///
    /// **A subscription's SLOT SIZE is a function of this and nothing else can
    /// supply it.** Issue 1319 measured four rows from the source; phase-454 W5
    /// ran them and found a FIFTH. Phase-456 W8 found that two of the five were
    /// not axes of the registration at all, and the vocabulary is THREE:
    ///
    /// | path | slot size |
    /// | --- | --- |
    /// | `in_place` | **no receive region at all** |
    /// | `typed_bound` | the type's own bound — at or below the model |
    /// | `unbounded` | **`RX_BUF`** |
    ///
    /// ## Why five became three
    ///
    /// The five rows decomposed into three properties of the registration and
    /// one that is not a property of it:
    ///
    /// | axis | values | decided by |
    /// | --- | --- | --- |
    /// | is the type's bound known? | yes / no | the CALL SITE, via the hint |
    /// | does the backend dispatch in place? | yes / no | `supports_process_in_place()` |
    /// | is the schema reachable? | yes / no | the backend's descriptor support |
    /// | *who called* | *Rust / C* | *nothing about the subscription* |
    ///
    /// `c_typed_hint` and `rust_typed_descriptors` described the SAME
    /// registration and differed only in the fourth; they are
    /// [`Self::TypedBound`]. `rust_typed_schemaless` and `c_raw_no_hint` are
    /// both "no bound reachable at this site"; they are [`Self::Unbounded`].
    /// `rust_typed_in_place` loses the language word it never earned and
    /// becomes [`Self::InPlace`].
    ///
    /// The language has NOT stopped being EVIDENCE — the descriptor writer
    /// cannot see a call site, so it still infers "did this site state a bound"
    /// from what the entry's language makes reachable, and phase-456 W7 is what
    /// makes that inference sound for C++. What it has stopped being is a NAME
    /// in this vocabulary, and therefore a thing a reader can mistake for a
    /// fact about the runtime.
    ///
    /// The field stays REQUIRED: the `unbounded` row is 1,848 bytes per
    /// subscription the model does not hold on the reference island at depth 1,
    /// in the UNDER direction, which is the one that ships `BufferTooSmall`.
    RegistrationPath {
        /// The backend dispatches the sample IN PLACE — zenoh and XRCE, whose
        /// `supports_process_in_place` is an unconditional `true` — so **no
        /// receive region is allocated at all**, whatever the type's bound or
        /// the image's `RX_BUF` say.
        ///
        /// **MEASURED, phase-454 W5.** The executor tests the capability BEFORE
        /// it computes a slot size and returns through an in-place entry when
        /// it holds; an explicit `.rx_buffer::<N>()` on such a backend is not
        /// even read. Measured on `contract-monitor-sub` over zenoh: a
        /// `std_msgs/Header` subscription claims **672 bytes** of arena, against
        /// the 9,768-byte region the model budgets it at `KEEP_LAST(10)`.
        ///
        /// **Phase-456 W8 made this row reachable from C and C++ too.** Until
        /// W8 the C registration path never consulted the capability, so every
        /// C/C++ subscription on zenoh or XRCE allocated a region the backend
        /// does not need. The capability now has exactly one consulting site and
        /// every language reaches it, which is why this row no longer carries a
        /// `rust_` prefix.
        ///
        /// This row is still priced at the type's bound rather than at zero —
        /// an OVER-statement, deliberately left where it was. Lowering it is
        /// worth ~9.7 KiB a subscription and is issue 1340.
        InPlace => "in_place",
        /// The site stated a bound and the backend BUFFERS, so the receive
        /// region is sized from the type's own bound.
        ///
        /// Reached two ways that used to be two rows: a C or C++ site supplying
        /// `nros::rx_size_bound<M>` (phase-456 W7 makes every registration site
        /// in the nros-cpp headers state one), and a Rust typed site against a
        /// backend that carries type descriptors (Cyclone), where the bound is
        /// reachable from `MessageForRmw`.
        TypedBound => "typed_bound",
        /// No bound is reachable at this site, so the registration takes the
        /// image-wide closure buffer (`RX_BUF`).
        ///
        /// Two ways, which used to be two rows: a Rust typed site against a
        /// SCHEMALESS buffering backend — no schema on `MessageForRmw`, so no
        /// bound exists to state at the type-erased site — and a genuinely
        /// type-erased C site that states nothing (`nros::rx_bound_unknown`,
        /// which phase-456 W7 made the only way to spell it out loud).
        ///
        /// Issue 1319 attributed the schemaless half to zenoh and XRCE. They are
        /// [`Self::InPlace`] instead, measured; this half stands for a
        /// schemaless backend that does not dispatch in place, which is a
        /// configuration the tree admits and does not currently ship.
        Unbounded => "unbounded",
    }
}

impl RegistrationPath {
    /// Does this path claim the CLOSURE buffer (`RX_BUF`) rather than the type's
    /// own bound?
    ///
    /// This is issue 1319's gap, and since phase-456 W8 it is answerable from
    /// the REGISTRATION's arguments — did the site state a bound, and does the
    /// backend buffer — rather than from who called. Stated as a predicate here
    /// so W5's arena term asks the question once instead of matching every
    /// variant in each of its call sites, and so adding a path has to answer it.
    ///
    /// `false` is NOT "claims the type's bound" for every row: it is "does not
    /// claim `RX_BUF`". [`Self::InPlace`] claims no region at all and is priced
    /// at the bound anyway, which is an over-statement its own doc argues for.
    pub const fn claims_closure_buffer(self) -> bool {
        matches!(self, RegistrationPath::Unbounded)
    }
}

/// Whether any declared parameter needs a BOARD capacity, and which one does.
///
/// `[params] needs_max_string_value_len` and its two siblings. The three
/// capacities themselves — `MAX_STRING_VALUE_LEN`, `MAX_ARRAY_LEN`,
/// `MAX_BYTE_ARRAY_LEN` — are deliberately NOT in `[params]`, and putting them
/// there is the change a future reader will want to make. They are RFC-0100 D1
/// **target** facts, owned by the board descriptor's `[board.knobs.params]`,
/// because an MCU and a PC want different string lengths for the same node. A
/// contract can only say whether a capacity is needed AT ALL; the number is the
/// board's, and the build refuses when the board states none. So the image
/// states the NEED and names who has it, and nothing more.
///
/// # `Unused` is a value, not an absent key
///
/// [`Self::Unused`] is a STATEMENT — *"no declared parameter has a type that
/// uses this knob"* — and is therefore a VALUE, carried by a
/// [`Stated`](crate::Fact::Stated). An absent key is
/// [`Absent`](crate::Fact::Absent), *"nobody said"*, which is the different
/// answer an image with no parameter declarations gives. Conflating the two is
/// the "absence is not zero" rule this whole schema exists to hold —
/// `Meta::undeclared_endpoints` carries the same distinction one section over —
/// and a consumer that read `Unused` as *absent* would keep its builtin
/// capacity for an image that has just told it the knob buys nothing.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CapacityNeed {
    /// No declared parameter has a type that uses this capacity.
    Unused,
    /// This declared parameter needs it — the first, in `(node, name)` order,
    /// which is the order the derivation walks. NAMED rather than counted
    /// because the number is the BOARD's: a build with no capacity to resolve
    /// against has to be able to say whose declaration it could not price.
    NeededBy {
        /// The node's name, as the contract spells it.
        node: String,
        /// The parameter's name, as the contract spells it.
        name: String,
    },
}

impl CapacityNeed {
    /// The spelling of [`Self::Unused`].
    pub const UNUSED_TAG: &'static str = "unused";
    /// What a [`Self::NeededBy`] spelling starts with.
    pub const NEEDED_BY_PREFIX: &'static str = "needed_by:";

    /// The canonical spelling, as it appears in the descriptor.
    ///
    /// A `String` rather than the `&'static str` every other vocabulary in this
    /// file returns, because this one carries data. That is also why it is not
    /// built by the `vocabulary!` macro: the macro's rows are fieldless.
    pub fn token(&self) -> String {
        match self {
            CapacityNeed::Unused => Self::UNUSED_TAG.to_string(),
            CapacityNeed::NeededBy { node, name } => {
                format!("{}{node}:{name}", Self::NEEDED_BY_PREFIX)
            }
        }
    }

    /// Parse one spelling. An unknown one is an error naming the legal forms —
    /// never a `None` the caller can drop and never a best effort, for
    /// [`History`]'s reason: a skipped row is exactly an under-report.
    pub fn parse(s: &str) -> Result<Self, String> {
        if s == Self::UNUSED_TAG {
            return Ok(CapacityNeed::Unused);
        }
        if let Some(rest) = s.strip_prefix(Self::NEEDED_BY_PREFIX)
            && let Some((node, name)) = rest.split_once(':')
            && !node.is_empty()
            && !name.is_empty()
        {
            return Ok(CapacityNeed::NeededBy {
                node: node.to_string(),
                name: name.to_string(),
            });
        }
        Err(format!(
            "unknown CapacityNeed `{s}` -- expected `{}` or `{}<node>:<name>`, \
             both parts non-empty",
            Self::UNUSED_TAG,
            Self::NEEDED_BY_PREFIX,
        ))
    }

    /// Does this capacity buy the image nothing?
    pub fn is_unused(&self) -> bool {
        matches!(self, CapacityNeed::Unused)
    }

    /// `(node, name)` of the declaration that needs this capacity, or `None`
    /// when nothing does.
    pub fn needed_by(&self) -> Option<(&str, &str)> {
        match self {
            CapacityNeed::Unused => None,
            CapacityNeed::NeededBy { node, name } => Some((node.as_str(), name.as_str())),
        }
    }
}

impl fmt::Display for CapacityNeed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.token())
    }
}

impl serde::Serialize for CapacityNeed {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.token())
    }
}

impl<'de> serde::Deserialize<'de> for CapacityNeed {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = <std::borrow::Cow<'de, str> as serde::Deserialize>::deserialize(d)?;
        CapacityNeed::parse(raw.as_ref()).map_err(serde::de::Error::custom)
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

    /// The one vocabulary the macro does not build, held to the same rule.
    #[test]
    fn a_capacity_need_round_trips_through_its_spelling() {
        for v in [
            CapacityNeed::Unused,
            CapacityNeed::NeededBy {
                node: "/a".into(),
                name: "label".into(),
            },
        ] {
            assert_eq!(CapacityNeed::parse(&v.token()).unwrap(), v, "{v}");
        }
        assert_eq!(CapacityNeed::Unused.token(), "unused");
        assert_eq!(
            CapacityNeed::NeededBy {
                node: "/a".into(),
                name: "label".into(),
            }
            .token(),
            "needed_by:/a:label"
        );
    }

    /// `Unused` says something; it is not the absence of an answer. The `Fact`
    /// that carries it is what spells absence, and these two accessors are how a
    /// consumer tells "this knob buys nothing" from "nobody looked".
    #[test]
    fn unused_and_needed_by_are_different_statements() {
        assert!(CapacityNeed::Unused.is_unused());
        assert_eq!(CapacityNeed::Unused.needed_by(), None);
        let needed = CapacityNeed::NeededBy {
            node: "/talker".into(),
            name: "greeting".into(),
        };
        assert!(!needed.is_unused());
        assert_eq!(needed.needed_by(), Some(("/talker", "greeting")));
    }

    #[test]
    fn an_unknown_capacity_need_spelling_is_an_error_naming_the_legal_forms() {
        for bad in [
            "maybe",
            "needed_by",
            "needed_by:",
            "needed_by:/a",
            "needed_by::name",
            "needed_by:/a:",
            "",
        ] {
            let err = CapacityNeed::parse(bad)
                .map(|v| v.token())
                .expect_err(&format!("`{bad}` must not parse"));
            assert!(err.contains("unused"), "{err}");
            assert!(err.contains("needed_by:"), "{err}");
        }
    }

    #[test]
    fn an_unknown_spelling_is_an_error_naming_the_legal_set() {
        let err = History::parse("keep_some").unwrap_err();
        assert!(err.contains("keep_some"), "{err}");
        assert!(err.contains("keep_last"), "{err}");
        assert!(err.contains("keep_all"), "{err}");
    }

    /// Issue 1319's gap is ONE row since phase-456 W8, and it is the row that
    /// says the site stated no bound — not the row that says who called.
    #[test]
    fn the_closure_buffer_path_is_the_one_that_states_no_bound() {
        assert!(RegistrationPath::Unbounded.claims_closure_buffer());
        assert!(!RegistrationPath::TypedBound.claims_closure_buffer());
        // phase-454 W5 — it claims no region at all, which is the opposite
        // direction from `Unbounded`, and is priced at the bound anyway.
        assert!(!RegistrationPath::InPlace.claims_closure_buffer());
    }

    /// Every path must ANSWER the predicate, and the three spellings must stay
    /// distinct. A fourth row added without a decision is what this catches:
    /// the macro gives it a spelling for free, and `claims_closure_buffer`'s
    /// `matches!` would silently answer `false` for it — which is the
    /// under-sizing direction (issue 1319).
    ///
    /// The count is THREE because phase-456 W8 removed the language axis: a row
    /// named for a caller rather than for a property of the registration is how
    /// five got here, and re-adding one should have to move this number.
    #[test]
    fn every_registration_path_is_classified_and_spelled_once() {
        assert_eq!(RegistrationPath::ALL.len(), 3);
        let mut tags: Vec<&str> = RegistrationPath::ALL.iter().map(|p| p.tag()).collect();
        tags.sort_unstable();
        tags.dedup();
        assert_eq!(tags.len(), RegistrationPath::ALL.len());
        assert_eq!(
            RegistrationPath::ALL
                .iter()
                .filter(|p| p.claims_closure_buffer())
                .count(),
            1,
            "a new registration path changed which rows claim RX_BUF -- that is \
             a sizing decision, not a spelling (issue 1319)"
        );
        // No row may name a LANGUAGE. That is the whole of W8's finding, and a
        // grep is the only thing that keeps it from coming back one row at a
        // time.
        for p in RegistrationPath::ALL {
            let tag = p.tag();
            assert!(
                !tag.starts_with("c_") && !tag.starts_with("rust_"),
                "`{tag}` names who called, which is not a property of the \
                 registration (phase-456 W8)"
            );
        }
    }
}
