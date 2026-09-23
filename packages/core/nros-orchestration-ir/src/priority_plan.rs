//! phase-459 W4 (issue 1427, RFC-0079) - the board's priority ADDRESS PLAN,
//! as the realizer's input.
//!
//! RFC-0079's rule is that a priority is ALLOCATED out of a board's address
//! plan, never authored per tier. Until this wave the realizer did not read a
//! plan at all: `rank_to_priority` mapped dense rank 0 onto the most urgent
//! number the kernel has (Zephyr 0), which sits ABOVE the transport threads
//! that feed the application - the inversion RFC-0079 exists to prevent, and
//! issue 1427 measured it on the Autoware Safety Island.
//!
//! A plan states four things, and this module is the only place they are
//! spelled in Rust:
//!
//! * `direction` - which way the numbers run (`smaller-is-urgent` on Zephyr
//!   and ThreadX, `bigger-is-urgent` on FreeRTOS, NuttX and POSIX);
//! * `range` - the kernel's usable priorities;
//! * `reserved` bands - what the image gives to something that is NOT an
//!   application tier (the transport, a driver);
//! * `pool.app` - the band a derived tier may be allocated from.
//!
//! # Static plans and the DERIVED Zephyr one
//!
//! Every other port's reserved band is a literal read off the port and is
//! stated in its descriptor (`packages/boards/*/nros-board.toml`
//! `[board.priority_plan]`, `packages/platform/*/nros-platform.toml`
//! `[priority_plan]`). Zephyr's is COMPUTED, per image, from Kconfig: the
//! zenoh-pico read/lease tasks are created through the POSIX layer, so their
//! k_thread priority depends on `CONFIG_NUM_PREEMPT_PRIORITIES` and on the
//! two `CONFIG_NROS_ZENOH_*_PRIORITY` bands. [`zephyr_plan`] is that
//! arithmetic, and [`PriorityPlan::from_zephyr_dotconfig`] runs it against one
//! image's `.config`.
//!
//! The same formula exists once more, in Python
//! (`scripts/lib/priority_plan.py:resolve_zephyr_plan`), and that copy is
//! deliberate: it is the CHECKER of this one
//! (`scripts/check-tier-priority-plan-image.py`, which judges a built image
//! and can therefore see what a unit test cannot). Two implementations of one
//! formula, one of them a test - not two sources.
//!
//! # Why the static tables below are not a second spelling of the descriptors
//!
//! They would be, if nothing compared them. `plans_match_the_descriptors` (in
//! this module's tests) parses every `[board.priority_plan]` /
//! `[priority_plan]` table in the tree and asserts this file agrees with it,
//! so the descriptor stays the source and this table stays a projection of it.
//! The leaf crate cannot READ those files at run time - it is linked into a
//! proc-macro and given a tier key, not a tree - which is why the projection
//! exists at all.

use std::collections::BTreeMap;

/// Which way a kernel's priority numbers run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    /// Zephyr, ThreadX: a numerically SMALLER priority is more urgent.
    SmallerIsUrgent,
    /// FreeRTOS, NuttX, POSIX: a numerically LARGER priority is more urgent.
    BiggerIsUrgent,
}

/// A closed band of priorities, `lo <= hi` in NUMERIC order (never in urgency
/// order - the direction says which end is urgent).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Band {
    pub lo: i64,
    pub hi: i64,
}

impl Band {
    pub fn new(lo: i64, hi: i64) -> Self {
        Self { lo, hi }
    }

    /// How many priorities the band holds; 0 when it is empty (`hi < lo`),
    /// which is what an image whose transport owns the whole range resolves
    /// to and is reported rather than silently allocated into.
    pub fn width(&self) -> usize {
        if self.hi < self.lo {
            0
        } else {
            (self.hi - self.lo + 1) as usize
        }
    }

    pub fn contains(&self, p: i64) -> bool {
        self.lo <= p && p <= self.hi
    }
}

/// One board's priority address plan (RFC-0079).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PriorityPlan {
    pub direction: Direction,
    /// The kernel's usable priorities.
    pub range: Band,
    /// Bands that are NOT the application's, by name (`transport`, a driver
    /// band). Kept whole so a diagnostic can name the band a tier collided
    /// with.
    pub reserved: BTreeMap<String, Band>,
    /// The band a derived tier is allocated from.
    pub app: Band,
    /// Where this plan came from, for a diagnostic: a descriptor path, an
    /// image's `.config`, or the defaults projection.
    pub source: String,
}

