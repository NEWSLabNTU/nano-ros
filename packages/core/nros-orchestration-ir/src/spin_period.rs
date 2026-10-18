//! RFC-0079's table, the `spin_period_us` row: a tier's spin period is
//! DERIVED from "the tier's fastest release", not authored.
//!
//! Before this module nothing derived it. An unset `spin_period` reached the
//! C/C++ tier runners as `0`, which they floor at 1 ms, so a tier of 10 Hz and
//! 30 Hz nodes woke a thousand times a second unless its author wrote a number
//! by hand - the "mechanism, not intent" field the RFC measured on 11 of 11
//! tiers.
//!
//! The rule, stated once:
//!
//! * **derived** = the smallest release period among the tier's members (a
//!   member's period is the realizer's: its fastest timer trigger, see
//!   [`crate::rtos_realizer::node_periods_us`]), and the tier's own authored
//!   `period` when it has one. A tier whose members are all event-driven has
//!   no release to derive from and keeps what it had.
//! * **unset** -> the derived value.
//! * **authored, no slower than derived** -> kept, as the override the RFC
//!   allows, and reported (`SpinPeriodNote::Override`) when it differs.
//! * **authored, slower than derived** -> refused: the tier would park past
//!   its fastest member's release, on every backend whose park is not bounded
//!   by the executor's own timer deadline.

use std::collections::BTreeMap;

use crate::ResolvedTierTable;

/// What [`apply_derived_spin_periods`] did to one tier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpinPeriodNote {
    /// The tier had no spin period; it now has the derived one.
    Derived {
        tier: String,
        spin_period_us: u64,
        /// The member whose release decided it (`None`: the tier's own period).
        from: Option<String>,
    },
    /// An authored spin period faster than the derived one, kept.
    Override {
        tier: String,
        authored_us: u64,
        derived_us: u64,
    },
}

impl std::fmt::Display for SpinPeriodNote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Derived {
                tier,
                spin_period_us,
                from,
            } => {
                write!(
                    f,
                    "tier `{tier}` spin period {spin_period_us} us, derived from {}",
                    from.as_deref()
                        .map(|n| format!("member `{n}`'s release period"))
                        .unwrap_or_else(|| "the tier's own period".to_string())
                )
            }
            Self::Override {
                tier,
                authored_us,
                derived_us,
            } => write!(
                f,
                "tier `{tier}` authors spin_period {authored_us} us; the derived value is \
                 {derived_us} us (its fastest release). The authored value is kept as an \
                 override; remove it to have the period derived (RFC-0079)."
            ),
        }
    }
}

/// An authored spin period slower than the tier's fastest release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpinPeriodTooSlow {
    pub tier: String,
    pub authored_us: u64,
    pub derived_us: u64,
    pub from: Option<String>,
}

impl std::fmt::Display for SpinPeriodTooSlow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "tier `{}` authors spin_period {} us, slower than its fastest release ({} us, {}). \
             Remove `spin_period` to have it derived, or state one no slower than {} us \
             (RFC-0079).",
            self.tier,
            self.authored_us,
            self.derived_us,
            self.from
                .as_deref()
                .map(|n| format!("member `{n}`"))
                .unwrap_or_else(|| "the tier's own period".to_string()),
            self.derived_us
        )
    }
}

impl std::error::Error for SpinPeriodTooSlow {}

