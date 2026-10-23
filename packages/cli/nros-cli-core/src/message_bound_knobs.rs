//! phase-439 W6 (issue 1252) — the Rust twin of
//! `nros_derive_message_bound_knobs` in `cmake/NanoRosMessageBounds.cmake`.
//!
//! # Why a twin exists
//!
//! On the Zephyr west road `nros/message_bound_knobs.cmake` is READ by the knob
//! resolver inside `find_package(Zephyr)` and WRITTEN by `nros_find_interfaces()`
//! later in the same configure. On a clean build dir the reader finds a
//! placeholder, the producer writes the answer, and issue 0991's arm buys a
//! whole configure pass so the reader sees it. That pass is the last one
//! RFC-0094 A3 has to remove.
//!
//! `nros build` can compose the same answer BEFORE the configure — every input
//! is a declaration or a table `nros sync` already produced — and hand it to
//! the configure as a seed, exactly as stage 3.5 seeds the entity fragment
//! (issue 1228). The seed removes the pass only if it is BYTE-IDENTICAL to what
//! the producer then writes, because the arm compares content. So [`compose`]
//! renders `_nros_message_bounds_write_output`'s text exactly, and
//! `tests/cmake-message-bounds-tests.sh` compares the two on every case it
//! runs.
//!
//! # What it refuses to render
//!
//! [`compose`] returns `None` for a branch whose text names a BUILD-DIR PATH (a
//! fragment still pending, a malformed entity fragment): the seed cannot know
//! that path, so a seed there would differ by construction. `None` means "no
//! seed", which is today's behaviour — the producer answers and arms, as before.
//! A seed is never more than a guess the producer checks: if the inputs differ
//! the producer's bytes differ, and the arm fires exactly as it did without one.
//!
//! # Inputs
//!
//! * the bound tables of the image's interface closure (`nros_message_bounds.json`,
//!   one per package — the same rows `BoundInventory::to_cmake` projects into
//!   the fragments the cmake composer includes);
//! * the entity fragment's TEXT — what the cmake composer `include()`s, read
//!   through [`cmake_set_values`] so the join sees what cmake sees rather than a
//!   second reading of the inventory;
//! * the small/large class split.
//!
//! Types and packages are composed in SORTED order on both sides (the cmake
//! composer sorts too), so the answer is a function of the closure as a SET and
//! not of the order in which the configure happened to register its fragments.

use std::collections::BTreeMap;

use rosidl_codegen::bounds::{BoundState, TypeBoundEntry};

/// The file `nros build` writes into an image's resolve dir, and
/// `nros_resolved_seed_message_bounds()` copies into the build dir. Spelled
/// once here and once in `cmake/NanoRosResolved.cmake`.
pub const RESOLVED_MESSAGE_BOUNDS_NAME: &str = "resolved-message-bounds.cmake";

/// One interface package's bound table.
#[derive(Debug, Clone)]
pub struct BoundTable {
    pub package: String,
    pub rows: Vec<TypeBoundEntry>,
}