/// Why an image's `.config` yields no plan.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PriorityPlanError {
    #[error("{key} is absent - not a Zephyr .config?")]
    MissingKey { key: String },
    /// The image applies no transport priority at all, so nothing is reserved
    /// and the transport inherits its creator (the NuttX pre-0736 state).
    /// Judged, never treated as a pass: see `check-tier-priority-plan-image.py`.
    #[error(
        "CONFIG_POSIX_PRIORITY_SCHEDULING and/or CONFIG_PREEMPT_ENABLED is off - the \
         transport priority is NOT applied in this image; the tasks inherit their \
         creator and no band can be reserved"
    )]
    Unapplied,
}

/// The platform ABI's normalised priority band (`NROS_PLATFORM_PRIORITY_MAX`,
/// `nros-platform/include/nros/platform.h`): 0 least urgent, 255 most.
pub const NROS_PLATFORM_PRIORITY_MAX: i64 = 255;

/// Zephyr's own default for `CONFIG_NUM_PREEMPT_PRIORITIES`.
const ZEPHYR_DEFAULT_NUM_PREEMPT: i64 = 15;
/// Zephyr's own default for `CONFIG_NUM_COOP_PRIORITIES`.
const ZEPHYR_DEFAULT_NUM_COOP: i64 = 16;
/// nano-ros's default for `CONFIG_NROS_ZENOH_READ_PRIORITY` (`zephyr/Kconfig`).
const ZENOH_DEFAULT_READ_BAND: i64 = 200;
/// nano-ros's default for `CONFIG_NROS_ZENOH_LEASE_PRIORITY` (`zephyr/Kconfig`).
/// The same 200 as the read band; an image may raise it (the Autoware Safety
/// Island sets 255), which widens the reserved band without moving the pool.
const ZENOH_DEFAULT_LEASE_BAND: i64 = 200;

/// The normalised band -> POSIX priority half of the chain, mirroring
/// `nros_zephyr_native_priority` (`nros-platform-zephyr/src/platform.c`):
/// `lo + band * (hi - lo) / 255`, truncating, clamped to `[lo, hi]`.
fn band_to_posix(band: i64, num_preempt: i64) -> i64 {
    let (lo, hi) = (0, num_preempt - 1);
    if hi < lo {
        return lo;
    }
    let n = band.clamp(0, NROS_PLATFORM_PRIORITY_MAX);
    let want = lo + (n * (hi - lo)) / NROS_PLATFORM_PRIORITY_MAX;
    want.clamp(lo, hi)
}

/// `POSIX_TO_ZEPHYR_PRIORITY(prio, SCHED_RR)` - Zephyr's
/// `lib/posix/options/pthread.c`.
fn posix_rr_to_kthread(posix: i64, num_preempt: i64) -> i64 {
    num_preempt - posix - 1
}

/// The DERIVED Zephyr plan (RFC-0079 section 4.1), from the four Kconfig
/// values that decide it.
///
/// Tiers are RAW `k_thread` priorities, so the transport band has to end in
/// k_thread units for a comparison against a tier to mean anything. The
/// application pool starts one step LESS urgent than the least urgent
/// transport thread: a derived tier never preempts the transport that feeds
/// it.
pub fn zephyr_plan(
    num_preempt: i64,
    num_coop: i64,
    read_band: i64,
    lease_band: i64,
    source: impl Into<String>,
) -> PriorityPlan {
    let ks = {
        let mut ks = [
            posix_rr_to_kthread(band_to_posix(read_band, num_preempt), num_preempt),
            posix_rr_to_kthread(band_to_posix(lease_band, num_preempt), num_preempt),
        ];
        ks.sort_unstable();
        ks
    };
    let mut reserved = BTreeMap::new();
    reserved.insert("transport".to_string(), Band::new(ks[0], ks[1]));
    PriorityPlan {
        direction: Direction::SmallerIsUrgent,
        // Negative k_thread priorities are cooperative.
        range: Band::new(-num_coop, num_preempt - 1),
        reserved,
        app: Band::new(ks[1] + 1, num_preempt - 1),
        source: source.into(),
    }
}

/// `CONFIG_x=<int>` pairs out of a Zephyr `.config`.
fn dotconfig_ints(text: &str) -> BTreeMap<&str, i64> {
    let mut out = BTreeMap::new();
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        if let Ok(n) = v.trim().parse::<i64>() {
            out.insert(k.trim(), n);
        }
    }
    out
}

