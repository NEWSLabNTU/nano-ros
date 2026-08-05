//! phase-329 W7 — the test-bucket → CI-tier map (RFC-0061 join).
//!
//! Every test file belongs to exactly ONE bucket (phase-329 "Target structure"
//! table). RFC-0061 assigns each bucket a CI-TIER HOME — the ladder rung(s) it
//! runs on — but until now that assignment lived only in lane-filter scripts and
//! justfile comments, i.e. tribal knowledge. This module is the SSoT: [`BUCKET_TIERS`]
//! declares, per bucket, which tiers exercise it and why. The gate ties it to the
//! real machinery — [`crate::ci_lane`] computes the cell-matrix lanes, and the
//! tests here assert the declaration agrees with that computation, so the two
//! cannot drift.
//!
//! The cost ladder itself (which cells each lane picks, and why 1-wise vs pairwise)
//! lives in [`crate::ci_lane`]; this module is only the coarse bucket→tier map.

use crate::ci_lane::CiLane;

/// The CI ladder (RFC-0061). `Tier1`/`Tier2`/`Tier2Nightly` have COMPUTED cell
/// selections in [`crate::ci_lane`]; `Tier3` is "everything", so it needs no
/// selection.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CiTier {
    /// `just ci` — host only, minutes, every commit / pre-push.
    Tier1,
    /// `just ci-matrix` — 1-wise over platform×lang×rmw×kind, per-change gate.
    Tier2,
    /// `just ci-matrix-nightly` — the pairwise cover + full interop/realtime-dim.
    Tier2Nightly,
    /// `just ci-full` — the whole matrix + the docker edition axis; pre-release.
    Tier3,
}

impl CiTier {
    /// The `CiTier` a computed [`CiLane`] corresponds to (Tier3 has no lane — it
    /// runs everything).
    pub fn of_lane(l: CiLane) -> CiTier {
        match l {
            CiLane::Tier1 => CiTier::Tier1,
            CiLane::Tier2 => CiTier::Tier2,
            CiLane::Tier2Nightly => CiTier::Tier2Nightly,
        }
    }
}

/// A test bucket — one row of the phase-329 "Target structure" table. Every test
/// file is exactly one of these.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Bucket {
    /// `matrix::CELLS` — platform × lang × rmw × workload × kind, boots fixtures.
    CellMatrix,
    /// `interop::CELLS` — nano cell + live ROS 2 peer.
    Interop,
    /// `ros_editions_e2e` — RFC-0058 edition × rmw × workload × dir, docker peer.
    Editions,
    /// `matrix::SCHED_CELLS` — the realtime-dim matrix, boots ws-realtime fixtures.
    RealtimeDim,
    /// CLI-behavior suite — TempDir staging, host only.
    CliBehavior,
    /// Fixture-artifact checks — `fixtures.toml` rows, no boot.
    FixtureArtifact,
    /// Guards / gates — host only (incl. this module's own gate).
    Guard,
    /// Host-unit tests — host only.
    HostUnit,
    /// Negative-diagnostic registry (phase-329 W5) — sanctioned FAIL-path
    /// diagnostics, fast, host.
    NegativeDiagnostic,
}

impl Bucket {
    pub const ALL: &'static [Bucket] = &[
        Bucket::CellMatrix,
        Bucket::Interop,
        Bucket::Editions,
        Bucket::RealtimeDim,
        Bucket::CliBehavior,
        Bucket::FixtureArtifact,
        Bucket::Guard,
        Bucket::HostUnit,
        Bucket::NegativeDiagnostic,
    ];
}

/// A bucket, the tiers that exercise it (ascending; the LAST is where the bucket
/// is FULLY covered), and the rationale.
pub struct BucketTier {
    pub bucket: Bucket,
    pub tiers: &'static [CiTier],
    pub reason: &'static str,
}

