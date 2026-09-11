//! phase-439 W2 (RFC-0094 D1/D2) — stage 3.5, the resolve phase.
//!
//! `nros build` has five stages: discover, plan, preflight, emit root, exec.
//! This is the phase between preflight and emit-root. It
//!
//!   reads   the board / platform descriptor the image resolved to,
//!           the image declaration (`[image.*]` in the bringup's `system.toml`),
//!           and the wiring the launch tree states (the resolved SystemModel,
//!           which folds in the `<stem>.contract.yaml` sidecar),
//!   runs    [`EntityInventory::derive`] ONCE, for the whole image,
//!   writes  `build/<image>/resolved.toml`.
//!
//! **No compile. No configure. No cargo invocation.** Everything above is a
//! declaration on disk. That is not a stylistic preference, it is the reason
//! the phase can exist at all — see "the direction problem" below.
//!
//! # The direction problem, which is why this is a phase and not more forwarding
//!
//! `nros-rmw-zenoh` is a DEPENDENCY of the leaf, so the crate that must know the
//! entity counts compiles BEFORE the crate whose source declares them. No build
//! script, proc macro or manifest key reaches backwards across that edge. Issue
//! 0827 already moved the derivation out of the cargo graph for this reason;
//! RFC-0094 finishes the move by putting it before the build system rather than
//! inside it.
//!
//! The same shape, one layer over, is what the three-pass convergence in
//! `zephyr/cmake/nros_cargo_build.cmake` exists to paper over: the entity
//! inventory's producer (`nano_ros_entry()`) runs LATER in a configure than its
//! reader (`nros_resolve_knobs()`, inside `find_package(Zephyr)`). A producer
//! that runs BEFORE the configure has no such lag, which is what makes the
//! future-mtime arm in `cmake/NanoRosReconfigure.cmake` deletable.
//!
//! # Why the metadata probe cannot be the source
//!
//! `nros metadata` compiles a harness and RUNS it; on `qemu-arm-baremetal` that
//! is a Cortex-M cross build executed under QEMU over semihosting, and issue
//! 1061 measured that it cannot run at all for pure-cargo leaves. It may remain
//! as a cross-check. It must not be what this phase depends on, and it is not:
//! nothing here executes anything it built.
//!
//! `${CMAKE_BINARY_DIR}/nros-metadata.json` is out for a different reason — it
//! is written DURING a configure by `nano_ros_node_register()`, so a phase that
//! runs before the configure cannot read it without recreating the very lag it
//! exists to remove.
//!
//! # DEMAND, not size — the floor belongs to the consumer
//!
//! Everything under `[executor]` and `[pools]` is the image's raw DEMAND,
//! ZERO INCLUDED. Whether zero is a legal SIZE is a property of the STORAGE the
//! knob reaches, not of the count:
//!
//! * `ZPICO_MAX_QUERYABLES` sizes a fixed C array. At 0 a board transmitted
//!   nothing for 15 s with no diagnostic (issue 1015).
//! * `XRCE_MAX_SUBSCRIBERS` at 0 is 33,296 bytes of heap the image gets back,
//!   and is the correct answer (issue 1033).
//!
//! One [`EntityInventory::derive`] feeds both. A floor applied here silently
//! defeated 1033 the day after it landed, and every knob gate stayed green
//! because the number was derived correctly and delivered faithfully. So this
//! file publishes demand and the consumers floor it
//! ([`crate::entity_inventory::c_array_pool_floor`] on the Rust lane,
//! `_nros_c_array_pool_floor` on the CMake one).
//!
//! # `[provenance]` is normative
//!
//! Today "which number did this image build at, and why?" is answered by
//! cache-variable archaeology. Every value this file publishes carries a line
//! saying where it came from — and every knob that has NO declarative
//! derivation carries a line saying THAT, which is strictly better than being
//! hand-set and silent.

use std::{collections::BTreeMap, fmt::Write as _};

use sha2::{Digest, Sha256};

use crate::entity_inventory::{Derivation, DerivedEntityKnobs, EntityInventory};

/// The `resolved.toml` schema this writer emits and its readers understand.
///
/// A reader that finds anything else REFUSES rather than reading field by field
/// on the hope that nothing moved — the same rule
/// `NROS_MESSAGE_BOUNDS_SCHEMA_SUPPORTED` states for the bound inventory.
pub const RESOLVED_SCHEMA_VERSION: u32 = 1;

