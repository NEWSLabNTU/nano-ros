//! phase-463 W7 — the PROFILING half of the host census (RFC-0078 amendment
//! 2026-10-07, issue 0403's `nros.wcet.measurements/1`).
//!
//! The census (W1-W5) answers "what does the code create". This answers "what
//! does it cost, ON THE HOST", and — the part that transfers to the RTOS image —
//! "which outputs does each tick actually publish".
//!
//! # What a host number is, and is not
//!
//! A WCET belongs to a context, not to code (RFC-0078 D1). A host x86-64 shares
//! no cycle count with a Cortex-M7, so every number written here is a
//! HIGH-WATER MARK in NANOSECONDS of the host's monotonic clock, under a
//! NAMED host profile (`host-<arch>-<profile>`, `clock_hz = 0`). It is never
//! a bound (RFC-0078 D1b): the file carries no `bound_cycles`, and the
//! `[wcet.select]` of a non-native image that names a host profile is refused
//! by `nros build`.
//!
//! What DOES transfer is structure, and that is the other half of each row:
//! the set of topics a callback published per invocation (the contract's
//! `paths.<p>.output`, which the layer map has trusted unchecked until now).
//!
//! # Shape
//!
//! * **Registration** (`on_register`, from `Executor::emplace_entry`) records
//!   every executor slot with its kind and the executor's own name for it
//!   (topic / `timer@<us>` / `<label>#<slot>`), attributed to the node the API
//!   layer made current on that thread ([`set_current_node`], from
//!   `nros::census_hooks`). The API layer then re-labels the slot it just
//!   registered with the census's own id ([`label_last`]) so the two files
//!   join row for row. Registration is recorded whether or not a run is
//!   profiling: it is setup-time, once per entity, and the switch may be read
//!   after the first registration.
//! * **Dispatch** (`dispatch_begin` / `dispatch_end`, around `try_process`)
//!   times the call on the steady clock (`nros_platform_clock_ns`, the one
//!   every timer reads) and counts it only when it did work (`Ok(true)`): a
//!   timer polled before it is due is not an invocation.
//! * **Publish** ([`on_publish`], from the publisher handles) adds the topic to
//!   the set of the callback dispatching ON THIS THREAD.
//!
//! # What this module does NOT do, and who does
//!
//! It names no thread and touches no file: the core is `core + alloc`
//! (phase-359; `check-std-census` holds `nros-node` at zero `std::` paths).
//! The hosted EDGE that reads `$NROS_PROFILE_OUT` (`nros-cpp`'s init, the Linux
//! funnel) supplies both through [`install`]: a `thread_key` that tells
//! concurrently-dispatching tier threads apart, and a `sink` that writes the
//! rendered document where the switch named.
//!
//! Host only: no RTOS umbrella resolves `profile-mode`
//! (`check-rtos-feature-set-excludes-analysis`), and every call site is
//! `#[cfg(feature = "profile-mode")]`, so an RTOS image carries none of it —
//! measured byte-identical (phase-463 W5 I3c).

use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::{String, ToString},
    vec::Vec,
};
use core::sync::atomic::{AtomicBool, Ordering};

use nros_rmw::sync::Mutex;

use super::arena::{EntryKind, TraceName};

/// The artifact schema — issue 0403's, so one reader serves the bench and this.
pub const SCHEMA: &str = "nros.wcet.measurements/1";

/// The CPU a host profile names (`host-<arch>-<profile>`).
#[cfg(target_arch = "x86_64")]
pub const HOST_ARCH: &str = "x86_64";
/// The CPU a host profile names (`host-<arch>-<profile>`).
#[cfg(target_arch = "aarch64")]
pub const HOST_ARCH: &str = "aarch64";
/// The CPU a host profile names (`host-<arch>-<profile>`).
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
pub const HOST_ARCH: &str = "unknown";

static ENABLED: AtomicBool = AtomicBool::new(false);

fn now_ns() -> u64 {
    let t = nros_core::clock::Clock::steady().now().to_nanos();
    if t < 0 { 0 } else { t as u64 }
}