/// `string(REGEX REPLACE "[^A-Za-z0-9]" "_" ...)` — the key the cmake side
/// looks every per-type variable up by.
fn cmake_key(type_name: &str) -> String {
    type_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

/// The `set(NAME value)` / `set(NAME "value")` lines of a cmake fragment, last
/// write winning, the way `include()` leaves them. Only the single-line shape
/// the Rust emitters write is understood; anything else is ignored, which can
/// only make a later comparison differ (a missed seed), never a wrong number.
pub fn cmake_set_values(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("set(") else {
            continue;
        };
        let Some(rest) = rest.strip_suffix(')') else {
            continue;
        };
        let (name, value) = match rest.split_once(' ') {
            Some((n, v)) => (n, v.trim()),
            None => (rest, ""),
        };
        let value = if let Some(q) = value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
            unescape(q)
        } else {
            value.to_string()
        };
        out.insert(name.to_string(), value);
    }
    out
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c == '\\' {
            match it.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(o) => out.push(o),
                None => out.push('\\'),
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// `_nros_message_bounds_write_output`'s escaping for a quoted reason.
fn escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// A cmake list value (`a;b;c`), empty for the empty string.
fn cmake_list(v: &str) -> Vec<&str> {
    if v.is_empty() {
        Vec::new()
    } else {
        v.split(';').collect()
    }
}

/// One composed type, keyed the way the cmake side keys it.
struct Priced {
    state: &'static str,
    tx: Option<usize>,
    rx: Option<usize>,
    reason: String,
}

#[derive(Default)]
struct Join {
    basis: String,
    status: String,
    why: String,
    count: u64,
    small: usize,
    large_types: Vec<String>,
    large_max: usize,
    large_count: u64,
    type_bounds: Vec<String>,
}

/// What `nros_derive_message_bound_knobs` publishes, in the names the writer
/// reads.
#[derive(Default)]
struct Published {
    basis: Option<String>,
    payload_status: String,
    payload_reason: String,
    subscription_count: u64,
    subscriber_buffer_size: Option<usize>,
    max_large_subscribers: Option<usize>,
    large_types: Vec<String>,
    subscriber_large_size: Option<usize>,
    subscribed_type_bounds: Option<String>,
    tl_retain_bytes: Option<usize>,
    subscription_buffer_size: Option<usize>,
    largest_type: String,
}

/// Compose the image-wide fragment, or `None` where the cmake composer's text
/// would name a path this caller cannot know (see the module docs).
///
/// `entity_cmake` is the entity fragment's text, or `None` when the configure
/// would find no fragment at all.
pub fn compose(
    tables: &[BoundTable],
    entity_cmake: Option<&str>,
    ceiling: usize,
) -> Option<String> {
    if tables.is_empty() {
        return Some(write_output(
            "refused",
            "no message-bound inventory was produced by this configure",
            ceiling,
            &[],
            0,
            &Published::default(),
        ));
    }

    // ---- Compose: packages and types as SETS, per-type values last-wins ----
    let mut packages: Vec<String> = tables.iter().map(|t| t.package.clone()).collect();
    packages.sort();
    packages.dedup();
    let mut priced: BTreeMap<String, Priced> = BTreeMap::new();
    let mut types: Vec<String> = Vec::new();
    for t in tables {
        for row in &t.rows {
            if !types.contains(&row.type_name) {
                types.push(row.type_name.clone());
            }
            let p = match &row.bound {
                BoundState::Bounded { tx, rx } => Priced {
                    state: "bounded",
                    tx: Some(*tx),
                    rx: Some(*rx),
                    reason: String::new(),
                },
                BoundState::Unbounded { reason } => Priced {
                    state: "unbounded",
                    tx: None,
                    rx: None,
                    reason: reason.clone(),
                },
                BoundState::Unresolved { reason } => Priced {
                    state: "unresolved",
                    tx: None,
                    rx: None,
                    reason: reason.clone(),
                },
            };
            priced.insert(cmake_key(&row.type_name), p);
        }
    }
    types.sort();
    let type_count = types.len();
    if type_count == 0 {
        return Some(write_output(
            "refused",
            "the composed inventory holds no message types",
            ceiling,
            &packages,
            0,
            &Published::default(),
        ));
    }

    // ---- Read every type ----
    let mut open: Vec<String> = Vec::new();
    let mut max_rx = 0usize;
    let mut max_type = String::new();
    let mut small = 0usize;
    let mut large_types: Vec<String> = Vec::new();
    let mut large_max = 0usize;
    for t in &types {
        let p = &priced[&cmake_key(t)];
        if p.state != "bounded" {
            let reason = if p.reason.is_empty() {
                "no reason recorded"
            } else {
                p.reason.as_str()
            };
            open.push(format!("    {t} ({}): {reason}", p.state));
            continue;
        }
        let rx = p.rx?;
        if rx > max_rx {
            max_rx = rx;
            max_type = t.clone();
        }
        if rx > ceiling {
            large_types.push(format!("{t}={rx}"));
            if rx > large_max {
                large_max = rx;
            }
        } else if rx > small {
            small = rx;
        }
    }

    let entity = entity_cmake.map(cmake_set_values);
    let join = join_subscribed(entity.as_ref(), &priced, ceiling)?;
    let tl_retain = tl_retain(entity.as_ref(), &priced);

    let mut published = Published {
        payload_status: "refused".to_string(),
        payload_reason: "the derivation did not reach the payload classes".to_string(),
        tl_retain_bytes: tl_retain,
        ..Default::default()
    };
    let publish_classes = |p: &mut Published| {
        p.basis = Some(join.basis.clone());
        let (sub_small, sub_large_types, sub_large_max, sub_large_count) =
            if join.basis == "closure" {
                (
                    small,
                    large_types.clone(),
                    large_max,
                    large_types.len() as u64,
                )
            } else {
                (
                    join.small,
                    join.large_types.clone(),
                    join.large_max,
                    join.large_count,
                )
            };
        if sub_small > 0 {
            p.subscriber_buffer_size = Some(sub_small);
        }
        p.max_large_subscribers = Some(sub_large_count as usize);
        p.large_types = sub_large_types;
        if sub_large_count > 0 {
            p.subscriber_large_size = Some(sub_large_max);
        }
        if join.basis == "subscribed" {
            p.subscribed_type_bounds = Some(join.type_bounds.join(";"));
        }
    };

    if !open.is_empty() {
        let why = format!(
            "{} of {type_count} types in the linked interface closure have no derived bound, so no class size can be trusted:\n{}",
            open.len(),
            open.join("\n")
        );
        published.payload_status = join.status.clone();
        published.payload_reason = join.why.clone();
        published.subscription_count = join.count;
        if join.status == "derived" && join.basis == "subscribed" {
            publish_classes(&mut published);
        }
        return Some(write_output(
            "refused", &why, ceiling, &packages, type_count, &published,
        ));
    }

    published.subscription_buffer_size = Some(max_rx);
    published.largest_type = max_type;
    published.payload_status = join.status.clone();
    published.payload_reason = join.why.clone();
    published.subscription_count = join.count;
    if join.status == "derived" {
        publish_classes(&mut published);
    }
    Some(write_output(
        "derived", "", ceiling, &packages, type_count, &published,
    ))
}

/// `_nros_bounds_join_subscribed`. `None` where its text would name the entity
/// fragment's path.
fn join_subscribed(
    entity: Option<&BTreeMap<String, String>>,
    priced: &BTreeMap<String, Priced>,
    ceiling: usize,
) -> Option<Join> {
    let closure = || Join {
        basis: "closure".to_string(),
        status: "derived".to_string(),
        ..Default::default()
    };
    let Some(e) = entity else {
        return Some(closure());
    };
    let supported = crate::entity_inventory::ENTITY_INVENTORY_SCHEMA_VERSION.to_string();
    if e.get("NROS_ENTITY_INVENTORY_SCHEMA_VERSION") != Some(&supported) {
        return Some(closure());
    }
    if e.get("NROS_ENTITY_INVENTORY_STATUS").map(String::as_str) != Some("derived") {
        return Some(closure());
    }
    let refused = |why: String| Join {
        status: "refused".to_string(),
        why,
        ..Default::default()
    };
    // Malformed: the cmake text names the fragment's path.
    let sub_status = e.get("NROS_ENTITY_SUBSCRIBED_TYPES_STATUS")?;
    if sub_status != "resolved" {
        let reason = e
            .get("NROS_ENTITY_SUBSCRIBED_TYPES_REASON")
            .cloned()
            .unwrap_or_default();
        return Some(refused(format!(
            "this image declares its entities, so the payload classes are derived from them or not at all -- and the subscribed-type set did not resolve:\n{reason}"
        )));
    }

    let mut j = Join {
        basis: "subscribed".to_string(),
        status: "derived".to_string(),
        ..Default::default()
    };
    let mut unpriced: Vec<String> = Vec::new();
    let mut open_subscribed: Vec<String> = Vec::new();
    let counts = e
        .get("NROS_ENTITY_SUBSCRIBED_TYPE_COUNTS")
        .cloned()
        .unwrap_or_default();
    for entry in cmake_list(&counts) {
        let (t, n) = entry.rsplit_once('=')?;
        let n: u64 = n.parse().ok()?;
        j.count += n;
        let Some(p) = priced.get(&cmake_key(t)) else {
            unpriced.push(t.to_string());
            continue;
        };
        if p.state != "bounded" {
            open_subscribed.push(format!("{t} ({})", p.state));
            continue;
        }
        let rx = p.rx?;
        j.type_bounds.push(format!("{t}={rx}"));
        if rx > ceiling {
            j.large_types.push(format!("{t}={rx}"));
            j.large_count += n;
            if rx > j.large_max {
                j.large_max = rx;
            }
        } else if rx > j.small {
            j.small = rx;
        }
    }
    if !open_subscribed.is_empty() {
        return Some(refused(format!(
            "{} type(s) this image RECEIVES carry no derived bound, so their payload class cannot be sized:\n    {}\n  Bound the member in its `.msg` (`string<=64`) or cap it `inline` in the package's `nros-codegen.toml` (RFC-0033). Narrowing to the subscribed set does not help here -- these are in it.",
            open_subscribed.len(),
            open_subscribed.join("\n    ")
        )));
    }
    if !unpriced.is_empty() {
        return Some(refused(format!(
            "{} type(s) this image receives are not in the bound inventory, so their payload class cannot be derived:\n    {}\n  Either the declaration names a type this image does not link, or it names a service/action type -- the bound inventory prices MESSAGES (`pkg/msg/Name`) and nothing else, so `pkg/srv/*` and `pkg/action/*` have no entry to join against.",
            unpriced.len(),
            unpriced.join("\n    ")
        )));
    }
    Some(j)
}

/// `_nros_bounds_tl_retain`: the bytes one transient-local retention slot must
/// hold, or `None` whenever the cmake side leaves the knob underived.
fn tl_retain(
    entity: Option<&BTreeMap<String, String>>,
    priced: &BTreeMap<String, Priced>,
) -> Option<usize> {
    let e = entity?;
    let tl: u64 = e.get("NROS_DERIVED_TL_PUBLISHERS")?.parse().ok()?;
    if tl == 0 {
        return None;
    }
    if let Some(a) = e.get("NROS_ENTITY_COUNT_ACTION_SERVER")
        && a.parse::<u64>().ok() != Some(0)
    {
        return None;
    }
    let rows_v = e
        .get("NROS_ENTITY_DECLARED_DURABILITY_PUBLISHER")
        .cloned()
        .unwrap_or_default();
    let mut max = 0usize;
    let mut rows = 0u64;
    for row in cmake_list(&rows_v) {
        let Some((t, _)) = row.split_once('|') else {
            continue;
        };
        if t.is_empty() || !row.ends_with("=transient_local") {
            continue;
        }
        rows += 1;
        let p = priced.get(&cmake_key(t))?;
        if p.state != "bounded" {
            return None;
        }
        max = max.max(p.tx?);
    }
    if rows != tl || max == 0 {
        return None;
    }
    Some(max)
}

fn write_output(
    status: &str,
    reason: &str,
    ceiling: usize,
    packages: &[String],
    type_count: usize,
    p: &Published,
) -> String {
    let mut c = String::new();
    c.push_str("# GENERATED by nros (phase-403 W8, issue 0940). Do not edit.\n");
    c.push_str("#\n");
    c.push_str("# The size knobs DERIVED from this image's message-bound inventory\n");
    c.push_str("# (`nros_message_bounds.cmake`, one per generated interface package).\n");
    c.push_str("#\n");
    c.push_str("# Every number here is a DEFAULT. A board `.conf`, a Kconfig value or an\n");
    c.push_str("# environment override states a number and WINS; this file only fills in\n");
    c.push_str("# what nobody stated.\n");
    c.push_str("#\n");
    c.push_str("# There are TWO BASES here and the file says which each number used.\n");
    c.push_str("# NROS_SUBSCRIPTION_BUFFER_SIZE is always derived over the LINKED closure:\n");
    c.push_str("# it is also DEFAULT_TX_BUF and the default buffer of every raw service,\n");
    c.push_str("# client and action entity, so a type this image only PUBLISHES still has\n");
    c.push_str("# to fit and narrowing it would size a buffer too small. The three payload\n");
    c.push_str("# classes are derived over the SUBSCRIBED set when the image declares its\n");
    c.push_str("# entities (phase-403 W9), and over the closure when it declares none.\n");
    c.push_str("#\n");
    c.push_str("# Derivation: nros_serdes::size::max_serialized_size, the same rule the\n");
    c.push_str("# runtime's M::MAX_SERIALIZED_SIZE_XCDR* uses. NOT the C++ pack's\n");
    c.push_str("# SERIALIZED_SIZE_MAX, which is an estimate (phase-403 W6).\n");
    c.push_str("#\n");
    c.push_str(&format!(
        "# small/large class split: {ceiling} B (policy -- ZPICO_SUBSCRIBER_SIZE_THRESHOLD)\n"
    ));
    c.push('\n');
    c.push_str(&format!("set(NROS_MESSAGE_BOUNDS_STATUS \"{status}\")\n"));
    let type_bounds = |c: &mut String| {
        if let Some(tb) = &p.subscribed_type_bounds {
            c.push_str(
                "# issue 1255 -- the bound of EACH subscribed type, for a consumer that\n\
                 # allocates per subscription rather than from a shared pool. The class\n\
                 # knobs above are the maximum over this table.\n",
            );
            c.push_str(&format!(
                "set(NROS_DERIVED_SUBSCRIBED_TYPE_BOUNDS \"{tb}\")\n"
            ));
        }
    };
    let tl = |c: &mut String| {
        if let Some(b) = p.tl_retain_bytes {
            c.push_str(
                "# The transient-local retention slot: the largest serialized bound over\n\
                 # the types this image publishes TRANSIENT_LOCAL (ZPICO_TL_RETAIN_BYTES).\n",
            );
            c.push_str(&format!("set(NROS_DERIVED_TL_RETAIN_BYTES {b})\n"));
        }
    };
    if status == "refused" {
        c.push_str(&format!(
            "set(NROS_MESSAGE_BOUNDS_REASON \"{}\")\n",
            escape(reason)
        ));
        if p.basis.as_deref() == Some("subscribed") {
            c.push_str(
                "# The TAKE BUFFER is refused: an unbounded type in the linked closure\n\
                 # could be published through it (DEFAULT_TX_BUF aliases it), so\n\
                 # NROS_SUBSCRIPTION_BUFFER_SIZE keeps its configured value.\n\
                 #\n\
                 # The PAYLOAD CLASSES below are derived anyway: they size the staging\n\
                 # pools for what this image RECEIVES, every subscribed type IS bounded,\n\
                 # and an unbounded type nothing subscribes to cannot reach them.\n",
            );
            c.push_str("set(NROS_MESSAGE_BOUNDS_BASIS \"subscribed\")\n");
            c.push_str(&format!(
                "set(NROS_MESSAGE_BOUNDS_PAYLOAD_STATUS \"{}\")\n",
                p.payload_status
            ));
            if let Some(v) = p.subscriber_buffer_size {
                c.push_str(&format!("set(NROS_DERIVED_SUBSCRIBER_BUFFER_SIZE {v})\n"));
            }
            if let Some(v) = p.max_large_subscribers {
                c.push_str(&format!("set(NROS_DERIVED_MAX_LARGE_SUBSCRIBERS {v})\n"));
            }
            if let Some(v) = p.subscriber_large_size {
                c.push_str(&format!("set(NROS_DERIVED_SUBSCRIBER_LARGE_SIZE {v})\n"));
            }
            type_bounds(&mut c);
            tl(&mut c);
        } else {
            c.push_str("# No knob is derived. Every one keeps its configured value.\n");
        }
        return c;
    }
    c.push_str(&format!(
        "set(NROS_MESSAGE_BOUNDS_PACKAGES \"{}\")\n",
        packages.join(";")
    ));
    c.push_str(&format!(
        "set(NROS_MESSAGE_BOUNDS_TYPE_COUNT {type_count})\n"
    ));
    c.push_str(&format!(
        "# {} is the largest type in the closure.\n",
        p.largest_type
    ));
    c.push_str(&format!(
        "set(NROS_DERIVED_SUBSCRIPTION_BUFFER_SIZE {})\n",
        p.subscription_buffer_size.unwrap_or(0)
    ));
    c.push('\n');
    c.push_str("# ---- the three PAYLOAD-CLASS knobs, and the set they were derived over ----\n");
    c.push_str(&format!(
        "set(NROS_MESSAGE_BOUNDS_PAYLOAD_STATUS \"{}\")\n",
        p.payload_status
    ));
    let derived = p.payload_status == "derived";
    if !derived {
        c.push_str(&format!(
            "set(NROS_MESSAGE_BOUNDS_PAYLOAD_REASON \"{}\")\n",
            escape(&p.payload_reason)
        ));
        c.push_str(
            "# NROS_SUBSCRIBER_BUFFER_SIZE, NROS_SUBSCRIBER_LARGE_SIZE and\n\
             # NROS_MAX_LARGE_SUBSCRIBERS are NOT derived and do NOT fall back to the\n\
             # closure. This image states what it receives, so the classes come from\n\
             # that or from nowhere; each keeps its configured value.\n",
        );
    } else {
        let basis = p.basis.as_deref().unwrap_or("");
        c.push_str(&format!("set(NROS_MESSAGE_BOUNDS_BASIS \"{basis}\")\n"));
        if basis == "subscribed" {
            c.push_str(&format!(
                "# Derived over the {} SUBSCRIPTIONS this image declares\n\
                 # (a launch contract sidecar, or `system.toml` `[[component]] entities`),\n\
                 # not over the\n\
                 # {type_count}-type linked closure. A type the image links and never\n\
                 # receives cannot set a class here.\n",
                p.subscription_count
            ));
        } else {
            c.push_str(&format!(
                "# UPPER BOUND: this image declares no entities, so the classes are derived\n\
                 # over the whole {type_count}-type linked closure and are sized for the largest\n\
                 # type it COULD receive, not the largest it does. Name each subscription's\n\
                 # type -- in `<bringup>/launch/<stem>.contract.yaml`, or in `system.toml`\n\
                 # `[[component]] entities` for a standalone leaf -- to narrow them.\n"
            ));
        }
        if let Some(v) = p.subscriber_buffer_size {
            c.push_str("# The largest received type at or under the class split.\n");
            c.push_str(&format!("set(NROS_DERIVED_SUBSCRIBER_BUFFER_SIZE {v})\n"));
        } else {
            c.push_str(&format!(
                "# Nothing this image receives fits under the {ceiling} B split, so the small\n\
                 # class size is not derivable from that set -- but the class is still used,\n\
                 # by any caller that states no hint. NROS_SUBSCRIBER_BUFFER_SIZE keeps its\n\
                 # configured value.\n"
            ));
        }
        c.push_str(&format!(
            "# Received types over the split: {}\n",
            p.large_types.join(";")
        ));
        c.push_str(&format!(
            "set(NROS_DERIVED_MAX_LARGE_SUBSCRIBERS {})\n",
            p.max_large_subscribers.unwrap_or(0)
        ));
        if let Some(v) = p.subscriber_large_size {
            c.push_str(&format!("set(NROS_DERIVED_SUBSCRIBER_LARGE_SIZE {v})\n"));
        } else {
            c.push_str(
                "# Zero large-class blocks, so the pool is zero bytes whatever size it\n\
                 # would name -- NROS_SUBSCRIBER_LARGE_SIZE is deliberately not derived.\n",
            );
        }
        type_bounds(&mut c);
    }
    tl(&mut c);
    c
}

/// Does this `package.xml` declare a dependency at all?
///
/// `_nros_generate_declared_interfaces` (cmake/NanoRosVerbs.cmake) runs
/// `nros_find_interfaces()` only for such a package, so a node that declares
/// none contributes nothing to the closure — on either side.
fn declares_a_dependency(package_xml: &std::path::Path) -> bool {
    std::fs::read_to_string(package_xml).is_ok_and(|body| {
        [
            "<depend>",
            "<build_depend>",
            "<exec_depend>",
            "<run_depend>",
            "<build_export_depend>",
        ]
        .iter()
        .any(|t| body.contains(t))
    })
}

/// Write the west road's message-bound seed into `resolved_dir`, or remove a
/// stale one. Returns the path written, or the reason there is no seed.
///
/// The inputs, and why each is the configure's own:
///
/// * the CLOSURE — the union, over the node packages the generated application
///   `add_subdirectory`s (`node_dirs`), of what `resolve-deps` resolves for
///   each `package.xml`. That is the call `nros_find_interfaces()` makes, and
///   the composer runs over every package one configure registered;
/// * the BOUND TABLES — the workspace metadata probe's (`probe_codegen_dir`,
///   `<ws>/build/nros-metadata/metadata-probe-cmake/build/nros-codegen/nano_ros_<lang>`),
///   which `nros sync` generated with the same codegen over the same node
///   packages and the same `CODEGEN_CONFIG`. A bound is a property of the
///   declaration and its caps, not of the target;
/// * the ENTITY fragment — `resolved.cmake` beside it, which the configure
///   copies into `nros/entity_inventory.cmake` before the composer reads it
///   (`nros_resolved_seed_entity_inventory`).
///
/// Every missing input is NO SEED, which is today's behaviour: the configure
/// derives, and arms its one pass. So is a closure the probe did not generate.
/// A seed that turns out different from the configure's answer costs the same
/// pass and nothing more — the producer always runs and always wins.
pub fn write_west_seed(
    resolved_dir: &std::path::Path,
    probe_codegen_dir: &std::path::Path,
    node_dirs: &[std::path::PathBuf],
) -> Result<std::path::PathBuf, String> {
    let target = resolved_dir.join(RESOLVED_MESSAGE_BOUNDS_NAME);
    let outcome = (|| {
        let entity_path = resolved_dir.join(crate::resolve::RESOLVED_CMAKE_NAME);
        let entity = std::fs::read_to_string(&entity_path).map_err(|_| {
            "the resolve derived no entity inventory, so the configure's first pass reads a \
             placeholder and composes over a different join"
                .to_string()
        })?;
        let mut closure: Vec<String> = Vec::new();
        for dir in node_dirs {
            let xml = dir.join("package.xml");
            if !declares_a_dependency(&xml) {
                continue;
            }
            let names = cargo_nano_ros::resolve_interface_package_names(&xml)
                .map_err(|e| format!("resolving {}: {e}", xml.display()))?;
            closure.extend(names);
        }
        closure.sort();
        closure.dedup();
        if closure.is_empty() {
            return Err(
                "no node package resolves an interface package, so the configure \
                        composes nothing and keeps its placeholder"
                    .to_string(),
            );
        }
        let mut tables = Vec::with_capacity(closure.len());
        for pkg in &closure {
            let json = probe_codegen_dir
                .join(pkg)
                .join(rosidl_codegen::INVENTORY_JSON_NAME);
            let doc = std::fs::read_to_string(&json).map_err(|_| {
                format!(
                    "the metadata probe generated no bound table for `{pkg}` ({}) -- run \
                     `nros sync`",
                    json.display()
                )
            })?;
            let rows = rosidl_codegen::bounds::bounds_from_json(&doc)
                .map_err(|e| format!("{}: {e}", json.display()))?;
            tables.push(BoundTable {
                package: pkg.clone(),
                rows,
            });
        }
        compose(
            &tables,
            Some(&entity),
            rosidl_codegen::bounds::DEFAULT_SMALL_CLASS_CEILING,
        )
        .ok_or_else(|| {
            "the composed text would name a build-dir path, which a seed cannot know".to_string()
        })
    })();
    match outcome {
        Ok(text) => {
            if std::fs::read_to_string(&target).ok().as_deref() != Some(text.as_str()) {
                crate::atomic_file::atomic_write(&target, &text)
                    .map_err(|e| format!("writing {}: {e}", target.display()))?;
            }
            Ok(target)
        }
        Err(why) => {
            // A seed from an earlier build must not outlive the inputs it was
            // composed from.
            if target.exists() {
                let _ = std::fs::remove_file(&target);
            }
            Err(why)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounded(t: &str, tx: usize, rx: usize) -> TypeBoundEntry {
        let mut inv = rosidl_codegen::BoundInventory::new("p");
        inv.insert(t, BoundState::Bounded { tx, rx });
        inv.entries().into_iter().next().unwrap().clone()
    }

    fn unbounded(t: &str, why: &str) -> TypeBoundEntry {
        let mut inv = rosidl_codegen::BoundInventory::new("p");
        inv.insert(
            t,
            BoundState::Unbounded {
                reason: why.to_string(),
            },
        );
        inv.entries().into_iter().next().unwrap().clone()
    }

    #[test]
    fn set_values_read_quoted_bare_and_escaped() {
        let v = cmake_set_values(
            "# c\nset(A 6)\nset(B \"x;y\")\nset(C \"a\\nb \\\"q\\\"\")\nset(A 7)\n",
        );
        assert_eq!(v["A"], "7");
        assert_eq!(v["B"], "x;y");
        assert_eq!(v["C"], "a\nb \"q\"");
    }

    #[test]
    fn an_unbounded_closure_still_derives_the_subscribed_classes() {
        let tables = [BoundTable {
            package: "std_msgs".into(),
            rows: vec![
                bounded("std_msgs/msg/Int32", 8, 12),
                unbounded("std_msgs/msg/String", "unbounded members: data (string)"),
            ],
        }];
        let entity = "set(NROS_ENTITY_INVENTORY_SCHEMA_VERSION 6)\n\
                      set(NROS_ENTITY_INVENTORY_STATUS \"derived\")\n\
                      set(NROS_ENTITY_SUBSCRIBED_TYPES_STATUS \"resolved\")\n\
                      set(NROS_ENTITY_SUBSCRIBED_TYPE_COUNTS \"std_msgs/msg/Int32=1\")\n";
        let out = compose(&tables, Some(entity), 2048).unwrap();
        assert!(
            out.contains("set(NROS_MESSAGE_BOUNDS_STATUS \"refused\")\n"),
            "{out}"
        );
        assert!(
            out.contains("1 of 2 types in the linked interface closure"),
            "{out}"
        );
        assert!(
            out.contains("set(NROS_DERIVED_SUBSCRIBER_BUFFER_SIZE 12)\n"),
            "{out}"
        );
        assert!(
            out.contains("set(NROS_DERIVED_SUBSCRIBED_TYPE_BOUNDS \"std_msgs/msg/Int32=12\")\n"),
            "{out}"
        );
    }

    #[test]
    fn a_malformed_entity_fragment_is_no_seed() {
        let tables = [BoundTable {
            package: "p".into(),
            rows: vec![bounded("p/msg/A", 4, 8)],
        }];
        let entity = "set(NROS_ENTITY_INVENTORY_SCHEMA_VERSION 6)\n\
                      set(NROS_ENTITY_INVENTORY_STATUS \"derived\")\n";
        assert!(compose(&tables, Some(entity), 2048).is_none());
    }
}