/// The canonical file name, so no reader hand-derives it.
pub const RESOLVED_TOML_NAME: &str = "resolved.toml";

/// The CMake projection's file name, beside it.
///
/// A second FILE, not a second DERIVATION: both are rendered from one
/// [`Resolved`] in one call to [`write`], exactly as
/// `EntityInventory` already renders JSON, CMake and env from one data model.
/// `resolved_toml_and_cmake_agree` is the test that holds them together.
pub const RESOLVED_CMAKE_NAME: &str = "resolved.cmake";

/// Which knobs have NO declarative derivation, and why.
///
/// RFC-0094 and phase-439 W2 both name these two explicitly as a KNOWN GAP
/// rather than a defect. Stating the gap in the artifact is the deliverable:
/// a hand-set knob that says it is hand-set can be audited, and one that does
/// not cannot.
///
/// Do not invent a derivation for either without measuring one. `ZPICO_MAX_
/// QUERYABLES` in particular has an addend this inventory cannot see — the ROS
/// parameter services claim 6 queryables and the REP-2002 lifecycle services 5,
/// before the application declares anything, and whether they are in the image
/// is a FEATURE of another crate that cargo exposes to nobody.
pub const HAND_SET_KNOBS: &[(&str, &str)] = &[
    (
        "ZPICO_MAX_QUERYABLES",
        "HAND-SET on the cargo-leaf road. `max_queryables` above counts the \
         runtime's parameter and lifecycle servers when the bringup declares \
         them (issue 1270), and the Zephyr lane takes it; a cargo leaf's \
         inventory is built from nros-metadata.json, which carries no bringup \
         features, so that road still cannot derive this knob. Issues 0827, \
         1061, 1125, 1270.",
    ),
    (
        "ZPICO_MAX_LARGE_SUBSCRIBERS",
        "HAND-SET: no declarative derivation. It is a count of payload BLOCKS, \
         which needs the message-bound inventory (a TYPE's size) joined to the \
         subscribed-type set — and the bound half is produced by codegen, not \
         by a declaration this phase reads. Issues 0827, 1061, 1125.",
    ),
];

/// The identity of the image whose knobs these are.
///
/// Every field is a DECLARATION: `entry` and `board` are what the bringup's
/// `[image.*]` states, `platform` is what the board catalog resolved it to.
/// None of them is read back off a compiled artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageIdent {
    /// The bringup package that declares this image.
    pub bringup: String,
    /// The `[image.<id>]` key.
    pub entry: String,
    /// The nano-ros board id, or empty when the image names none.
    pub board: String,
    /// The platform the board catalog resolved the board to.
    pub platform: String,
    /// The RMW backend, or empty when the image inherits the system header's.
    pub rmw: String,
}

impl ImageIdent {
    /// The directory name this image's artifacts live under.
    ///
    /// `<bringup>__<entry>`, which is RFC-0094 D1's `build/<image>/` with the
    /// bringup carried so two bringups declaring the same image id cannot land
    /// in one directory. Path-safe by construction: every character outside
    /// `[A-Za-z0-9_-]` becomes `-`, so a launch-argument-bearing id cannot
    /// escape the build tree. `.` is NOT in the allowed set even though it is a
    /// legal filename character — keeping it would let a `..` survive intact,
    /// and this string is joined onto a build root.
    #[must_use]
    pub fn dir_name(&self) -> String {
        let sanitize = |s: &str| -> String {
            s.chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() || matches!(c, '_' | '-') {
                        c
                    } else {
                        '-'
                    }
                })
                .collect()
        };
        format!("{}__{}", sanitize(&self.bringup), sanitize(&self.entry))
    }
}

/// One image's resolved answer, plus how it got there.
///
/// `Refused` carries prose and NO numbers, which is [`Derivation`]'s own
/// discipline and for its own reason: a consumer either reads a value this
/// phase derived or reads nothing at all. A substituted default that looks like
/// an answer is the silent shape this whole campaign exists to remove.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub image: ImageIdent,
    /// Where the declarations were read from — the model path, for the
    /// provenance line.
    pub source: String,
    /// The composed inventory's answer, run ONCE.
    pub derivation: Derivation,
    /// Per-kind entity counts, when derived.
    pub per_kind: BTreeMap<String, usize>,
    /// Per-component `(pkg, component, entities, slots)`, when derived — the
    /// provenance a reader needs to attribute a number to a declaration.
    pub per_component: Vec<(String, String, usize, usize)>,
}

