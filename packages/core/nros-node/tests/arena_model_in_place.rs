//! Issue 1340 — an `in_place` subscription row reaches the executor ARENA, not
//! just the registration.
//!
//! The runtime half is `executor::tests::a_generic_subscription_on_an_in_place_
//! backend_claims_no_receive_region`: on a backend that dispatches in place, a
//! registration claims an entry struct and no receive region. This is the BUILD
//! half — `nros-node/build.rs` pricing the arena from the sizing descriptor —
//! which no test reached: a build script has no harness, so the only evidence
//! was an image built and measured by hand.
//!
//! That measurement (issue 1340, 2026-10-01): a five-subscription copy of
//! `examples/native/rust/listener` on zenoh, every row `KEEP_LAST(1)` and
//! `registration_path = "in_place"`. Its arena model said `REQUIRED` 7,168
//! (`ARENA_SIZE` 8,192, the floor), against 22,528 for the same image with the
//! rows priced as buffered; `just mem-report --baseline` read
//! `EXECUTOR_BACKING` 32,400 -> 18,064 bytes, and all five subscriptions
//! registered and delivered on the smaller arena. The test below drives the
//! SAME functions that build ran (`build/sub_arena.rs`, included here by
//! `#[path]` exactly as `build.rs` includes it) over a descriptor in the shape
//! `nros sync` wrote for that image, so the saving cannot quietly turn back
//! into headroom.
//!
//! Every entry-struct and buffer size is read from the constants THIS build of
//! `nros-node` emitted (`config::arena_model`), never restated.

// `build.rs` reads every item of the shared file; this test reads most of them.
#[allow(dead_code)]
#[path = "../build/sub_arena.rs"]
mod sub_arena;

use nros_node::config::{DEFAULT_RX_BUF_SIZE, arena_model};
use sub_arena::{
    SubEndpoint, buffered_region, descriptor_subscriptions, row_slot_bytes,
    subs_arena_from_descriptor,
};

/// `[target] pointer_bytes` of the measured image (x86_64). Only a ring deeper
/// than 1 reads it.
const RING_LEN_BYTES: usize = 8;

/// One `[[endpoint]]` subscription row, in the shape `nros sync` writes.
///
/// `path`: `Some(p)` states `registration_path = p`; `None` REFUSES it, which is
/// what the descriptor says for a registration the metadata probe did not
/// observe (issue 1522).
fn sub_row(topic: &str, depth: Option<u32>, path: Option<&str>) -> String {
    let mut row = format!(
        "[[endpoint]]\nkind = \"subscription\"\ntype = \"std_msgs/msg/String\"\n\
         topic = \"{topic}\"\nhistory = \"keep_last\"\n"
    );
    if let Some(d) = depth {
        row += &format!("depth = {d}\n");
    }
    if let Some(p) = path {
        row += &format!("registration_path = \"{p}\"\n");
    }
    row += "\n[endpoint.refused]\n";
    if depth.is_none() {
        row += "depth = \"synthetic: this row states no depth\"\n";
    }
    if path.is_none() {
        row += "registration_path = \"synthetic: no observed registration (issue 1522)\"\n";
    }
    row += "wire_bound_bytes = \"`std_msgs/msg/String` has no static bound\"\n\n";
    row
}

fn descriptor(rows: &[String]) -> nros_sizing_descriptor::SizingDescriptor {
    let text = format!(
        "schema_version = {}\n\n[meta]\nentry = \"native\"\nstatus = \"partial\"\n\
         basis = \"contract\"\nundeclared_endpoints = 0\n\n\
         [target]\npointer_bytes = 8\nmax_align = 8\n\n{}\
         [image]\nsubscription_entities = {}\n",
        nros_sizing_descriptor::SCHEMA_VERSION,
        rows.concat(),
        rows.len(),
    );
    nros_sizing_descriptor::parse(&text, std::path::Path::new("synthetic.toml"))
        .unwrap_or_else(|e| panic!("the synthetic descriptor must parse: {e}\n{text}"))
}

/// The rows the build derives, for a build whose backends do (`true`) or do not
/// (`false`) claim in-place dispatch — the issue-1577 guard.
fn rows(
    desc: &nros_sizing_descriptor::SizingDescriptor,
    in_place_trusted: bool,
) -> Vec<SubEndpoint> {
    descriptor_subscriptions(
        desc,
        DEFAULT_RX_BUF_SIZE,
        DEFAULT_RX_BUF_SIZE,
        in_place_trusted,
    )
    .expect("a contract-basis descriptor with no undeclared endpoint is attributable")
}