fn dotconfig_has(text: &str, key: &str) -> bool {
    text.lines().any(|l| l.trim() == format!("{key}=y"))
}

impl PriorityPlan {
    /// The plan for a tier KEY, with no image in hand.
    ///
    /// `target` is the output of `BoardFamily::tier_rtos_key` and is matched
    /// EXACTLY, the same rule `sched_caps_for` follows (issue 1285). An
    /// unrecognised key - including `""`, a board with no RTOS family - gets
    /// the whole range and no reserved band, which is the pre-RFC-0079
    /// behaviour and is what a target with no descriptor has ever promised.
    ///
    /// For Zephyr this is the DEFAULTS projection, not one image's answer:
    /// Zephyr's own `CONFIG_NUM_PREEMPT_PRIORITIES` / `CONFIG_NUM_COOP_PRIORITIES`
    /// defaults with nano-ros's two `CONFIG_NROS_ZENOH_*_PRIORITY` defaults,
    /// through the same [`zephyr_plan`] arithmetic. An image that changes any
    /// of the four resolves its own plan with
    /// [`PriorityPlan::from_zephyr_dotconfig`], and
    /// `check-tier-priority-plan-image.py` is what judges a BUILT image
    /// against its `.config` - a unit test cannot see one.
    pub fn for_target(target: &str) -> Self {
        let band = |lo, hi| Band::new(lo, hi);
        let reserved = |name: &str, lo, hi| {
            let mut m = BTreeMap::new();
            m.insert(name.to_string(), band(lo, hi));
            m
        };
        match target {
            // packages/boards/zephyr/nros-board.toml: `derived = "zephyr"`.
            "zephyr" => zephyr_plan(
                ZEPHYR_DEFAULT_NUM_PREEMPT,
                ZEPHYR_DEFAULT_NUM_COOP,
                ZENOH_DEFAULT_READ_BAND,
                ZENOH_DEFAULT_LEASE_BAND,
                "derived from the Kconfig DEFAULTS (RFC-0079 section 4.1); an \
                 image's own .config refines it",
            ),
            // packages/platform/nros-platform-freertos/nros-platform.toml.
            "freertos" => PriorityPlan {
                direction: Direction::BiggerIsUrgent,
                range: band(1, 7),
                reserved: reserved("transport", 4, 4),
                app: band(1, 3),
                source: "packages/platform/nros-platform-freertos/nros-platform.toml".into(),
            },
            // packages/boards/nros-board-threadx-linux/nros-board.toml.
            "threadx" => PriorityPlan {
                direction: Direction::SmallerIsUrgent,
                range: band(0, 31),
                reserved: reserved("transport", 14, 14),
                app: band(15, 31),
                source: "packages/boards/nros-board-threadx-linux/nros-board.toml".into(),
            },
            // packages/boards/nros-board-nuttx-qemu/nros-board.toml.
            "nuttx" => PriorityPlan {
                direction: Direction::BiggerIsUrgent,
                range: band(1, 255),
                reserved: reserved("transport", 100, 100),
                app: band(1, 99),
                source: "packages/boards/nros-board-nuttx-qemu/nros-board.toml".into(),
            },
            // packages/boards/linux/nros-board.toml. RFC-0079 records POSIX as
            // HALF-SOLVED: the tiers are SCHED_FIFO and the transport is
            // SCHED_OTHER, so the reserved band is an ordering inside the
            // executor's space rather than a kernel band this wave can claim.
            // Allocating inside `pool.app` is still the right move - it is
            // what the descriptor says the application owns.
            "posix" | "native" => PriorityPlan {
                direction: Direction::BiggerIsUrgent,
                range: band(1, 99),
                reserved: reserved("transport", 90, 99),
                app: band(1, 89),
                source: "packages/boards/linux/nros-board.toml".into(),
            },
            _ => PriorityPlan {
                direction: Direction::BiggerIsUrgent,
                range: band(0, 0),
                reserved: BTreeMap::new(),
                app: band(0, 0),
                source: format!("no priority plan for tier key {target:?}"),
            },
        }
    }