struct Row {
    exec: usize,
    slot: usize,
    kind: &'static str,
    node: String,
    /// The census's id for this callback when the API layer supplied one,
    /// otherwise the executor's own name for it.
    id: String,
    /// The executor's own name (`/topic`, `timer@<us>us`, `guard#3`).
    executor_name: String,
    period_us: Option<u64>,
    iterations: u64,
    min_ns: u64,
    max_ns: u64,
    total_ns: u64,
    /// Distinct published-topic sets -> how many invocations published exactly
    /// that set (the empty set counts too: a tick that published nothing is
    /// evidence about the path, not an absence).
    output_sets: BTreeMap<Vec<String>, u64>,
    /// Superseded by a later registration in the same slot: kept in the file
    /// (it ran), but no longer the slot's occupant.
    retired: bool,
}

/// One thread's in-flight dispatch: (row index, start ns, topics published).
type InFlight = (usize, u64, BTreeSet<String>);

struct State {
    rows: Vec<Row>,
    /// Per thread key: the node the API layer made current, and the row this
    /// thread registered last. Per THREAD, not per process: a tiered native
    /// boot runs each tier's setup on its own task, concurrently, and a global
    /// cursor attributed one tier's timer to another tier's node (measured on
    /// `examples/workspaces/derived-tiers-cpp`, the first run of this module).
    cursors: BTreeMap<usize, (Option<String>, Option<usize>)>,
    in_flight: BTreeMap<usize, InFlight>,
    thread_key: fn() -> usize,
    sink: Option<fn(&str)>,
    launch: String,
    inputs: String,
    started_ns: u64,
    last_flush_ns: u64,
}

/// A build with no hosted edge installed: one thread, so one cursor.
fn single_thread() -> usize {
    0
}

static STATE: Mutex<State> = Mutex::new(State {
    rows: Vec::new(),
    cursors: BTreeMap::new(),
    in_flight: BTreeMap::new(),
    thread_key: single_thread,
    sink: None,
    launch: String::new(),
    inputs: String::new(),
    started_ns: 0,
    last_flush_ns: 0,
});

fn kind_name(kind: EntryKind) -> &'static str {
    match kind {
        EntryKind::Subscription => "subscription",
        EntryKind::Service => "service",
        EntryKind::ServiceClient => "service_client",
        EntryKind::Timer => "timer",
        EntryKind::ActionServer => "action_server",
        EntryKind::ActionClient => "action_client",
        EntryKind::GuardCondition => "guard_condition",
    }
}

/// Install the hosted edge's two capabilities. Call BEFORE the first executor
/// opens, so every registration is attributed per thread: `thread_key` tells
/// the threads apart (any value unique per live thread), and `sink` receives
/// the rendered document — at most once a second while dispatching, and on
/// [`flush`]. Does not start timing; [`enable`] does.
pub fn install(thread_key: fn() -> usize, sink: fn(&str)) {
    STATE.with(|st| {
        st.thread_key = thread_key;
        st.sink = Some(sink);
    });
}

/// Start profiling. `launch` and `inputs` are the run's `coverage`
/// (RFC-0078 D1b: a maximum is only as good as what was exercised, so the
/// file says what that was).
pub fn enable(launch: &str, inputs: &str) {
    let now = now_ns();
    STATE.with(|st| {
        st.launch = launch.to_string();
        st.inputs = inputs.to_string();
        st.started_ns = now;
        st.last_flush_ns = now;
    });
    ENABLED.store(true, Ordering::Release);
}

/// Is this process profiling?
#[inline]
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Acquire)
}

/// The node subsequent registrations ON THIS THREAD attribute to
/// (`nros::census_hooks`' cursor, the same one the census uses).
pub fn set_current_node(fqn: &str) {
    STATE.with(|st| {
        let key = (st.thread_key)();
        st.cursors.entry(key).or_default().0 = Some(fqn.to_string());
    });
}

/// Re-label the slot THIS THREAD registered last with the census's id for it,
/// so the two files join one-to-one. Called by the API layer right after the
/// registration it hooks (timers and guard conditions carry no name in the
/// executor).
pub fn label_last(id: &str) {
    STATE.with(|st| {
        let key = (st.thread_key)();
        if let Some(i) = st.cursors.get(&key).and_then(|c| c.1)
            && let Some(row) = st.rows.get_mut(i)
        {
            row.id = id.to_string();
        }
    });
}

