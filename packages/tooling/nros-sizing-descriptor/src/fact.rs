//! Per-FIELD status — RFC-0100 D6.
//!
//! The tree already answers a derived question with a two-variant enum rather
//! than an `Option`: `Derivation::Derived | Refused`, `ReceivedTypes::Resolved |
//! Refused`, `DeclaredDepths::Resolved | Refused`, `PayloadClasses::Derived |
//! Refused`. Each carries prose and NO number on the refusing arm, so a consumer
//! either reads a value the producer derived or reads nothing at all.
//!
//! [`Fact`] is that discipline at FIELD granularity, which is what a shared
//! descriptor forces. D6:
//!
//! > Today refusal is coarse: all five `NROS_ENTITY_COUNT_*` or the whole arena
//! > falls to the undeclared branch. That is wrong for a shared descriptor — a
//! > `keep_all` subscription says nothing about Cyclone's type table, and a
//! > global refusal would degrade it anyway. **Each derived field carries its
//! > own status.**
//!
//! # Why THREE variants and not two
//!
//! `Refused` and `Absent` are different facts and the existing types already say
//! so in prose — `EntityDecl::depth` is `Option` because "`None` means NOBODY
//! SAID, and a consumer that needs a depth must refuse on it. It must never read
//! as 0." A refusal is the producer saying *I looked and there is no number, here
//! is why*; an absence is *nobody stated this and I had no reason to derive it*.
//! Collapsing them loses the prose exactly where a user needs it, and a consumer
//! that must fall back wants to print a reason when one exists.
//!
//! What the two share is the only thing that matters at a size: neither yields a
//! value. [`Fact::stated`] is the sole accessor that produces one, so there is no
//! spelling of "read it, and if that fails use 10" that does not go through a
//! `match`.

use core::fmt;

/// One field of the descriptor, with its own status.
///
/// `T` is always a plain value type — a `usize`, a vocabulary enum. The refusal
/// carries prose and no number, per D6.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fact<T> {
    /// The producer derived or read this value. The only arm that yields one.
    Stated(T),
    /// The producer could not derive it and says why.
    ///
    /// D6's triggers: `history = keep_all` kills the depth-derived fields,
    /// `undeclared_endpoints != 0` kills the per-endpoint attributions, an
    /// unbounded or unpriced type kills the payload-class fields, and an absent
    /// `[target]` kills the storage-size fields.
    Refused(String),
    /// Nobody stated it and nothing derived it. No prose, because there is no
    /// event to describe.
    Absent,
}

impl<T> Fact<T> {
    /// The value, or `None`. **The only way to one.**
    pub fn stated(&self) -> Option<&T> {
        match self {
            Fact::Stated(v) => Some(v),
            Fact::Refused(_) | Fact::Absent => None,
        }
    }

    /// [`Self::stated`], consuming.
    pub fn into_stated(self) -> Option<T> {
        match self {
            Fact::Stated(v) => Some(v),
            Fact::Refused(_) | Fact::Absent => None,
        }
    }

    /// The refusal prose, when this field was refused.
    ///
    /// `None` for `Absent` as well as for `Stated`: a consumer printing "why can
    /// I not have this?" has something to say only in the refusing case.
    pub fn refusal(&self) -> Option<&str> {
        match self {
            Fact::Refused(r) => Some(r.as_str()),
            Fact::Stated(_) | Fact::Absent => None,
        }
    }

    /// The status word, for a report or a `--json` projection.
    pub fn tag(&self) -> &'static str {
        match self {
            Fact::Stated(_) => "stated",
            Fact::Refused(_) => "refused",
            Fact::Absent => "absent",
        }
    }

    /// Is there a number here?
    pub fn is_stated(&self) -> bool {
        matches!(self, Fact::Stated(_))
    }
}

impl<T: Copy> Fact<T> {
    /// The value, copied.
    pub fn get(&self) -> Option<T> {
        match self {
            Fact::Stated(v) => Some(*v),
            Fact::Refused(_) | Fact::Absent => None,
        }
    }

    /// Take `fallback` when this field yields no value, and say so.
    ///
    /// Returns `(value, Some(explanation))` on the fallback path and
    /// `(value, None)` when the descriptor answered. The explanation is what a
    /// build script prints as a `cargo::warning`, which is D6's second half:
    ///
    /// > Worst case when refused, always the safe direction and always **loud**
    /// > — the build prints what declaring would save.
    ///
    /// A caller that wants the value and does not want to say anything writes
    /// `.get().unwrap_or(x)` and owns that choice at the call site. This
    /// function exists so that the loud path is the short one.
    pub fn or_report(&self, field: &str, fallback: T) -> (T, Option<String>) {
        match self {
            Fact::Stated(v) => (*v, None),
            Fact::Refused(reason) => (
                fallback,
                Some(format!("sizing descriptor refused `{field}`: {reason}")),
            ),
            Fact::Absent => (
                fallback,
                Some(format!(
                    "sizing descriptor states no `{field}`; nothing declared it"
                )),
            ),
        }
    }
}

impl<T: fmt::Display> fmt::Display for Fact<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Fact::Stated(v) => write!(f, "{v}"),
            Fact::Refused(r) => write!(f, "refused ({r})"),
            Fact::Absent => write!(f, "absent"),
        }
    }
}