impl Resolved {
    /// Compose one image's answer. [`EntityInventory::derive`] runs exactly
    /// once here and the result is carried, not re-derived per renderer.
    #[must_use]
    pub fn compose(image: ImageIdent, inventory: &EntityInventory) -> Self {
        let derivation = inventory.derive();
        let (per_kind, per_component) = match &derivation {
            Derivation::Derived(k) => (
                k.per_kind
                    .iter()
                    .map(|(name, n)| ((*name).to_string(), *n))
                    .collect(),
                k.per_component.clone(),
            ),
            Derivation::Refused { .. } => (BTreeMap::new(), Vec::new()),
        };
        Self {
            image,
            source: inventory.source.clone(),
            derivation,
            per_kind,
            per_component,
        }
    }

    /// An image whose declarations could not be reached at all.
    ///
    /// Distinct from a refusal the inventory itself raised: this one says the
    /// phase never got as far as an inventory (no launch, no model, no
    /// contract). Both write a file, and neither writes a number.
    #[must_use]
    pub fn unresolvable(image: ImageIdent, source: impl Into<String>, reason: String) -> Self {
        Self {
            image,
            source: source.into(),
            derivation: Derivation::Refused { reason },
            per_kind: BTreeMap::new(),
            per_component: Vec::new(),
        }
    }

    /// The knobs, when this image derived any.
    #[must_use]
    pub fn knobs(&self) -> Option<&DerivedEntityKnobs> {
        self.derivation.knobs()
    }