    /// The pre-RFC-0079 plan: the whole of a board's priority range, nothing
    /// reserved. Used where only [`crate::rtos_realizer::SchedCaps`] is known
    /// (a synthetic board in a test), never on a bake road.
    pub fn whole_range(n_priorities: u16, low_number_is_high: bool) -> Self {
        let hi = i64::from(n_priorities.max(1)) - 1;
        Self {
            direction: if low_number_is_high {
                Direction::SmallerIsUrgent
            } else {
                Direction::BiggerIsUrgent
            },
            range: Band::new(0, hi),
            reserved: BTreeMap::new(),
            app: Band::new(0, hi),
            source: "the board's whole priority range (no plan)".into(),
        }
    }

    /// Resolve the DERIVED Zephyr plan against ONE image's `.config`.
    ///
    /// The `.config` is the only honest input: both ends of the chain are
    /// per-image Kconfig, so a literal band would be true for one build and
    /// quietly wrong for the next - the failure RFC-0079 exists to eliminate
    /// one level up.
    pub fn from_zephyr_dotconfig(text: &str) -> Result<Self, PriorityPlanError> {
        let cfg = dotconfig_ints(text);
        let num_preempt =
            *cfg.get("CONFIG_NUM_PREEMPT_PRIORITIES")
                .ok_or(PriorityPlanError::MissingKey {
                    key: "CONFIG_NUM_PREEMPT_PRIORITIES".into(),
                })?;
        let num_coop = cfg.get("CONFIG_NUM_COOP_PRIORITIES").copied().unwrap_or(0);
        // Both gates must be on, or `nros_zephyr_native_priority` returns -1
        // and the tasks inherit their creator (platform.c + issue 0766).
        if !(dotconfig_has(text, "CONFIG_POSIX_PRIORITY_SCHEDULING")
            && dotconfig_has(text, "CONFIG_PREEMPT_ENABLED"))
        {
            return Err(PriorityPlanError::Unapplied);
        }
        // Absent means the Kconfig default applies. The Python resolver reads
        // it out of `zephyr/Kconfig`; this crate has no tree to read, so the
        // defaults are the two constants above and they are checked against
        // that file by `check-zephyr-priority-plan-defaults` (the Python
        // selftest asserts the same triple).
        let read = cfg
            .get("CONFIG_NROS_ZENOH_READ_PRIORITY")
            .copied()
            .unwrap_or(ZENOH_DEFAULT_READ_BAND);
        let lease = cfg
            .get("CONFIG_NROS_ZENOH_LEASE_PRIORITY")
            .copied()
            .unwrap_or(ZENOH_DEFAULT_LEASE_BAND);
        Ok(zephyr_plan(
            num_preempt,
            num_coop,
            read,
            lease,
            "derived from an image .config (RFC-0079 section 4.1)",
        ))
    }

    /// The `k`-th most urgent priority in `pool.app` (`k = 0` is the most
    /// urgent one the application may have), or `None` when the pool is
    /// narrower than `k + 1`.
    pub fn nth_app_priority(&self, k: usize) -> Option<i64> {
        if k >= self.app.width() {
            return None;
        }
        let step = k as i64;
        Some(match self.direction {
            Direction::SmallerIsUrgent => self.app.lo + step,
            Direction::BiggerIsUrgent => self.app.hi - step,
        })
    }

    /// The LEAST urgent priority in `pool.app` - where a rank past the pool's
    /// width is clamped (loudly: the realizer records a `Degradation`).
    pub fn least_urgent_app_priority(&self) -> i64 {
        match self.direction {
            Direction::SmallerIsUrgent => self.app.hi,
            Direction::BiggerIsUrgent => self.app.lo,
        }
    }