/// The SSoT map. Editing which tier a bucket runs on happens HERE, and the gate
/// keeps it honest against `ci_lane`.
pub const BUCKET_TIERS: &[BucketTier] = &[
    BucketTier {
        bucket: Bucket::CellMatrix,
        tiers: &[
            CiTier::Tier1,
            CiTier::Tier2,
            CiTier::Tier2Nightly,
            CiTier::Tier3,
        ],
        reason: "native subset @ tier1, 1-wise @ tier2, pairwise @ nightly, FULL @ tier3 \
                 — the three computed lanes live in ci_lane; tier3 runs every cell",
    },
    BucketTier {
        bucket: Bucket::Interop,
        tiers: &[CiTier::Tier1, CiTier::Tier2Nightly, CiTier::Tier3],
        reason: "interop/bridge cells are in the ci_lane pool (Kind axis), so the 1-wise \
                 subset rides tier1; FULL live-peer interop is a nightly cost, all @ tier3",
    },
    BucketTier {
        bucket: Bucket::RealtimeDim,
        tiers: &[CiTier::Tier2Nightly, CiTier::Tier3],
        reason: "SCHED_CELLS boots ws-realtime QEMU fixtures — too heavy for the per-change \
                 gate; FULL dim honoring runs nightly, and again in the tier3 sweep",
    },
    BucketTier {
        bucket: Bucket::Editions,
        tiers: &[CiTier::Tier3],
        reason: "the RFC-0058 docker edition axis (jazzy/iron/…) — pre-release only",
    },
    BucketTier {
        bucket: Bucket::CliBehavior,
        tiers: &[CiTier::Tier1],
        reason: "TempDir staging, no fixture/boot — host only, every commit",
    },
    BucketTier {
        bucket: Bucket::FixtureArtifact,
        tiers: &[CiTier::Tier1],
        reason: "asserts fixtures.toml artifacts exist/shape; no boot — host only",
    },
    BucketTier {
        bucket: Bucket::Guard,
        tiers: &[CiTier::Tier1],
        reason: "gates over source/tables (this module, matrix_fixture_coverage, the \
                 negative-diagnostic + axis-table gates) — host only, fast",
    },
    BucketTier {
        bucket: Bucket::HostUnit,
        tiers: &[CiTier::Tier1],
        reason: "pure host unit tests — no fixture, no toolchain",
    },
    BucketTier {
        bucket: Bucket::NegativeDiagnostic,
        tiers: &[CiTier::Tier1],
        reason: "sanctioned FAIL-path diagnostics (phase-329 W5) — a configure/compile that \
                 must fail; fast, host only",
    },
];

/// The declared tier participation of a bucket.
pub fn tiers_of(b: Bucket) -> &'static [CiTier] {
    BUCKET_TIERS
        .iter()
        .find(|e| e.bucket == b)
        .map(|e| e.tiers)
        .unwrap_or(&[])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    /// Every bucket is mapped exactly once, with a non-empty ascending tier list
    /// and a reason. A new bucket with no row fails here rather than silently
    /// running in no lane.
    #[test]
    fn every_bucket_mapped_once() {
        let mut seen = BTreeSet::new();
        for b in Bucket::ALL {
            let rows: Vec<_> = BUCKET_TIERS.iter().filter(|e| e.bucket == *b).collect();
            assert_eq!(
                rows.len(),
                1,
                "bucket {b:?} must have exactly one row, got {}",
                rows.len()
            );
            assert!(seen.insert(format!("{b:?}")), "duplicate {b:?}");
            let e = rows[0];
            assert!(!e.tiers.is_empty(), "bucket {b:?} runs in no tier");
            assert!(!e.reason.is_empty(), "bucket {b:?} has no reason");
            // ascending + de-duplicated
            let mut sorted = e.tiers.to_vec();
            sorted.sort();
            sorted.dedup();
            assert_eq!(
                sorted,
                e.tiers.to_vec(),
                "bucket {b:?} tiers must be ascending + unique"
            );
        }
        // BUCKET_TIERS has no row for a bucket not in ALL.
        assert_eq!(
            BUCKET_TIERS.len(),
            Bucket::ALL.len(),
            "BUCKET_TIERS has a row for a bucket missing from Bucket::ALL"
        );
    }

    /// The declaration must agree with the real computed lanes: every tier
    /// `ci_lane` computes a selection for is a tier the CellMatrix bucket declares
    /// it runs in. If someone adds a computed lane (a new rung) without declaring
    /// the cell matrix runs there, this fails.
    #[test]
    fn cell_matrix_covers_every_computed_lane() {
        let declared: BTreeSet<CiTier> = tiers_of(Bucket::CellMatrix).iter().copied().collect();
        for lane in crate::ci_lane::ALL {
            let t = CiTier::of_lane(lane);
            assert!(
                declared.contains(&t),
                "ci_lane computes a {lane:?} ({t:?}) selection but the CellMatrix bucket \
                 does not declare it runs at {t:?} — update BUCKET_TIERS"
            );
        }
        // The cell matrix is fully covered only in the tier3 sweep.
        assert!(
            declared.contains(&CiTier::Tier3),
            "the CellMatrix is only FULLY covered at tier3 — it must declare Tier3"
        );
    }

    /// The host-only buckets must all be Tier1 (RFC-0061: guards/gates + host-unit
    /// + CLI-behavior + fixture-artifact + negative-diagnostic run on every commit).
    #[test]
    fn host_only_buckets_are_tier1() {
        for b in [
            Bucket::CliBehavior,
            Bucket::FixtureArtifact,
            Bucket::Guard,
            Bucket::HostUnit,
            Bucket::NegativeDiagnostic,
        ] {
            assert_eq!(
                tiers_of(b),
                &[CiTier::Tier1],
                "host-only bucket {b:?} must run at tier1 only"
            );
        }
    }
}