    /// `derived` | `refused`.
    #[must_use]
    pub fn status(&self) -> &'static str {
        self.derivation.tag()
    }

    /// A digest over every RESOLVED VALUE — not over the file, which carries
    /// comments and provenance prose that must be free to improve.
    ///
    /// RFC-0094 D2. The point is that two images whose knobs agree share a
    /// digest and two whose knobs differ do not, so the digest can key a shared
    /// artifact directory (D4) without the key depending on wording.
    ///
    /// 16 hex digits rather than the RFC's illustrative 8: a 32-bit digest over
    /// a whole tree's images collides at a few tens of thousands by the
    /// birthday bound, and a collision here silently hands one image another's
    /// numbers — issue 0616's shape, which is the failure D4 exists to prevent.
    #[must_use]
    pub fn digest(&self) -> String {
        let mut h = Sha256::new();
        h.update(RESOLVED_SCHEMA_VERSION.to_string());
        h.update(b"\x1fimage\x1f");
        for field in [
            &self.image.bringup,
            &self.image.entry,
            &self.image.board,
            &self.image.platform,
            &self.image.rmw,
        ] {
            h.update(field.as_bytes());
            h.update(b"\x1f");
        }
        h.update(b"\x1fstatus\x1f");
        h.update(self.status().as_bytes());
        // The VALUES, in a fixed order. A refusal contributes none, so every
        // refused image of one identity shares a digest — which is correct: it
        // is the same answer, namely none.
        if let Some(k) = self.knobs() {
            for (name, v) in self.value_rows() {
                h.update(b"\x1f");
                h.update(name.as_bytes());
                h.update(b"=");
                h.update(v.to_string().as_bytes());
            }
            // per-kind is provenance for the values above, but it is also the
            // only place a declaration change with no knob effect shows up, and
            // a consumer keyed on this digest wants to notice that.
            for (kind, n) in &self.per_kind {
                h.update(b"\x1f");
                h.update(kind.as_bytes());
                h.update(b"#");
                h.update(n.to_string().as_bytes());
            }
            let _ = k;
        }
        let out = h.finalize();
        out.iter().take(8).fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
    }

    /// The resolved values, in emission order. ONE list, so the TOML body, the
    /// CMake projection and the digest cannot disagree about what was resolved.
    #[must_use]
    pub fn value_rows(&self) -> Vec<(&'static str, usize)> {
        match self.knobs() {
            None => Vec::new(),
            Some(k) => vec![
                ("max_cbs", k.max_cbs),
                ("action_clients", k.heavy_slots),
                ("max_nodes", k.max_nodes),
                ("entity_total", k.entity_total),
                ("max_subscribers", k.max_subscribers),
                ("rmw_subscriber_slots", k.max_subscribers),
                ("max_publishers", k.max_publishers),
                ("max_queryables", k.max_queryables),
                ("max_liveliness", k.max_liveliness),
                ("max_cell_entities", k.max_cell_entities),
            ],
        }
    }

    /// `resolved.toml` — RFC-0094 D2's artifact.
    #[must_use]
    pub fn to_toml(&self) -> String {
        let mut s = String::new();
        s.push_str(
            "# GENERATED by `nros build` stage 3.5, the resolve phase (RFC-0094 D1/D2,\n\
             # phase-439 W2). Do not edit: the next build overwrites it.\n\
             #\n\
             # ONE PLACE DECIDES A KNOB, EVERY OTHER PLACE READS IT. This is that place\n\
             # for the counts an image's DECLARATIONS answer. It is written BEFORE any\n\
             # configure, from declarations only — no compile, no cargo, no cmake — which\n\
             # is what lets a reader that runs early in a configure see the final answer\n\
             # on its first pass.\n\
             #\n\
             # THESE NUMBERS ARE DEMAND, NOT SIZE. Zero is a legitimate demand. Whether\n\
             # zero is a legal size is a property of the storage the knob reaches, so the\n\
             # FLOOR belongs to the consumer that names the knob and never here: issue\n\
             # 1015 (a fixed C array at 0 = a board transmitting nothing for 15 s) and\n\
             # issue 1033 (heap at 0 = 33,296 bytes returned, and correct) are the same\n\
             # derived number answered two different ways.\n\
             #\n\
             # A derived value is a DEFAULT. An environment value or a Kconfig / board\n\
             # `.conf` value states a number and WINS.\n\n",
        );
        let _ = writeln!(s, "schema_version = {RESOLVED_SCHEMA_VERSION}");
        s.push('\n');

        s.push_str("[image]\n");
        let _ = writeln!(s, "bringup = {}", toml_str(&self.image.bringup));
        let _ = writeln!(s, "entry = {}", toml_str(&self.image.entry));
        let _ = writeln!(s, "board = {}", toml_str(&self.image.board));
        let _ = writeln!(s, "platform = {}", toml_str(&self.image.platform));
        let _ = writeln!(s, "rmw = {}", toml_str(&self.image.rmw));
        let _ = writeln!(s, "status = {}", toml_str(self.status()));
        let _ = writeln!(s, "digest = {}", toml_str(&self.digest()));
        s.push('\n');

        match &self.derivation {
            Derivation::Refused { reason } => {
                s.push_str(
                    "# REFUSED. No knob is derived and NO NUMBER is published: every one\n\
                     # keeps its configured value. A substituted default here would be\n\
                     # indistinguishable from an answer, which is the failure this file\n\
                     # exists to remove.\n",
                );
                s.push_str("[provenance]\n");
                let _ = writeln!(s, "source = {}", toml_str(&self.source));
                let _ = writeln!(s, "refused = {}", toml_str(reason));
                for (knob, why) in HAND_SET_KNOBS {
                    let _ = writeln!(s, "{} = {}", knob.to_ascii_lowercase(), toml_str(why));
                }
            }
            Derivation::Derived(k) => {
                s.push_str(
                    "# The executor's own table. A PUBLISHER CLAIMS NO SLOT — it writes an\n\
                     # RmwPublisher into caller storage and never reaches\n\
                     # Executor::next_entry_slot — so `entity_total` is larger than\n\
                     # `max_cbs`, and the gap is the finding rather than a discrepancy.\n",
                );
                s.push_str("[executor]\n");
                let _ = writeln!(s, "max_cbs = {}", k.max_cbs);
                let _ = writeln!(s, "action_clients = {}", k.heavy_slots);
                let _ = writeln!(s, "max_nodes = {}", k.max_nodes);
                let _ = writeln!(s, "entity_total = {}", k.entity_total);
                s.push('\n');

                s.push_str(
                    "# The RMW SESSION pools, which count what the executor's table does\n\
                     # not: a publisher claims a session slot but no callback slot, and a\n\
                     # declared action is ONE entity that costs SEVERAL session slots (a\n\
                     # server opens 3 queryables and 2 publishers, a client 1\n\
                     # subscription).\n",
                );
                s.push_str("[pools]\n");
                let _ = writeln!(s, "max_subscribers = {}", k.max_subscribers);
                let _ = writeln!(s, "rmw_subscriber_slots = {}", k.max_subscribers);
                let _ = writeln!(s, "max_publishers = {}", k.max_publishers);
                let _ = writeln!(s, "max_queryables = {}", k.max_queryables);
                // phase-412 W2 -- one liveliness token per session entity, plus
                // the node names. Local tokens only; the peer graph is not here.
                let _ = writeln!(s, "max_liveliness = {}", k.max_liveliness);
                s.push('\n');

                // Issue 1130 -- the component runtime's knob-capped cell
                // registries: the largest single kind in any one component.
                s.push_str(
                    "# The component runtime. Per-kind capacity of a cell whose class\n\
                     # states no ENTITY_BOUNDS; an explicit ENTITY_BOUNDS still wins.\n",
                );
                s.push_str("[runtime]\n");
                let _ = writeln!(s, "max_cell_entities = {}", k.max_cell_entities);
                s.push('\n');

                s.push_str("[entities]\n");
                for (kind, n) in &self.per_kind {
                    let _ = writeln!(s, "{kind} = {n}");
                }
                s.push('\n');

                s.push_str(
                    "# NORMATIVE, not decoration. Without it \"which number did this image\n\
                     # build at, and why?\" is answered by cache-variable archaeology.\n",
                );
                s.push_str("[provenance]\n");
                let _ = writeln!(s, "source = {}", toml_str(&self.source));
                let _ = writeln!(
                    s,
                    "max_cbs = {}",
                    toml_str(&format!(
                        "{} → {} declared entities over {} components, {} of them claiming a \
                         callback slot",
                        self.source,
                        k.entity_total,
                        k.per_component.len(),
                        k.max_cbs
                    ))
                );
                let _ = writeln!(
                    s,
                    "action_clients = {}",
                    toml_str(
                        "of those slots, the ones the arena must budget at the ACTION entry \
                         size rather than the pub/sub one — action clients AND action \
                         servers, since the arena stores ActionServerArenaEntry too (issue \
                         0900)"
                    )
                );
                let _ = writeln!(
                    s,
                    "max_nodes = {}",
                    toml_str(
                        "one node per declared component; over-counts when two share a name \
                         (slots are keyed by name), which is the safe direction"
                    )
                );
                let _ = writeln!(
                    s,
                    "max_queryables = {}",
                    toml_str(
                        "APPLICATION service servers + 3 per action server + the runtime's own \
                         servers for the families the bringup declares (6 per node for \
                         param_services, 5 for lifecycle; issue 1270). A DEFAULT and not a \
                         ceiling: a stated knob wins."
                    )
                );
                for (pkg, comp, count, slots) in &k.per_component {
                    let _ = writeln!(
                        s,
                        "component_{} = {}",
                        sanitize_key(&format!("{pkg}_{comp}")),
                        toml_str(&format!("{count} entities, {slots} callback slots"))
                    );
                }
                for (knob, why) in HAND_SET_KNOBS {
                    let _ = writeln!(s, "{} = {}", knob.to_ascii_lowercase(), toml_str(why));
                }
            }
        }
        s
    }
}

