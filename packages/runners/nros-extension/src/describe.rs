//! The `describe` reply (RFC-0119 D6).
//!
//! `<bin> describe` prints one JSON object on stdout:
//!
//! ```json
//! {"protocol": 1, "verbs": ["run", "console"], "needs": ["elf", "nic"]}
//! ```
//!
//! `protocol` is a number when the runner speaks one version and an array when
//! it speaks several; both forms are read.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    plan::{PLACEHOLDERS, Placeholder, Verb},
    protocol::{PROTOCOL, ProtocolMismatch, negotiate},
};

/// What a runner says about itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Describe {
    /// The protocol versions the runner speaks.
    #[serde(serialize_with = "ser_protocol", deserialize_with = "de_protocol")]
    pub protocol: Vec<u32>,
    /// The verbs it implements.
    pub verbs: Vec<Verb>,
    /// The plan placeholders it consumes (RFC-0118 D5).
    #[serde(default)]
    pub needs: Vec<String>,
}

fn ser_protocol<S: Serializer>(v: &[u32], s: S) -> Result<S::Ok, S::Error> {
    match v {
        [one] => s.serialize_u32(*one),
        many => many.serialize(s),
    }
}

fn de_protocol<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u32>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(u32),
        Many(Vec<u32>),
    }
    Ok(match OneOrMany::deserialize(d)? {
        OneOrMany::One(v) => vec![v],
        OneOrMany::Many(v) => v,
    })
}

/// A `describe` reply that is not a valid one.
#[derive(Debug)]
pub enum DescribeError {
    /// Not JSON of the right shape.
    Json(serde_json::Error),
    /// States no protocol version.
    NoProtocol,
    /// Implements no verb.
    NoVerbs,
    /// Needs a placeholder RFC-0118 D5 does not define.
    UnknownNeed(String),
}

impl fmt::Display for DescribeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DescribeError::Json(e) => write!(f, "the describe reply is not valid JSON: {e}"),
            DescribeError::NoProtocol => f.write_str("the describe reply states no protocol"),
            DescribeError::NoVerbs => f.write_str("the describe reply lists no verb"),
            DescribeError::UnknownNeed(n) => write!(
                f,
                "the describe reply needs {n:?}, which is not a plan placeholder (known: {}, \
                 tool:<name>)",
                PLACEHOLDERS.join(", ")
            ),
        }
    }
}

impl std::error::Error for DescribeError {}

impl Describe {
    /// Start a reply that speaks this crate's [`PROTOCOL`].
    #[must_use]
    pub fn builder() -> DescribeBuilder {
        DescribeBuilder {
            inner: Describe {
                protocol: vec![PROTOCOL],
                verbs: Vec::new(),
                needs: Vec::new(),
            },
        }
    }

    /// The reply as one line of JSON.
    #[must_use]
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("a Describe always serialises")
    }

    /// Print the reply on stdout, which is the whole of `<bin> describe`.
    pub fn print(&self) {
        println!("{}", self.to_json());
    }

    /// Parse and validate a reply, as nros does after running `describe`.
    ///
    /// # Errors
    ///
    /// A [`DescribeError`].
    pub fn parse(text: &str) -> Result<Describe, DescribeError> {
        let d: Describe = serde_json::from_str(text.trim()).map_err(DescribeError::Json)?;
        d.validate()?;
        Ok(d)
    }

    /// Check what serde alone cannot.
    ///
    /// # Errors
    ///
    /// A [`DescribeError`].
    pub fn validate(&self) -> Result<(), DescribeError> {
        if self.protocol.is_empty() {
            return Err(DescribeError::NoProtocol);
        }
        if self.verbs.is_empty() {
            return Err(DescribeError::NoVerbs);
        }
        if let Some(bad) = self.needs.iter().find(|n| Placeholder::parse(n).is_none()) {
            return Err(DescribeError::UnknownNeed(bad.clone()));
        }
        Ok(())
    }

    /// The protocol version nros and this runner will use.
    ///
    /// # Errors
    ///
    /// [`ProtocolMismatch`], naming both sides.
    pub fn negotiate(&self, ours: &[u32]) -> Result<u32, ProtocolMismatch> {
        negotiate(ours, &self.protocol)
    }

    /// Whether the runner implements `verb`.
    #[must_use]
    pub fn supports(&self, verb: Verb) -> bool {
        self.verbs.contains(&verb)
    }
}