pub(crate) fn on_register(exec: usize, slot: usize, kind: EntryKind, name: TraceName<'_>) {
    let (executor_name, period_us) = match name {
        TraceName::Text(s) => (s.to_string(), None),
        TraceName::TimerPeriod(us) => (alloc::format!("timer@{us}us"), Some(us)),
        TraceName::Slot(label, slot) => (alloc::format!("{label}#{slot}"), None),
    };
    STATE.with(|st| {
        // A reused slot retires its previous occupant's row.
        for row in st.rows.iter_mut() {
            if row.exec == exec && row.slot == slot {
                row.retired = true;
            }
        }
        let key = (st.thread_key)();
        let node = st
            .cursors
            .get(&key)
            .and_then(|c| c.0.clone())
            .unwrap_or_default();
        st.rows.push(Row {
            exec,
            slot,
            kind: kind_name(kind),
            node,
            id: executor_name.clone(),
            executor_name,
            period_us,
            iterations: 0,
            min_ns: u64::MAX,
            max_ns: 0,
            total_ns: 0,
            output_sets: BTreeMap::new(),
            retired: false,
        });
        let idx = st.rows.len() - 1;
        st.cursors.entry(key).or_default().1 = Some(idx);
    });
}

/// Before `try_process`. A no-op unless [`enabled`].
#[inline]
pub(crate) fn dispatch_begin(exec: usize, slot: usize) {
    if !enabled() {
        return;
    }
    let start = now_ns();
    STATE.with(|st| {
        let Some(row) = st
            .rows
            .iter()
            .rposition(|r| r.exec == exec && r.slot == slot && !r.retired)
        else {
            return;
        };
        let key = (st.thread_key)();
        st.in_flight.insert(key, (row, start, BTreeSet::new()));
    });
}

/// After `try_process`; `did_work` is its `Ok(true)`.
#[inline]
pub(crate) fn dispatch_end(did_work: bool) {
    if !enabled() {
        return;
    }
    let end = now_ns();
    let flush = STATE.with(|st| {
        let key = (st.thread_key)();
        let (row, start, topics) = st.in_flight.remove(&key)?;
        if did_work && let Some(r) = st.rows.get_mut(row) {
            let ns = end.saturating_sub(start);
            r.iterations += 1;
            r.total_ns += ns;
            r.min_ns = r.min_ns.min(ns);
            r.max_ns = r.max_ns.max(ns);
            *r.output_sets
                .entry(topics.into_iter().collect())
                .or_insert(0) += 1;
        }
        if end.saturating_sub(st.last_flush_ns) >= 1_000_000_000 {
            st.last_flush_ns = end;
            let sink = st.sink?;
            return Some((sink, render(st, end)));
        }
        None
    });
    // Outside the lock: the sink does I/O.
    if let Some((sink, doc)) = flush {
        sink(&doc);
    }
}

/// A publish on `topic`, attributed to the callback dispatching on this thread
/// (none outside a dispatch: a publish from `main` belongs to no path).
#[inline]
pub fn on_publish(topic: &str) {
    if !enabled() {
        return;
    }
    STATE.with(|st| {
        let key = (st.thread_key)();
        if let Some((_, _, topics)) = st.in_flight.get_mut(&key)
            && !topics.contains(topic)
        {
            topics.insert(topic.to_string());
        }
    });
}

/// Render the measurements and hand them to the installed sink now (also done
/// once a second while dispatching). A no-op with no sink installed.
pub fn flush() {
    let now = now_ns();
    let out = STATE.with(|st| {
        let sink = st.sink?;
        Some((sink, render(st, now)))
    });
    if let Some((sink, doc)) = out {
        sink(&doc);
    }
}

/// The document, as [`flush`] would hand it to the sink.
pub fn render_now() -> String {
    let now = now_ns();
    STATE.with(|st| render(st, now))
}

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    for ch in s.chars() {
        match ch {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            c if (c as u32) < 0x20 => o.push_str(&alloc::format!("\\u{:04x}", c as u32)),
            c => o.push(c),
        }
    }
    o
}