/// What one image's resolve phase wrote.
#[derive(Debug, Clone)]
pub struct Written {
    pub resolved: Resolved,
    /// `<dir>/resolved.toml` — RFC-0094 D2's artifact, always written.
    pub toml_path: std::path::PathBuf,
    /// `<dir>/resolved.cmake` — written ONLY when the phase derived something.
    ///
    /// Absent on a refusal, deliberately. A configure that finds no projection
    /// falls through to exactly the behaviour it has today, so an image this
    /// phase cannot answer for is never made WORSE by the phase existing. An
    /// empty projection would instead publish "every count is zero", which is
    /// the substituted-default failure in a new place.
    pub cmake_path: Option<std::path::PathBuf>,
}

/// Run the resolve phase for one image and write its artifacts.
///
/// [`EntityInventory`] is composed by the caller from declarations; this
/// function is the one place that turns it into files. Both transports are
/// rendered from the SAME inventory in one call — a second derivation for a
/// second file is how two artifacts of one answer come to disagree, which is
/// the defect RFC-0094 exists to remove rather than to reproduce.
pub fn write(
    dir: &std::path::Path,
    image: ImageIdent,
    inventory: &EntityInventory,
) -> std::io::Result<Written> {
    let resolved = Resolved::compose(image, inventory);
    std::fs::create_dir_all(dir)?;
    let toml_path = dir.join(RESOLVED_TOML_NAME);
    let map = |e: eyre::Report| std::io::Error::other(e.to_string());
    crate::atomic_file::atomic_write(&toml_path, &resolved.to_toml()).map_err(map)?;

    let cmake_path = match resolved.knobs() {
        None => {
            // Never leave a STALE projection beside a fresh refusal: a
            // configure reading last build's numbers for this build's
            // declarations is the museum-binary shape one directory over.
            let stale = dir.join(RESOLVED_CMAKE_NAME);
            if stale.exists() {
                std::fs::remove_file(&stale)?;
            }
            None
        }
        Some(_) => {
            let p = dir.join(RESOLVED_CMAKE_NAME);
            crate::atomic_file::atomic_write(&p, &inventory.to_cmake()).map_err(map)?;
            Some(p)
        }
    };
    Ok(Written {
        resolved,
        toml_path,
        cmake_path,
    })
}