/// Builds a [`Describe`]. Verbs and needs keep their first-given order and
/// drop duplicates.
#[derive(Debug, Clone)]
pub struct DescribeBuilder {
    inner: Describe,
}

impl DescribeBuilder {
    /// Add a verb.
    #[must_use]
    pub fn verb(mut self, verb: Verb) -> Self {
        if !self.inner.verbs.contains(&verb) {
            self.inner.verbs.push(verb);
        }
        self
    }

    /// Add a placeholder the runner consumes.
    #[must_use]
    pub fn need(mut self, name: &str) -> Self {
        if !self.inner.needs.iter().any(|n| n == name) {
            self.inner.needs.push(name.to_string());
        }
        self
    }

    /// Replace the protocol list, for a runner that speaks several versions.
    #[must_use]
    pub fn protocols(mut self, versions: &[u32]) -> Self {
        self.inner.protocol = versions.to_vec();
        self
    }

    /// Finish.
    ///
    /// # Panics
    ///
    /// When the result is not a valid reply (no verb, an unknown need): that
    /// is a bug in the runner, found the first time it runs `describe`.
    #[must_use]
    pub fn build(self) -> Describe {
        if let Err(e) = self.inner.validate() {
            panic!("invalid describe reply: {e}");
        }
        self.inner
    }

    /// Finish without panicking.
    ///
    /// # Errors
    ///
    /// A [`DescribeError`].
    pub fn try_build(self) -> Result<Describe, DescribeError> {
        self.inner.validate()?;
        Ok(self.inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_protocol_serialises_as_a_number() {
        let d = Describe::builder()
            .verb(Verb::Run)
            .verb(Verb::Console)
            .verb(Verb::Run)
            .need("elf")
            .need("nic")
            .build();
        assert_eq!(
            d.to_json(),
            r#"{"protocol":1,"verbs":["run","console"],"needs":["elf","nic"]}"#
        );
        assert_eq!(Describe::parse(&d.to_json()).unwrap(), d);
    }

    #[test]
    fn several_protocols_serialise_as_an_array() {
        let d = Describe::builder()
            .protocols(&[1, 2])
            .verb(Verb::Flash)
            .build();
        assert!(
            d.to_json().starts_with(r#"{"protocol":[1,2]"#),
            "{}",
            d.to_json()
        );
        assert_eq!(Describe::parse(&d.to_json()).unwrap().protocol, vec![1, 2]);
        assert_eq!(d.negotiate(&[1]), Ok(1));
        assert!(d.negotiate(&[3]).is_err());
    }

    #[test]
    fn invalid_replies_are_refused() {
        assert!(matches!(
            Describe::parse(r#"{"protocol":1,"verbs":[]}"#),
            Err(DescribeError::NoVerbs)
        ));
        assert!(matches!(
            Describe::parse(r#"{"protocol":[],"verbs":["run"]}"#),
            Err(DescribeError::NoProtocol)
        ));
        assert!(matches!(
            Describe::parse(r#"{"protocol":1,"verbs":["run"],"needs":["home"]}"#),
            Err(DescribeError::UnknownNeed(_))
        ));
        assert!(matches!(
            Describe::parse(r#"{"protocol":1,"verbs":["debug"]}"#),
            Err(DescribeError::Json(_))
        ));
        assert!(
            Describe::builder()
                .verb(Verb::Run)
                .need("tool:openocd")
                .try_build()
                .is_ok()
        );
    }
}
