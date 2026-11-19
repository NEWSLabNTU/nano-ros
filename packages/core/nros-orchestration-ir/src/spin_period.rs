//! The priority-allocation table, the `spin_period_us` row: a sched task's spin period is
//! DERIVED from "the sched task's fastest release", not authored.
//!
//! Before this module nothing derived it. An unset `spin_period` reached the
//! C/C++ sched task runners as `0`, which they floor at 1 ms, so a sched task of 10 Hz and
//! 30 Hz nodes woke a thousand times a second unless its author wrote a number
//! by hand - the "mechanism, not intent" field the RFC measured on 11 of 11
//! sched tasks.
//!
//! The rule, stated once:
//!
//! * **derived** = the smallest release period among the sched task's members (a
//!   member's period is the realizer's: its fastest timer trigger, see
//!   [`crate::rtos_realizer::node_periods_us`]), and the sched task's own authored
//!   `period` when it has one. A sched task whose members are all event-driven has
//!   no release to derive from and keeps what it had.
//! * **unset** -> the derived value.
//! * **authored, no slower than derived** -> kept, as the override the RFC
//!   allows, and reported (`SpinPeriodNote::Override`) when it differs.
//! * **authored, slower than derived** -> refused: the sched task would park past
//!   its fastest member's release, on every backend whose park is not bounded
//!   by the executor's own timer deadline.

use std::collections::BTreeMap;

use crate::ResolvedSchedTaskTable;

/// What [`apply_derived_spin_periods`] did to one sched task.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpinPeriodNote {
    /// The sched task had no spin period; it now has the derived one.
    Derived {
        sched_task: String,
        spin_period_us: u64,
        /// The member whose release decided it (`None`: the sched task's own period).
        from: Option<String>,
    },
    /// An authored spin period faster than the derived one, kept.
    Override {
        sched_task: String,
        authored_us: u64,
        derived_us: u64,
    },
}

impl std::fmt::Display for SpinPeriodNote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Derived {
                sched_task,
                spin_period_us,
                from,
            } => {
                write!(
                    f,
                    "sched task `{sched_task}` spin period {spin_period_us} us, derived from {}",
                    from.as_deref()
                        .map(|n| format!("member `{n}`'s release period"))
                        .unwrap_or_else(|| "the sched task's own period".to_string())
                )
            }
            Self::Override {
                sched_task,
                authored_us,
                derived_us,
            } => write!(
                f,
                // RFC-0079
                "sched task `{sched_task}` authors spin_period {authored_us} us; the derived value is \
                 {derived_us} us (its fastest release). The authored value is kept as an \
                 override; remove it to have the period derived."
            ),
        }
    }
}

/// An authored spin period slower than the sched task's fastest release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpinPeriodTooSlow {
    pub sched_task: String,
    pub authored_us: u64,
    pub derived_us: u64,
    pub from: Option<String>,
}

impl std::fmt::Display for SpinPeriodTooSlow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            // RFC-0079
            "sched task `{}` authors spin_period {} us, slower than its fastest release ({} us, {}): \
             that release would run at the spin rate instead. Remove `spin_period` from \
             `[tasks.{}]` in system.toml to have it derived, or state one no slower than {} us.",
            self.sched_task,
            self.authored_us,
            self.derived_us,
            self.from
                .as_deref()
                .map(|n| format!("member `{n}`'s timer"))
                .unwrap_or_else(|| "the sched task's own period".to_string()),
            self.sched_task,
            self.derived_us
        )
    }
}

impl std::error::Error for SpinPeriodTooSlow {}

fn bare(name: &str) -> &str {
    name.rsplit('/').next().unwrap_or(name)
}

/// The derived spin period of one sched task: `(period_us, deciding member)`.
///
/// `periods` is keyed by node name, fully qualified or bare (both are read
/// bare, as the sched task's members are).
pub fn derive_spin_period_us(
    members: &[(String, String)],
    sched_task_period_us: Option<u64>,
    periods: &BTreeMap<String, u64>,
) -> Option<(u64, Option<String>)> {
    let bare_periods: BTreeMap<&str, u64> = periods
        .iter()
        .filter(|(_, p)| **p > 0)
        .map(|(n, p)| (bare(n), *p))
        .collect();
    let mut best: Option<(u64, Option<String>)> =
        sched_task_period_us.filter(|p| *p > 0).map(|p| (p, None));
    for (node, _group) in members {
        if let Some(&p) = bare_periods.get(bare(node))
            && best.as_ref().is_none_or(|(b, _)| p < *b)
        {
            best = Some((p, Some(bare(node).to_string())));
        }
    }
    best
}