/// A TOML basic string. Hand-rolled rather than `toml::to_string` because the
/// document above is a TEMPLATE with comments in load-bearing positions, and a
/// serializer would drop every one of them.
fn toml_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A bare TOML key. `pkg::component` carries characters a bare key may not.
fn sanitize_key(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity_inventory::{ComponentEntities, Declaration, EntityDecl};

    fn ident() -> ImageIdent {
        ImageIdent {
            bringup: "demo_bringup".into(),
            entry: "talker".into(),
            board: "mps2-an385-freertos".into(),
            platform: "freertos".into(),
            rmw: "zenoh".into(),
        }
    }

    fn inventory(entities: &[&str]) -> EntityInventory {
        let mut inv = EntityInventory::new("test://model");
        inv.insert(ComponentEntities {
            pkg: "talker_pkg".into(),
            component: "talker".into(),
            class: "TalkerNode".into(),
            declaration: Declaration::Stated(
                entities
                    .iter()
                    .flat_map(|s| EntityDecl::parse(s).expect("parses"))
                    .collect(),
            ),
        });
        inv
    }

    #[test]
    fn derived_image_publishes_demand_and_names_its_source() {
        let r = Resolved::compose(ident(), &inventory(&["pub:std_msgs/msg/String", "timer"]));
        assert_eq!(r.status(), "derived");
        let toml = r.to_toml();
        // A timer claims a callback slot, a publisher does not.
        assert!(toml.contains("max_cbs = 1"), "{toml}");
        assert!(toml.contains("entity_total = 2"), "{toml}");
        assert!(toml.contains("max_publishers = 1"), "{toml}");
        // DEMAND, unfloored: this image declares no service server.
        assert!(toml.contains("max_queryables = 0"), "{toml}");
        assert!(toml.contains("[provenance]"), "{toml}");
        assert!(toml.contains("test://model"), "{toml}");
    }

    #[test]
    fn a_refusal_publishes_prose_and_no_number() {
        let mut inv = EntityInventory::new("test://model");
        inv.insert(ComponentEntities {
            pkg: "talker_pkg".into(),
            component: "talker".into(),
            class: "TalkerNode".into(),
            declaration: Declaration::Absent,
        });
        let r = Resolved::compose(ident(), &inv);
        assert_eq!(r.status(), "refused");
        let toml = r.to_toml();
        assert!(!toml.contains("[executor]"), "{toml}");
        assert!(!toml.contains("[pools]"), "{toml}");
        assert!(toml.contains("refused = "), "{toml}");
        assert!(r.value_rows().is_empty());
    }

    #[test]
    fn hand_set_knobs_say_so_in_both_outcomes() {
        let derived = Resolved::compose(ident(), &inventory(&["timer"])).to_toml();
        let refused = Resolved::unresolvable(ident(), "test://none", "no launch".into()).to_toml();
        for (knob, _) in HAND_SET_KNOBS {
            let key = knob.to_ascii_lowercase();
            assert!(
                derived.contains(&key),
                "{knob} missing from a derived answer"
            );
            assert!(refused.contains(&key), "{knob} missing from a refusal");
            assert!(derived.contains("HAND-SET"), "the reason must say hand-set");
        }
    }

    #[test]
    fn the_digest_moves_with_a_value_and_not_with_prose() {
        let a = Resolved::compose(ident(), &inventory(&["timer"]));
        let b = Resolved::compose(ident(), &inventory(&["timer"]));
        assert_eq!(a.digest(), b.digest(), "same declarations, same digest");

        let c = Resolved::compose(ident(), &inventory(&["timer", "timer"]));
        assert_ne!(a.digest(), c.digest(), "a moved knob must move the digest");

        // The identity is part of it: two boards at the same counts are two
        // images, and D4 keys a cargo directory on this.
        let mut other = ident();
        other.board = "mps2-an385".into();
        let d = Resolved::compose(other, &inventory(&["timer"]));
        assert_ne!(a.digest(), d.digest(), "a moved board must move the digest");

        assert_eq!(a.digest().len(), 16);
    }

    #[test]
    fn the_directory_name_cannot_escape_the_build_tree() {
        let ident = ImageIdent {
            bringup: "../../etc".into(),
            entry: "a/b".into(),
            board: String::new(),
            platform: "posix".into(),
            rmw: String::new(),
        };
        let d = ident.dir_name();
        assert!(!d.contains('/'), "{d}");
        assert!(!d.contains(".."), "{d}");
    }

    /// The acceptance in miniature: the two transports of ONE resolve carry
    /// the same numbers.
    ///
    /// This is the property RFC-0094 D2 turns on — `resolved.toml` is "the
    /// single answer" only if the projection a build actually reads says the
    /// same thing. Written as an identity over `value_rows()` rather than a
    /// hand-listed pairing, so a knob added to one transport and not the other
    /// reds this instead of drifting quietly (the `NROS_DERIVED_NROS_MAX_*`
    /// class, which cmake resolves to EMPTY rather than to an error).
    #[test]
    fn resolved_toml_and_cmake_agree() {
        let inv = inventory(&[
            "sub:std_msgs/msg/String",
            "pub:std_msgs/msg/String",
            "timer",
            "service_server:example_interfaces/srv/AddTwoInts",
            "action_server:example_interfaces/action/Fibonacci",
        ]);
        let r = Resolved::compose(ident(), &inv);
        let cmake = inv.to_cmake();
        let pairs = [
            ("max_cbs", "NROS_DERIVED_EXECUTOR_MAX_CBS"),
            ("action_clients", "NROS_DERIVED_EXECUTOR_ACTION_CLIENTS"),
            ("max_nodes", "NROS_DERIVED_EXECUTOR_MAX_NODES"),
            ("max_subscribers", "NROS_DERIVED_MAX_SUBSCRIBERS"),
            ("rmw_subscriber_slots", "NROS_DERIVED_RMW_SUBSCRIBER_SLOTS"),
            ("max_publishers", "NROS_DERIVED_MAX_PUBLISHERS"),
            ("max_queryables", "NROS_DERIVED_MAX_QUERYABLES"),
            ("max_liveliness", "NROS_DERIVED_MAX_LIVELINESS"),
            (
                "max_cell_entities",
                "NROS_DERIVED_RUNTIME_MAX_CELL_ENTITIES",
            ),
        ];
        let rows: BTreeMap<&str, usize> = r.value_rows().into_iter().collect();
        for (toml_key, cmake_var) in pairs {
            let v = rows[toml_key];
            assert!(
                cmake.contains(&format!("set({cmake_var} {v})\n")),
                "resolved.toml says {toml_key} = {v}; the CMake projection does not:\n{cmake}"
            );
        }
        // And every value the TOML publishes is covered by that pairing, so a
        // new knob cannot be added to `value_rows` and silently skip the check.
        let covered: Vec<&str> = pairs.iter().map(|(t, _)| *t).collect();
        for (name, _) in r.value_rows() {
            assert!(
                covered.contains(&name) || name == "entity_total",
                "`{name}` is published with no CMake counterpart under test"
            );
        }
    }

    /// The property the resolve seed's whole value rests on: stage 3.5's
    /// projection and the mid-configure producer's write the SAME NUMBERS and
    /// the SAME BYTES when they see the same wiring.
    ///
    /// `nros_reconfigure_snapshot` hashes CONTENT, so the CMake seed
    /// (`nros_resolved_seed_entity_inventory`) removes a configure pass only
    /// when the producer that follows it writes what the seed already wrote —
    /// case B of `tests/cmake-resolved-seed-tests.sh`, which measures exactly
    /// that on a five-line project.
    ///
    /// **This test used to assert the opposite half, and phase-439 W2 was right
    /// to: they DID differ.** Measured on `demo_bringup:zephyr`
    /// (native_sim/native/64), the difference was composer-dependent PROVENANCE
    /// in a hashed file, and it was TWO renderings rather than the one issue
    /// 1228 named — `NROS_ENTITY_INVENTORY_SOURCE`, and the per-component
    /// line's PACKAGE (`/talker::talker` from the model, `talker_pkg::talker`
    /// from the merge). Both are gone from `to_cmake`; the provenance lives in
    /// `entity_inventory.json` and `resolved.toml`'s `[provenance]`, where a
    /// byte comparison cannot reach it.
    ///
    /// The two composers are set up here the way the real tree produces them —
    /// a model row's `pkg` is the node FQN, because `EntityInventory::from_model`
    /// states the node rather than inventing an ament package — so this test
    /// reproduces the second half. Built with equal `pkg` on both sides it
    /// would have stayed green through the very defect it now holds.
    ///
    /// Both halves stay asserted and they fail in different directions:
    /// diverging NUMBERS would make the seed able to under-size an image (the
    /// safety property, which case C of the shell test holds from the other
    /// side), while diverging BYTES silently costs the configure pass back with
    /// nothing going red.
    #[test]
    fn stage_3_5_and_the_mid_configure_producer_agree_byte_for_byte() {
        let entities = [
            "sub:std_msgs/msg/String",
            "pub:std_msgs/msg/String",
            "timer",
        ];
        // What stage 3.5 composes: the model alone, whose rows are keyed by the
        // node FQN — `from_model`'s own choice, and the second half of the
        // byte difference this test exists to hold.
        let mut model_only = EntityInventory::new("model.yaml");
        model_only.insert(ComponentEntities {
            pkg: "/talker".into(),
            component: "talker".into(),
            class: String::new(),
            declaration: Declaration::Stated(
                entities
                    .iter()
                    .flat_map(|s| EntityDecl::parse(s).expect("parses"))
                    .collect(),
            ),
        });

        // What the configure composes: the metadata component set (whose
        // `entities` key is ABSENT since phase-412 retired `ENTITIES`) merged
        // with the model. `merged_per_kind_max` is the production function.
        let mut from_metadata = EntityInventory::new("nros-metadata.json");
        from_metadata.insert(ComponentEntities {
            pkg: "talker_pkg".into(),
            component: "talker".into(),
            class: "TalkerNode".into(),
            declaration: Declaration::Absent,
        });
        let merged = from_metadata.merged_per_kind_max(&model_only);

        let a = Resolved::compose(ident(), &model_only);
        let b = Resolved::compose(ident(), &merged);
        assert_eq!(
            a.value_rows(),
            b.value_rows(),
            "the two composers must derive the same numbers; if they do not, the seed \
             can under-size an image and `nros_resolved_seed_entity_inventory` is unsafe"
        );

        // And the BYTES, which is what `nros_reconfigure_snapshot` compares. A
        // whole-string equality rather than a diff of interesting lines: the
        // class is "content that depends on which composer ran" — a path, an
        // ordering, a package spelling, a banner — not one named field.
        let ca = model_only.to_cmake();
        let cb = merged.to_cmake();
        assert_eq!(
            ca, cb,
            "the seed and the mid-configure producer must render the same bytes for the \
             same wiring, or `nros_resolved_seed_entity_inventory` arms exactly the \
             re-configure it exists to remove (issue 1228). Composer-dependent content \
             belongs in `entity_inventory.json` / `resolved.toml`'s [provenance], not in \
             this fragment."
        );
        // And it must be a REAL fragment, not two empty strings agreeing.
        assert!(ca.contains("set(NROS_DERIVED_EXECUTOR_MAX_CBS 2)"), "{ca}");
        assert!(
            !ca.contains("NROS_ENTITY_INVENTORY_SOURCE ") && !ca.contains("::talker ="),
            "composer-dependent provenance is back in the hashed fragment:\n{ca}"
        );
    }

    #[test]
    fn every_value_row_reaches_the_toml() {
        let r = Resolved::compose(ident(), &inventory(&["sub:std_msgs/msg/String", "timer"]));
        let toml = r.to_toml();
        for (name, v) in r.value_rows() {
            assert!(
                toml.contains(&format!("{name} = {v}")),
                "`{name} = {v}` is a resolved value the artifact does not carry:\n{toml}"
            );
        }
    }
}
