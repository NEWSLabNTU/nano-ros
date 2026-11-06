//! The sizing inputs a descriptor does not state, which a consumer then turns
//! into a builtin default or a worst case — the ONE list, reported ONCE.
//!
//! RFC-0100 D6 says a fallback is "always LOUD". It used to be loud in every
//! build script that took one: three crates (`nros-node`, `nros-rmw-zenoh`,
//! `zpico-sys`) each printed a `cargo:warning` for the fact they read, so one
//! missing `durability` on a scaffolded talker printed four warnings from two
//! crates and a hello world printed six (issue 2270). Each of those warnings
//! was a function of the DESCRIPTOR alone, so whoever writes the descriptor can
//! say all of it at once, before any build script runs. That is where it is
//! said now — the `nros` CLI prints [`defaults_in_use`] as one summary line
//! beside the descriptor it wrote, and in full on request — and the build
//! scripts no longer repeat it.
//!
//! What stays in the build scripts is what this list cannot know: a stated
//! knob below the declared demand, a `KEEP_LAST(0)`, a road input (a receive
//! ceiling from the transport config). Those are about the build, not about
//! the declaration.
//!
//! Every entry calls the same rule the consumer calls (the functions in the
//! crate root and the section accessors), so this list and the pools it speaks
//! for cannot disagree about which fact is missing.

use crate::{Fact, SizingDescriptor};

/// One consequence of an input the descriptor does not state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultInUse {
    /// What keeps its builtin, or is sized for the worst case. Several
    /// consequences of ONE missing fact share an entry.
    pub consequences: Vec<&'static str>,
    /// Why — the descriptor's own reading of the missing fact.
    pub reason: String,
    /// What the user writes to state it. `None`: no declaration can bound it
    /// (an unbounded `string` member is a property of the message type), and
    /// the default is the right answer, so the entry is a note, not a to-do.
    pub remedy: Option<&'static str>,
}

/// Every input this descriptor leaves to a default, grouped by cause.
///
/// Empty means every consumer sizes from a declaration.
pub fn defaults_in_use(desc: &SizingDescriptor) -> Vec<DefaultInUse> {
    let mut out: Vec<DefaultInUse> = Vec::new();
    let mut add = |consequence: &'static str, reason: String, remedy: Option<&'static str>| {
        if let Some(e) = out.iter_mut().find(|e| e.reason == reason) {
            if !e.consequences.contains(&consequence) {
                e.consequences.push(consequence);
            }
            return;
        }
        out.push(DefaultInUse {
            consequences: vec![consequence],
            reason,
            remedy,
        });
    };

    // nros-node: the receive buffer is sized from the largest type the image
    // can carry.
    if let Some(why) = desc.types.max_wire_bound_bytes().refusal() {
        add(
            "the receive buffer (RX_BUF) keeps its crate default",
            why.to_string(),
            None,
        );
    }

    // nros-rmw-zenoh + zpico-sys: a publisher with no stated durability may be
    // latched, so both pools count it as one.
    const DURABILITY: Option<&str> = Some(
        "state `qos: { durability: volatile }` (or `transient_local`) for each publisher in \
         the `system.contract.yaml` beside `system.toml`",
    );
    if let Fact::Refused(why) = crate::transient_local_publishers(desc) {
        add(
            "the zenoh queryable table and the transient-local retention pool are sized for \
             the worst case, counting each such publisher as transient-local",
            why,
            DURABILITY,
        );
    }
    if let Fact::Refused(why) = crate::transient_local_retain_bytes(desc) {
        add(
            "each transient-local retention slot keeps its builtin size",
            why,
            DURABILITY,
        );
    }
    if let Fact::Refused(why) = crate::transient_local_retain_depth(desc) {
        add(
            "each transient-local publisher retains one sample",
            why,
            DURABILITY,
        );
    }

    // nros-rmw-zenoh: the per-subscription receive ring.
    if let Fact::Refused(why) = crate::max_subscription_depth(desc) {
        add(
            "each subscription's receive ring keeps its default depth",
            why,
            Some(
                "state `qos: { depth: N }` for each subscription in the bringup's \
                 `system.contract.yaml`",
            ),
        );
    }

    // nros-rmw-zenoh: the parameter services' inbox slot.
    if desc.params.service_shape().into_stated().is_none() {
        add(
            "the parameter services' inbox keeps its builtin slot, so a larger \
             `set_parameters` is dropped with no reply",
            "the contract declares no `params:` for this image's nodes".to_string(),
            Some(
                "declare each node's `params:` in the `system.contract.yaml` beside `system.toml` \
                 (`params: {}` for a node that has none)",
            ),
        );
    }

    out
}

/// The one-line form of [`defaults_in_use`], or `None` when there is nothing
/// to say. `how_to_see_more` is the caller's spelling of the detailed view.
pub fn summary_line(entries: &[DefaultInUse], how_to_see_more: &str) -> Option<String> {
    if entries.is_empty() {
        return None;
    }
    let actionable = entries.iter().filter(|e| e.remedy.is_some()).count();
    let n = entries.len();
    let (noun, verb) = if n == 1 {
        ("input", "uses")
    } else {
        ("inputs", "use")
    };
    let tail = match actionable {
        0 => "the defaults are safe and nothing needs declaring".to_string(),
        a if a == n => "the defaults are safe; declaring them saves RAM".to_string(),
        a => format!("the defaults are safe; declaring {a} of them saves RAM"),
    };
    Some(format!(
        "{n} {noun} {verb} a default ({tail}) -- details: {how_to_see_more}"
    ))
}

/// The detailed form: one block per cause.
pub fn explain(entries: &[DefaultInUse]) -> String {
    let mut s = String::new();
    for e in entries {
        // A refusal ends in a list of types or a hint; its first sentence is
        // the reason, and the rest is detail worth keeping.
        s.push_str(&format!("  - {}\n", e.reason));
        for c in &e.consequences {
            s.push_str(&format!("      so {c}\n"));
        }
        match e.remedy {
            Some(r) => s.push_str(&format!("      to declare it: {r}\n")),
            None => s.push_str("      nothing to declare: the default is the right answer\n"),
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(remedy: Option<&'static str>) -> DefaultInUse {
        DefaultInUse {
            consequences: vec!["x"],
            reason: "r".into(),
            remedy,
        }
    }

    #[test]
    fn the_summary_is_silent_when_everything_is_declared() {
        assert_eq!(summary_line(&[], "see"), None);
    }

    #[test]
    fn the_summary_counts_what_a_declaration_can_fix() {
        let one_note = summary_line(&[entry(None)], "see").unwrap();
        assert!(one_note.starts_with("1 input uses"), "{one_note}");
        assert!(one_note.contains("nothing needs declaring"), "{one_note}");

        let mixed = summary_line(&[entry(None), entry(Some("do"))], "see").unwrap();
        assert!(mixed.contains("declaring 1 of them"), "{mixed}");
        assert!(mixed.ends_with("details: see"), "{mixed}");
    }

    #[test]
    fn the_summary_names_no_tracker_id() {
        let line = summary_line(&[entry(Some("do"))], "see").unwrap();
        assert!(!line.contains("issue"), "{line}");
    }
}