/// Fill every sched task's unset spin period from its members' releases, and hold
/// an authored one to them. See the module docs for the rule.
pub fn apply_derived_spin_periods(
    table: &mut ResolvedSchedTaskTable,
    periods: &BTreeMap<String, u64>,
) -> Result<Vec<SpinPeriodNote>, SpinPeriodTooSlow> {
    let mut notes = Vec::new();
    for sched_task in &mut table.sched_tasks {
        let Some((derived, from)) =
            derive_spin_period_us(&sched_task.members, sched_task.period_us, periods)
        else {
            continue;
        };
        match sched_task.spin_period_us {
            None | Some(0) => {
                sched_task.spin_period_us = Some(derived);
                notes.push(SpinPeriodNote::Derived {
                    sched_task: sched_task.name.clone(),
                    spin_period_us: derived,
                    from,
                });
            }
            Some(a) if a > derived => {
                return Err(SpinPeriodTooSlow {
                    sched_task: sched_task.name.clone(),
                    authored_us: a,
                    derived_us: derived,
                    from,
                });
            }
            Some(a) if a < derived => notes.push(SpinPeriodNote::Override {
                sched_task: sched_task.name.clone(),
                authored_us: a,
                derived_us: derived,
            }),
            Some(_) => {}
        }
    }
    Ok(notes)
}