    /// The reserved band a priority lands on, if any - for a diagnostic that
    /// names what a tier collided with.
    pub fn reserved_band_of(&self, priority: i64) -> Option<(&str, Band)> {
        self.reserved
            .iter()
            .find(|(_, b)| b.contains(priority))
            .map(|(n, b)| (n.as_str(), *b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    /// The island's image, from its Kconfig: 15 preemptive priorities, the two
    /// zenoh bands at their nano-ros defaults.
    fn island_dotconfig() -> String {
        "CONFIG_NUM_PREEMPT_PRIORITIES=15\n\
         CONFIG_NUM_COOP_PRIORITIES=16\n\
         CONFIG_POSIX_PRIORITY_SCHEDULING=y\n\
         CONFIG_PREEMPT_ENABLED=y\n\
         CONFIG_NROS_ZENOH_READ_PRIORITY=200\n\
         CONFIG_NROS_ZENOH_LEASE_PRIORITY=255\n"
            .to_string()
    }

    /// The measured projection phase-459 records: transport at k_thread 4, the
    /// application pool `[5, 14]`. The same triple is asserted by
    /// `scripts/check-tier-priority-plan-image.py --selftest`.
    #[test]
    fn priority_plan_resolves_the_island_band_from_a_dotconfig() {
        let plan = PriorityPlan::from_zephyr_dotconfig(&island_dotconfig()).expect("resolves");
        assert_eq!(plan.direction, Direction::SmallerIsUrgent);
        assert_eq!(plan.range, Band::new(-16, 14));
        assert_eq!(plan.reserved["transport"], Band::new(0, 4));
        assert_eq!(plan.app, Band::new(5, 14));
        // Rank 0 is the most urgent priority the APPLICATION owns, which is
        // one step below the transport - not Zephyr's 0 (issue 1427).
        assert_eq!(plan.nth_app_priority(0), Some(5));
        assert_eq!(plan.nth_app_priority(1), Some(6));
        assert_eq!(plan.nth_app_priority(9), Some(14));
        assert_eq!(plan.nth_app_priority(10), None, "the pool holds ten");
    }

    /// The defaults projection and the island's image differ in ONE input -
    /// the island raises the lease band from Kconfig's 200 to 255 - and that
    /// widens the reserved band without moving the pool. So a bake with no
    /// `.config` in hand allocates the same priorities; it just knows less
    /// about what else is reserved.
    #[test]
    fn priority_plan_for_zephyr_matches_the_defaults_image() {
        let from_cfg = PriorityPlan::from_zephyr_dotconfig(&island_dotconfig()).expect("resolves");
        let for_target = PriorityPlan::for_target("zephyr");
        assert_eq!(for_target.app, from_cfg.app, "the pool is the same");
        assert_eq!(for_target.range, from_cfg.range);
        assert_eq!(
            for_target.reserved["transport"],
            Band::new(4, 4),
            "both bands at Kconfig's 200 resolve to one k_thread priority"
        );
        assert_eq!(for_target.nth_app_priority(0), Some(5));
    }

    /// The pre-0852 band (`READ_PRIORITY=16` on the 0-255 scale) resolves the
    /// STALE transport band the tier-2 lane tripped over: k_thread 14, the
    /// least urgent preemptive priority, which leaves `pool.app` EMPTY. It is
    /// reported as a zero-width pool rather than silently allocated into.
    #[test]
    fn priority_plan_reports_the_stale_band_as_an_empty_pool() {
        let stale = island_dotconfig().replace(
            "CONFIG_NROS_ZENOH_READ_PRIORITY=200",
            "CONFIG_NROS_ZENOH_READ_PRIORITY=16",
        );
        let plan = PriorityPlan::from_zephyr_dotconfig(&stale).expect("resolves");
        assert_eq!(plan.reserved["transport"], Band::new(0, 14));
        assert_eq!(
            plan.app.width(),
            0,
            "pool [15, 14] is empty: {:?}",
            plan.app
        );
        assert_eq!(plan.nth_app_priority(0), None);
    }

    /// An image whose Kconfig gates are off applies no band at all: the
    /// transport inherits its creator and nothing can be reserved.
    #[test]
    fn priority_plan_refuses_an_image_that_applies_no_band() {
        let text =
            island_dotconfig().replace("CONFIG_PREEMPT_ENABLED=y", "CONFIG_PREEMPT_ENABLED=n");
        assert_eq!(
            PriorityPlan::from_zephyr_dotconfig(&text),
            Err(PriorityPlanError::Unapplied)
        );
        assert!(matches!(
            PriorityPlan::from_zephyr_dotconfig("CONFIG_FOO=y\n"),
            Err(PriorityPlanError::MissingKey { .. })
        ));
    }

    /// Direction decides which END of the pool rank 0 takes.
    #[test]
    fn priority_plan_allocates_from_the_urgent_end_of_the_pool() {
        let posix = PriorityPlan::for_target("posix");
        assert_eq!(posix.direction, Direction::BiggerIsUrgent);
        assert_eq!(posix.nth_app_priority(0), Some(89));
        assert_eq!(posix.nth_app_priority(1), Some(88));
        assert_eq!(posix.least_urgent_app_priority(), 1);

        let threadx = PriorityPlan::for_target("threadx");
        assert_eq!(threadx.nth_app_priority(0), Some(15));
        assert_eq!(threadx.least_urgent_app_priority(), 31);

        // A key with no RTOS family gets no plan and no pool to allocate from.
        assert_eq!(PriorityPlan::for_target("").app.width(), 1);
    }

    /// No derived priority may land on a reserved band, by construction.
    #[test]
    fn priority_plan_pools_never_overlap_a_reserved_band() {
        for key in ["zephyr", "freertos", "threadx", "nuttx", "posix"] {
            let plan = PriorityPlan::for_target(key);
            for k in 0..plan.app.width() {
                let p = plan.nth_app_priority(k).expect("inside the pool");
                assert!(
                    plan.reserved_band_of(p).is_none(),
                    "{key}: pool priority {p} lands on a reserved band: {:?}",
                    plan.reserved
                );
                assert!(
                    plan.range.contains(p),
                    "{key}: {p} outside {:?}",
                    plan.range
                );
            }
        }
    }

    // ---- the descriptors are the source; this file is a projection --------

    fn repo_root() -> PathBuf {
        // <repo>/packages/core/nros-orchestration-ir -> <repo>
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("repo root")
            .to_path_buf()
    }

    /// Every `[board.priority_plan]` / `[priority_plan]` table in the tree,
    /// keyed by `tier_key`, skipping the DERIVED ones (Zephyr states no
    /// numbers by design).
    fn descriptor_plans() -> BTreeMap<String, (String, Band, Band)> {
        let mut out = BTreeMap::new();
        let mut visit = |dir: &Path, file: &str, under_board: bool| {
            let Ok(entries) = std::fs::read_dir(dir) else {
                return;
            };
            for e in entries.flatten() {
                let path = e.path().join(file);
                let Ok(raw) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let Ok(doc) = raw.parse::<toml::Table>() else {
                    continue;
                };
                // `nros-board.toml` opens with `[[board]]`, an ARRAY of
                // tables, so `[board.priority_plan]` attaches to its last
                // element - not to a `[board]` table.
                let board = doc.get("board").and_then(|b| match b {
                    toml::Value::Array(a) => a.last(),
                    other => Some(other),
                });
                let plan = if under_board {
                    board.and_then(|b| b.get("priority_plan"))
                } else {
                    doc.get("priority_plan")
                };
                let Some(plan) = plan else { continue };
                if plan.get("derived").is_some() {
                    continue;
                }
                let key = plan
                    .get("tier_key")
                    .and_then(toml::Value::as_str)
                    .or_else(|| {
                        board
                            .and_then(|b| b.get("platform"))
                            .and_then(toml::Value::as_str)
                    })
                    .unwrap_or_default()
                    .to_string();
                let pair = |v: Option<&toml::Value>| {
                    v.and_then(toml::Value::as_array).map(|a| {
                        Band::new(
                            a[0].as_integer().expect("lo"),
                            a[1].as_integer().expect("hi"),
                        )
                    })
                };
                let (Some(range), Some(app)) = (
                    pair(plan.get("range")),
                    pair(plan.get("pool").and_then(|p| p.get("app"))),
                ) else {
                    continue;
                };
                let dir_str = plan
                    .get("direction")
                    .and_then(toml::Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if !key.is_empty() {
                    out.insert(key, (dir_str, range, app));
                }
            }
        };
        let root = repo_root();
        visit(&root.join("packages/boards"), "nros-board.toml", true);
        visit(&root.join("packages/platform"), "nros-platform.toml", false);
        out
    }

    /// The projection in [`PriorityPlan::for_target`] says what the
    /// descriptors say. Without this the table above would be the second
    /// spelling of a band, which is the failure the plan tables exist to
    /// prevent.
    #[test]
    fn priority_plan_projection_matches_the_descriptors() {
        let found = descriptor_plans();
        assert!(
            found.len() >= 3,
            "the tree states static plans for at least freertos, threadx, nuttx \
             and posix; found {:?}",
            found.keys().collect::<Vec<_>>()
        );
        for (key, (direction, range, app)) in &found {
            let plan = PriorityPlan::for_target(key);
            let want_dir = match plan.direction {
                Direction::SmallerIsUrgent => "smaller-is-urgent",
                Direction::BiggerIsUrgent => "bigger-is-urgent",
            };
            assert_eq!(direction, want_dir, "{key}: direction");
            assert_eq!(&plan.range, range, "{key}: range");
            assert_eq!(&plan.app, app, "{key}: pool.app");
        }
    }
}