fn price(rows: &[SubEndpoint]) -> Option<usize> {
    subs_arena_from_descriptor(
        rows,
        rows.len(),
        arena_model::PUBSUB_STRUCT,
        RING_LEN_BYTES,
        DEFAULT_RX_BUF_SIZE,
    )
}

fn five_in_place() -> nros_sizing_descriptor::SizingDescriptor {
    let rows: Vec<String> = [
        "/chatter",
        "/chatter2",
        "/chatter3",
        "/chatter4",
        "/chatter5",
    ]
    .iter()
    .map(|t| sub_row(t, Some(1), Some("in_place")))
    .collect();
    descriptor(&rows)
}

#[test]
fn in_place_rows_are_priced_at_the_entry_struct_alone() {
    let desc = five_in_place();
    let rows = rows(&desc, true);
    assert!(
        rows.iter().all(|r| r.claims_no_region),
        "a stated `in_place` row on a build whose backends claim in-place dispatch \
         must claim no receive region"
    );
    let priced = price(&rows).expect("every row states a depth");
    assert_eq!(
        priced,
        rows.len() * arena_model::PUBSUB_STRUCT,
        "five in-place subscriptions cost five entry structs and nothing else"
    );
}

#[test]
fn the_same_rows_priced_as_buffered_cost_one_receive_region_each_more() {
    let desc = five_in_place();
    let in_place = price(&rows(&desc, true)).unwrap();

    // The issue-1577 guard: a build whose backends do not claim in-place
    // dispatch (Cyclone, or a bridge linking it) prices the same rows buffered.
    // That is also the arena this image had before issue 1340's saving.
    let buffered_rows = rows(&desc, false);
    assert!(buffered_rows.iter().all(|r| !r.claims_no_region));
    let buffered = price(&buffered_rows).unwrap();

    let regions: usize = buffered_rows
        .iter()
        .map(|r| {
            buffered_region(
                r.depth.unwrap() as usize,
                row_slot_bytes(r, DEFAULT_RX_BUF_SIZE),
                RING_LEN_BYTES,
            )
        })
        .sum();
    assert!(regions > 0, "a buffered row reserves a region");
    assert_eq!(
        buffered - in_place,
        regions,
        "the in-place saving must be exactly the receive regions, no more: \
         in-place {in_place}, buffered {buffered}"
    );
    // The measured image's own figures, at this build's defaults: 5 x 1,024
    // against 5 x (1,024 + 3 x 1,024). Asserted as a relation above so a moved
    // default does not break it; recorded here so the two can be compared.
    eprintln!(
        "five KEEP_LAST(1) rows: in-place {in_place} B, buffered {buffered} B, \
         + BASE_OVERHEAD {} B each",
        arena_model::BASE_OVERHEAD
    );
}

#[test]
fn a_row_nobody_observed_in_place_keeps_its_region_beside_ones_that_were() {
    // The direction that cannot ship `BufferTooSmall`: an unobserved registration
    // path is priced at the closure buffer even on an in-place build, and one
    // such row does not drag the in-place rows back with it.
    let desc = descriptor(&[
        sub_row("/seen", Some(1), Some("in_place")),
        sub_row("/unseen", Some(1), None),
    ]);
    let rows = rows(&desc, true);
    assert!(rows[0].claims_no_region);
    assert!(!rows[1].claims_no_region);
    assert_eq!(
        price(&rows).unwrap(),
        2 * arena_model::PUBSUB_STRUCT + buffered_region(1, DEFAULT_RX_BUF_SIZE, RING_LEN_BYTES),
    );
}

#[test]
fn an_in_place_row_with_no_depth_refuses_the_whole_sum() {
    // A depth-less row describes an endpoint the table cannot price; letting an
    // in-place row through without one would make the guard depend on which
    // path the row happens to state.
    let desc = descriptor(&[
        sub_row("/a", Some(1), Some("in_place")),
        sub_row("/b", None, Some("in_place")),
    ]);
    assert_eq!(price(&rows(&desc, true)), None);
}