fn bare(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

/// The derived spin period of one tier: `(period_us, deciding member)`.
///
/// `periods` is keyed by node name, fully qualified or bare (both are read
/// bare, as the tier's members are).
pub fn derive_spin_period_us(
    members: &[(String, String)],
    tier_period_us: Option<u64>,
    periods: &BTreeMap<String, u64>,
) -> Option<(u64, Option<String>)> {
    let bare_periods: BTreeMap<&str, u64> = periods
        .iter()
        .filter(|(_, p)| **p > 0)
        .map(|(n, p)| (bare(n), *p))
        .collect();
    let mut best: Option<(u64, Option<String>)> =
        tier_period_us.filter(|p| *p > 0).map(|p| (p, None));
    for (node, _group) in members {
        if let Some(&p) = bare_periods.get(bare(node))
            && best.as_ref().is_none_or(|(b, _)| p < *b)
        {
            best = Some((p, Some(bare(node).to_string())));
        }
    }
    best
}

/// Fill every tier's unset spin period from its members' releases, and hold
/// an authored one to them. See the module docs for the rule.
pub fn apply_derived_spin_periods(
    table: &mut ResolvedTierTable,
    periods: &BTreeMap<String, u64>,
) -> Result<Vec<SpinPeriodNote>, SpinPeriodTooSlow> {
    let mut notes = Vec::new();
    for tier in &mut table.tiers {
        let Some((derived, from)) = derive_spin_period_us(&tier.members, tier.period_us, periods)
        else {
            continue;
        };
        match tier.spin_period_us {
            None | Some(0) => {
                tier.spin_period_us = Some(derived);
                notes.push(SpinPeriodNote::Derived {
                    tier: tier.name.clone(),
                    spin_period_us: derived,
                    from,
                });
            }
            Some(a) if a > derived => {
                return Err(SpinPeriodTooSlow {
                    tier: tier.name.clone(),
                    authored_us: a,
                    derived_us: derived,
                    from,
                });
            }
            Some(a) if a < derived => notes.push(SpinPeriodNote::Override {
                tier: tier.name.clone(),
                authored_us: a,
                derived_us: derived,
            }),
            Some(_) => {}
        }
    }
    Ok(notes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ResolvedTier;

    fn tier(name: &str, spin: Option<u64>, members: &[&str]) -> ResolvedTier {
        ResolvedTier {
            name: name.into(),
            priority: 3,
            stack_bytes: None,
            spin_period_us: spin,
            preempt_threshold: None,
            time_slice_us: None,
            sched_class: None,
            class: None,
            period_us: None,
            budget_us: None,
            deadline_us: None,
            deadline_policy: None,
            core: None,
            members: members
                .iter()
                .map(|m| (m.to_string(), "main".to_string()))
                .collect(),
        }
    }

    /// The safety island's tier: a 10 Hz handler, a 30 Hz and a 10 Hz operator.
    fn island_periods() -> BTreeMap<String, u64> {
        [
            ("/mrm_handler", 100_000),
            ("/mrm_emergency_stop_operator", 33_333),
            ("/mrm_comfortable_stop_operator", 100_000),
        ]
        .into_iter()
        .map(|(n, p)| (n.to_string(), p))
        .collect()
    }

    const ISLAND: [&str; 3] = [
        "mrm_handler",
        "mrm_emergency_stop_operator",
        "mrm_comfortable_stop_operator",
    ];

    #[test]
    fn an_unset_spin_period_is_the_fastest_members_release() {
        let mut t = ResolvedTierTable {
            tiers: vec![tier("mrm", None, &ISLAND)],
        };
        let notes = apply_derived_spin_periods(&mut t, &island_periods()).unwrap();
        assert_eq!(t.tiers[0].spin_period_us, Some(33_333));
        assert_eq!(
            notes,
            vec![SpinPeriodNote::Derived {
                tier: "mrm".into(),
                spin_period_us: 33_333,
                from: Some("mrm_emergency_stop_operator".into()),
            }]
        );
    }

    #[test]
    fn a_faster_authored_spin_period_is_kept_and_reported() {
        let mut t = ResolvedTierTable {
            tiers: vec![tier("mrm", Some(10_000), &ISLAND)],
        };
        let notes = apply_derived_spin_periods(&mut t, &island_periods()).unwrap();
        assert_eq!(t.tiers[0].spin_period_us, Some(10_000));
        assert!(matches!(
            &notes[..],
            [SpinPeriodNote::Override {
                authored_us: 10_000,
                derived_us: 33_333,
                ..
            }]
        ));
    }

    #[test]
    fn an_authored_spin_period_equal_to_the_derived_one_is_silent() {
        let mut t = ResolvedTierTable {
            tiers: vec![tier("mrm", Some(33_333), &ISLAND)],
        };
        assert!(
            apply_derived_spin_periods(&mut t, &island_periods())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_slower_authored_spin_period_is_refused() {
        let mut t = ResolvedTierTable {
            tiers: vec![tier("mrm", Some(50_000), &ISLAND)],
        };
        let e = apply_derived_spin_periods(&mut t, &island_periods()).unwrap_err();
        assert_eq!(e.derived_us, 33_333);
        assert_eq!(e.from.as_deref(), Some("mrm_emergency_stop_operator"));
        assert!(e.to_string().contains("slower than its fastest release"));
    }

    #[test]
    fn a_tier_of_event_driven_members_keeps_what_it_had() {
        let mut t = ResolvedTierTable {
            tiers: vec![
                tier("ev", None, &["listener"]),
                tier("ev2", Some(7), &["x"]),
            ],
        };
        assert!(
            apply_derived_spin_periods(&mut t, &island_periods())
                .unwrap()
                .is_empty()
        );
        assert_eq!(t.tiers[0].spin_period_us, None);
        assert_eq!(t.tiers[1].spin_period_us, Some(7));
    }

    #[test]
    fn the_tiers_own_period_is_a_release_too() {
        let mut t = ResolvedTierTable {
            tiers: vec![tier("ev", None, &["listener"])],
        };
        t.tiers[0].period_us = Some(20_000);
        apply_derived_spin_periods(&mut t, &island_periods()).unwrap();
        assert_eq!(t.tiers[0].spin_period_us, Some(20_000));
    }

    #[test]
    fn each_tier_derives_from_its_own_members() {
        let mut t = ResolvedTierTable {
            tiers: vec![
                tier("estop", None, &["mrm_emergency_stop_operator"]),
                tier(
                    "mrm",
                    None,
                    &["mrm_handler", "mrm_comfortable_stop_operator"],
                ),
            ],
        };
        apply_derived_spin_periods(&mut t, &island_periods()).unwrap();
        assert_eq!(t.tiers[0].spin_period_us, Some(33_333));
        assert_eq!(t.tiers[1].spin_period_us, Some(100_000));
    }
}