/// [`apply_derived_spin_periods`] with the periods read from `model`: the
/// realizer's per-node release periods ([`crate::rtos_realizer::node_periods_us`]
/// over [`crate::mapper_input::mapper_input_from_model`]), each node's fastest
/// timer trigger.
///
/// The one entry both entry roads call -- `nros` codegen's C/C++ entries and
/// `nros::main!` -- so a sched task's spin period is derived, and an authored
/// one slower than its tightest member timer refused, whichever road bakes it.
pub fn apply_model_spin_periods(
    model: &ros_launch_manifest_model::SystemModel,
    table: &mut ResolvedSchedTaskTable,
) -> Result<Vec<SpinPeriodNote>, SpinPeriodTooSlow> {
    let input = crate::mapper_input::mapper_input_from_model(model);
    let periods = crate::rtos_realizer::node_periods_us(&input);
    apply_derived_spin_periods(table, &periods)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ResolvedSchedTask;

    fn sched_task(name: &str, spin: Option<u64>, members: &[&str]) -> ResolvedSchedTask {
        ResolvedSchedTask {
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

    /// The safety island's sched task: a 10 Hz handler, a 30 Hz and a 10 Hz operator.
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
        let mut t = ResolvedSchedTaskTable {
            sched_tasks: vec![sched_task("mrm", None, &ISLAND)],
        };
        let notes = apply_derived_spin_periods(&mut t, &island_periods()).unwrap();
        assert_eq!(t.sched_tasks[0].spin_period_us, Some(33_333));
        assert_eq!(
            notes,
            vec![SpinPeriodNote::Derived {
                sched_task: "mrm".into(),
                spin_period_us: 33_333,
                from: Some("mrm_emergency_stop_operator".into()),
            }]
        );
    }

    #[test]
    fn a_faster_authored_spin_period_is_kept_and_reported() {
        let mut t = ResolvedSchedTaskTable {
            sched_tasks: vec![sched_task("mrm", Some(10_000), &ISLAND)],
        };
        let notes = apply_derived_spin_periods(&mut t, &island_periods()).unwrap();
        assert_eq!(t.sched_tasks[0].spin_period_us, Some(10_000));
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
        let mut t = ResolvedSchedTaskTable {
            sched_tasks: vec![sched_task("mrm", Some(33_333), &ISLAND)],
        };
        assert!(
            apply_derived_spin_periods(&mut t, &island_periods())
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn a_slower_authored_spin_period_is_refused() {
        let mut t = ResolvedSchedTaskTable {
            sched_tasks: vec![sched_task("mrm", Some(50_000), &ISLAND)],
        };
        let e = apply_derived_spin_periods(&mut t, &island_periods()).unwrap_err();
        assert_eq!(e.derived_us, 33_333);
        assert_eq!(e.from.as_deref(), Some("mrm_emergency_stop_operator"));
        assert!(e.to_string().contains("slower than its fastest release"));
    }

    #[test]
    fn a_sched_task_of_event_driven_members_keeps_what_it_had() {
        let mut t = ResolvedSchedTaskTable {
            sched_tasks: vec![
                sched_task("ev", None, &["listener"]),
                sched_task("ev2", Some(7), &["x"]),
            ],
        };
        assert!(
            apply_derived_spin_periods(&mut t, &island_periods())
                .unwrap()
                .is_empty()
        );
        assert_eq!(t.sched_tasks[0].spin_period_us, None);
        assert_eq!(t.sched_tasks[1].spin_period_us, Some(7));
    }

    #[test]
    fn the_sched_tasks_own_period_is_a_release_too() {
        let mut t = ResolvedSchedTaskTable {
            sched_tasks: vec![sched_task("ev", None, &["listener"])],
        };
        t.sched_tasks[0].period_us = Some(20_000);
        apply_derived_spin_periods(&mut t, &island_periods()).unwrap();
        assert_eq!(t.sched_tasks[0].spin_period_us, Some(20_000));
    }

    #[test]
    fn each_sched_task_derives_from_its_own_members() {
        let mut t = ResolvedSchedTaskTable {
            sched_tasks: vec![
                sched_task("estop", None, &["mrm_emergency_stop_operator"]),
                sched_task(
                    "mrm",
                    None,
                    &["mrm_handler", "mrm_comfortable_stop_operator"],
                ),
            ],
        };
        apply_derived_spin_periods(&mut t, &island_periods()).unwrap();
        assert_eq!(t.sched_tasks[0].spin_period_us, Some(33_333));
        assert_eq!(t.sched_tasks[1].spin_period_us, Some(100_000));
    }

    /// One node, `/ctl`, whose path is released by a 30 Hz timer.
    fn timer_model() -> ros_launch_manifest_model::SystemModel {
        let yaml = "meta:\n  version: 1\n\
             structure:\n  scopes:\n    d.launch.xml:\n      file: d.launch.xml\n  \
             nodes:\n    /ctl:\n      scope: d.launch.xml\n      pkg: d\n      exec: ctl\n      \
             node_name: ctl\n      namespace: /\n  \
             topics:\n    /cmd:\n      type: std_msgs/msg/Empty\n      pub: [/ctl/cmd]\n\
             contracts:\n  node_paths:\n    /ctl/loop:\n      output: [/ctl/cmd]\n      \
             trigger:\n        kind: timer\n        value:\n          rate_hz: 30.0\n";
        ros_launch_manifest_model::SystemModel::from_yaml_str(yaml).expect("fixture parses")
    }

    /// The model entry both roads call reads the release from the model's
    /// timer trigger: an unset spin period becomes the timer's period.
    #[test]
    fn the_model_entry_derives_from_the_member_timer() {
        let mut t = ResolvedSchedTaskTable {
            sched_tasks: vec![sched_task("app", None, &["ctl"])],
        };
        let notes = apply_model_spin_periods(&timer_model(), &mut t).unwrap();
        assert_eq!(t.sched_tasks[0].spin_period_us, Some(33_333));
        assert!(
            matches!(&notes[..], [SpinPeriodNote::Derived { from: Some(n), .. }] if n == "ctl"),
            "{notes:?}"
        );
    }

    /// A spin period longer than the member timer's period is refused through
    /// the model entry, naming the sched task, both periods and the member.
    #[test]
    fn the_model_entry_refuses_a_spin_period_longer_than_the_member_timer() {
        let mut t = ResolvedSchedTaskTable {
            sched_tasks: vec![sched_task("app", Some(50_000), &["ctl"])],
        };
        let err = apply_model_spin_periods(&timer_model(), &mut t).unwrap_err();
        let msg = err.to_string();
        for want in [
            "`app`",
            "50000 us",
            "33333 us",
            "member `ctl`'s timer",
            "`[tasks.app]` in system.toml",
        ] {
            assert!(msg.contains(want), "{want} missing: {msg}");
        }
    }
}