/// The `nros.wcet.measurements/1` document, host flavour.
///
/// Differences from the bench's artifact (issue 0403), each stated in the file:
/// the unit is `ns` (`unit`), the time base is the host's monotonic clock so
/// `clock_hz` is `0` and `convertible_to_time` is `false` (a host clock is not a
/// fact of the image), rows are CALLBACKS with `max_observed` rather than
/// primitives, and there is no `bound_cycles` anywhere (RFC-0078 D1b).
fn render(st: &State, now: u64) -> String {
    use core::fmt::Write as _;
    let mut o = String::new();
    let duration_ms = now.saturating_sub(st.started_ns) / 1_000_000;
    let _ = write!(
        o,
        "{{\"schema\":\"{SCHEMA}\",\"profile\":\"host-{HOST_ARCH}-release\",\"unit\":\"ns\",\
         \"conditions\":{{\"cpu\":\"{HOST_ARCH}\",\"clock_hz\":0,\"time_base\":\"host monotonic \
         (nros_platform_clock_ns)\",\"counter_valid\":true}},\"convertible_to_time\":false,\
         \"coverage\":{{\"launch\":\"{}\",\"inputs\":\"{}\",\"duration_ms\":{duration_ms}}},\
         \"measurements\":[",
        esc(&st.launch),
        esc(&st.inputs),
    );
    for (i, r) in st.rows.iter().enumerate() {
        if i > 0 {
            o.push(',');
        }
        let _ = write!(
            o,
            "{{\"node\":\"{}\",\"id\":\"{}\",\"kind\":\"{}\",\"executor_name\":\"{}\",",
            esc(&r.node),
            esc(&r.id),
            r.kind,
            esc(&r.executor_name)
        );
        match r.period_us {
            Some(p) => {
                let _ = write!(o, "\"period_us\":{p},");
            }
            None => o.push_str("\"period_us\":null,"),
        }
        // A row that never ran says so with nulls rather than zeros: 0 would
        // read as "free", the issue-0403 failure.
        if r.iterations == 0 {
            o.push_str(
                "\"iterations\":0,\"min_observed\":null,\"max_observed\":null,\"mean_observed\":null,",
            );
        } else {
            let _ = write!(
                o,
                "\"iterations\":{},\"min_observed\":{},\"max_observed\":{},\"mean_observed\":{},",
                r.iterations,
                r.min_ns,
                r.max_ns,
                r.total_ns / r.iterations
            );
        }
        let mut union: BTreeSet<&String> = BTreeSet::new();
        o.push_str("\"output_sets\":[");
        for (j, (set, n)) in r.output_sets.iter().enumerate() {
            if j > 0 {
                o.push(',');
            }
            o.push_str("{\"topics\":[");
            for (k, t) in set.iter().enumerate() {
                if k > 0 {
                    o.push(',');
                }
                union.insert(t);
                let _ = write!(o, "\"{}\"", esc(t));
            }
            let _ = write!(o, "],\"invocations\":{n}}}");
        }
        o.push_str("],\"observed_outputs\":[");
        for (j, t) in union.iter().enumerate() {
            if j > 0 {
                o.push(',');
            }
            let _ = write!(o, "\"{}\"", esc(t));
        }
        let _ = write!(o, "],\"retired\":{}}}", r.retired);
    }
    o.push_str("]}\n");
    o
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One process-wide state, so everything runs as ONE test.
    #[test]
    fn a_timed_dispatch_records_ns_and_the_topics_it_published() {
        let exec = 0xdead_0000;
        set_current_node("/talker");
        on_register(exec, 0, EntryKind::Timer, TraceName::TimerPeriod(100_000));
        label_last("timer0");
        on_register(exec, 1, EntryKind::Subscription, TraceName::Text("/in"));

        // Not enabled: nothing is timed.
        dispatch_begin(exec, 0);
        on_publish("/chatter");
        dispatch_end(true);

        enable("unit-test", "synthetic");
        dispatch_begin(exec, 0);
        on_publish("/chatter");
        on_publish("/chatter");
        dispatch_end(true);
        // A poll that did no work is not an invocation.
        dispatch_begin(exec, 0);
        dispatch_end(false);
        // A publish outside any dispatch belongs to no path.
        on_publish("/stray");

        let text = render_now();
        assert!(
            text.contains("\"schema\":\"nros.wcet.measurements/1\""),
            "{text}"
        );
        assert!(text.contains("\"unit\":\"ns\""), "{text}");
        assert!(text.contains("\"clock_hz\":0"), "{text}");
        assert!(!text.contains("bound_cycles"), "RFC-0078 D1b: {text}");
        assert!(
            text.contains("\"node\":\"/talker\",\"id\":\"timer0\",\"kind\":\"timer\""),
            "{text}"
        );
        assert!(
            text.contains("\"iterations\":1,"),
            "one timed invocation: {text}"
        );
        assert!(
            text.contains("\"output_sets\":[{\"topics\":[\"/chatter\"],\"invocations\":1}]"),
            "{text}"
        );
        assert!(!text.contains("/stray"), "{text}");
        // The subscription never ran: nulls, never zeros.
        assert!(
            text.contains("\"iterations\":0,\"min_observed\":null,\"max_observed\":null"),
            "{text}"
        );
    }
}
