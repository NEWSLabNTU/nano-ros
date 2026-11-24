//! Issue 2640 (phase-566) -- the launch values a metadata probe
//! constructs a component with.
//!
//! The probe runs a component's constructor on the host to count what it
//! registers. An rclcpp-shaped node that declares a parameter WITHOUT a default
//! takes its value from the launch (`<param>`, `params_files`), and a probe
//! that hands it none runs the constructor on `T()`: a period of 0, a NaN, a
//! failed probe. A firmware entry seeds every launch value before it builds the
//! node; so does the probe now, from the SAME place:
//!
//! - **D1, the source.** The models the sync just resolved, through
//!   `model_gate::verify` (a refused or stale model is not a seed source), each
//!   node projected by `launch_param_text::baked_params` -- the projection the
//!   firmware entry, `nros::main!` and the planner's record read. One resolve,
//!   one projection: a seed and a baked value cannot disagree.
//! - **D2, which instance.** No instance: no seeds, the probe runs as before.
//!   One instance, or several resolving to EQUAL values: those values. Several
//!   that differ on some keys (absent in one counts): the probe is seeded with
//!   every key they agree on, and only the keys they differ on stay on their
//!   declaration defaults (issue 2901, the user's ruling of 2026-11-25). Its
//!   outcome decides -- kept with a note, or refused naming only the
//!   disagreeing keys.
//! - **D4, freshness.** The seed set's `sha256` digest is a field of the
//!   component's probe key, so editing a parameter file re-probes, and a reader
//!   that recomputes the key (the sizing join) skips a sidecar probed under
//!   other values.
//!
//! `qos_overrides.*` keys are not seeds: they are a QoS table, not declared
//! parameters, and the census records the QoS the code asks for.
//!
//! The Rust cargo harness takes the same seeds (phase-566 W4): one rule for
//! every probe, whichever language built the component.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use sha2::Digest as _;

/// One model a seed was read from: where, and its content digest.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SeedSource {
    pub path: String,
    pub sha256: String,
}

/// One node instance of a component in a resolved model.
#[derive(Clone, Debug)]
struct Instance {
    pkg: String,
    exec: Option<String>,
    plugin: Option<String>,
    fqn: String,
    /// Index into [`SeedModels::sources`].
    source: usize,
    /// The node's launch values, `qos_overrides.*` removed, sorted by name.
    params: Vec<(String, String)>,
}

/// `(pkg, exec, fqn, params)` of one instance, for [`SeedModels::for_test`].
#[cfg(test)]
pub(crate) type TestInstance<'a> = (&'a str, &'a str, &'a str, &'a [(&'a str, &'a str)]);

/// Every node instance of every model the sync resolved, read once per pass.
#[derive(Clone, Debug, Default)]
pub struct SeedModels {
    sources: Vec<SeedSource>,
    instances: Vec<Instance>,
}

/// One instance of a disagreeing set: its FQN, the model it came from, and
/// the launch values it resolves to.
pub type DisagreeingInstance = (String, String, Vec<(String, String)>);

/// What the probe of one component is seeded with (D2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Seeding {
    /// No launch instance names the component.
    None,
    /// Every instance resolves to these values.
    Seeded {
        instances: Vec<String>,
        params: Vec<(String, String)>,
        sources: Vec<SeedSource>,
        digest: String,
    },
    /// The instances resolve to different values.
    Disagree {
        /// `(fqn, model path, values)` per instance.
        instances: Vec<DisagreeingInstance>,
        /// The keys on which they differ.
        keys: Vec<String>,
        /// Issue 2901 -- every other key, which each instance sets to the same
        /// value: the probe is seeded with these.
        agreed: Vec<(String, String)>,
        sources: Vec<SeedSource>,
        digest: String,
    },
}

/// The `probe` object a sidecar records (D4): what the probe was seeded with,
/// or why it ran unseeded although the component has launch instances.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "snake_case")]
pub enum ProbeSeedRecord {
    Seeds {
        instances: Vec<String>,
        sources: Vec<SeedSource>,
        digest: String,
    },
    Unseeded {
        reason: String,
        instances: Vec<String>,
        keys: Vec<String>,
        sources: Vec<SeedSource>,
        digest: String,
    },
    /// Issue 2901 -- instances that disagree on `keys`: the probe was seeded
    /// with the keys they agree on (`seeded`, names only), and `keys` stayed on
    /// their declaration defaults.
    PartlySeeded {
        instances: Vec<String>,
        seeded: Vec<String>,
        keys: Vec<String>,
        sources: Vec<SeedSource>,
        digest: String,
    },
}

/// The directory the sync writes models into and every reader reads them
/// from: `explicit` (`nros sync --model-dir`), else `NROS_MODEL_DIR`, else
/// `<ws>/build/nros/models` -- the sync's own order.
pub fn models_dir(ws_root: &Path, explicit: Option<&Path>) -> PathBuf {
    explicit
        .map(Path::to_path_buf)
        .or_else(|| {
            std::env::var_os("NROS_MODEL_DIR")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        })
        .unwrap_or_else(|| ws_root.join("build").join("nros").join("models"))
}

impl SeedModels {
    /// Every `<dir>/<bringup>/*_model.yaml` that `model_gate::verify` admits.
    /// A refused, stale or unreadable model contributes nothing: the sync has
    /// already failed on it, or it is a leftover of a bringup that no longer
    /// resolves it.
    pub fn load(ws_root: &Path, explicit_dir: Option<&Path>) -> Self {
        let dir = models_dir(ws_root, explicit_dir);
        let mut paths: Vec<PathBuf> = Vec::new();
        if let Ok(bringups) = std::fs::read_dir(&dir) {
            for b in bringups.flatten() {
                if let Ok(files) = std::fs::read_dir(b.path()) {
                    paths.extend(files.flatten().map(|e| e.path()).filter(|p| {
                        p.is_file()
                            && p.file_name()
                                .and_then(|n| n.to_str())
                                .is_some_and(|n| n.ends_with("_model.yaml"))
                    }));
                }
            }
        }
        paths.sort();
        let mut out = Self::default();
        for path in paths {
            if crate::model_gate::verify(&path, None).is_err() {
                continue;
            }
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let Ok(model) = crate::orchestration::model_ingest::load_model(&path, None) else {
                continue;
            };
            let shown = path
                .strip_prefix(ws_root)
                .unwrap_or(&path)
                .display()
                .to_string();
            out.sources.push(SeedSource {
                path: shown,
                sha256: format!("{:x}", sha2::Sha256::digest(&bytes)),
            });
            let source = out.sources.len() - 1;
            for (fqn, inst) in &model.structure.nodes {
                let Some(pkg) = inst.pkg.clone() else {
                    continue;
                };
                let params = nros_orchestration_ir::launch_param_text::baked_params(inst, fqn)
                    .into_iter()
                    .filter(|(k, _)| !nros_orchestration_ir::qos_override::is_qos_override(k))
                    .collect();
                out.instances.push(Instance {
                    pkg,
                    exec: nros_orchestration_ir::launch_param_text::instance_executable(inst),
                    plugin: inst.plugin.clone(),
                    fqn: fqn.clone(),
                    source,
                    params,
                });
            }
        }
        out
    }

    /// Models holding one instance per `(pkg, exec, fqn, params)`, all from
    /// one source: for a caller's test of what it does with a seeding.
    #[cfg(test)]
    pub(crate) fn for_test(instances: &[TestInstance<'_>]) -> Self {
        Self {
            sources: vec![SeedSource {
                path: "build/nros/models/b/system_model.yaml".into(),
                sha256: "00".into(),
            }],
            instances: instances
                .iter()
                .map(|(pkg, exec, fqn, params)| Instance {
                    pkg: (*pkg).into(),
                    exec: Some((*exec).into()),
                    plugin: None,
                    fqn: (*fqn).into(),
                    source: 0,
                    params: params
                        .iter()
                        .map(|(k, v)| ((*k).into(), (*v).into()))
                        .collect(),
                })
                .collect(),
        }
    }

    /// D2 for the component `package` registers as `executable` (class
    /// `class`). An instance names it by `(pkg, exec)`, the key every sidecar
    /// consumer matches on, or by its composable `plugin` naming the class.
    pub fn seeding(&self, package: &str, executable: &str, class: Option<&str>) -> Seeding {
        let mine: Vec<&Instance> = self
            .instances
            .iter()
            .filter(|i| {
                i.pkg == package
                    && (i.exec.as_deref() == Some(executable)
                        || (class.is_some() && i.plugin.as_deref() == class))
            })
            .collect();
        if mine.is_empty() {
            return Seeding::None;
        }
        let mut src_idx: Vec<usize> = mine.iter().map(|i| i.source).collect();
        src_idx.sort_unstable();
        src_idx.dedup();
        let sources: Vec<SeedSource> = src_idx.iter().map(|&i| self.sources[i].clone()).collect();
        let mut distinct: Vec<&Vec<(String, String)>> = mine.iter().map(|i| &i.params).collect();
        distinct.sort();
        distinct.dedup();
        let digest = seed_set_digest(&distinct);
        let mut instances: Vec<String> = mine.iter().map(|i| i.fqn.clone()).collect();
        instances.sort();
        instances.dedup();
        if let [one] = distinct.as_slice() {
            return Seeding::Seeded {
                instances,
                params: (*one).clone(),
                sources,
                digest,
            };
        }
        // Keys on which any two instances differ: a key absent in one is the
        // declaration's default there, which is a value too.
        let maps: Vec<BTreeMap<&str, &str>> = mine
            .iter()
            .map(|i| {
                i.params
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.as_str()))
                    .collect()
            })
            .collect();
        let mut keys: Vec<String> = maps
            .iter()
            .flat_map(|m| m.keys().copied())
            .filter(|k| {
                let first = maps[0].get(k);
                maps.iter().any(|m| m.get(k) != first)
            })
            .map(str::to_string)
            .collect();
        keys.sort();
        keys.dedup();
        // Issue 2901 -- what is left once the disagreeing keys are out: a key
        // every instance sets, to one value.
        let agreed: Vec<(String, String)> = mine[0]
            .params
            .iter()
            .filter(|(k, _)| !keys.contains(k))
            .cloned()
            .collect();
        let mut rows: Vec<DisagreeingInstance> = mine
            .iter()
            .map(|i| {
                (
                    i.fqn.clone(),
                    self.sources[i.source].path.clone(),
                    i.params.clone(),
                )
            })
            .collect();
        rows.sort();
        Seeding::Disagree {
            instances: rows,
            keys,
            agreed,
            sources,
            digest,
        }
    }
}

impl Seeding {
    /// The probe key's seed field (D4): the seed set's digest, or `None` when
    /// no instance names the component -- the key is then what it was before
    /// seeds existed, so a component nothing launches never re-probes for it.
    pub fn digest(&self) -> Option<&str> {
        match self {
            Seeding::None => None,
            Seeding::Seeded { digest, .. } | Seeding::Disagree { digest, .. } => Some(digest),
        }
    }

    /// The values the probe seeds the node with: every instance's values when
    /// they agree, the keys they agree on when they do not (issue 2901).
    pub fn params(&self) -> &[(String, String)] {
        match self {
            Seeding::Seeded { params, .. } => params,
            Seeding::Disagree { agreed, .. } => agreed,
            Seeding::None => &[],
        }
    }

    /// The sidecar's `probe` object for a probe that SUCCEEDED this way.
    pub fn record(&self) -> Option<ProbeSeedRecord> {
        match self {
            Seeding::None => None,
            Seeding::Seeded {
                instances,
                sources,
                digest,
                ..
            } => Some(ProbeSeedRecord::Seeds {
                instances: instances.clone(),
                sources: sources.clone(),
                digest: digest.clone(),
            }),
            Seeding::Disagree {
                instances,
                keys,
                agreed,
                sources,
                digest,
            } => Some(ProbeSeedRecord::PartlySeeded {
                // One FQN may be launched by several models: named once.
                instances: {
                    let mut fqns: Vec<String> =
                        instances.iter().map(|(f, _, _)| f.clone()).collect();
                    fqns.dedup();
                    fqns
                },
                seeded: agreed.iter().map(|(k, _)| k.clone()).collect(),
                keys: keys.clone(),
                sources: sources.clone(),
                digest: digest.clone(),
            }),
        }
    }

    /// For [`Seeding::Disagree`]: `<fqn> (<model>) has <k> = <v>, ...` per
    /// instance, over the keys they differ on.
    fn disagreement(&self) -> Option<String> {
        let Seeding::Disagree {
            instances, keys, ..
        } = self
        else {
            return None;
        };
        let rows: Vec<String> = instances
            .iter()
            .map(|(fqn, src, params)| {
                let vals: Vec<String> = keys
                    .iter()
                    .map(|k| {
                        let v = params
                            .iter()
                            .find(|(n, _)| n == k)
                            .map_or("its default", |(_, v)| v.as_str());
                        format!("{k} = {v}")
                    })
                    .collect();
                format!("{fqn} ({src}) has {}", vals.join(", "))
            })
            .collect();
        Some(rows.join("; "))
    }

    /// The note for a component whose instances disagree and whose probe,
    /// seeded with the keys they agree on, succeeded.
    pub fn unseeded_note(&self, node: &str) -> Option<String> {
        let rows = self.disagreement()?;
        let Seeding::Disagree { keys, agreed, .. } = self else {
            return None;
        };
        let agreed_text = match agreed.len() {
            0 => "they agree on no other key".to_string(),
            1 => "the 1 key they agree on was seeded".to_string(),
            n => format!("the {n} keys they agree on were seeded"),
        };
        Some(format!(
            "metadata probe for {node} ran with {} on its declaration default (its launch \
             instances disagree: {rows}); {agreed_text}",
            plural_keys(keys)
        ))
    }

    /// The refusal for a component whose instances disagree and whose probe,
    /// seeded with the keys they agree on, failed with `error`. It names only
    /// the keys they disagree on: those are the ones the probe left on their
    /// declaration defaults.
    pub fn refusal(&self, node: &str, error: &str) -> Option<String> {
        let rows = self.disagreement()?;
        let Seeding::Disagree { keys, .. } = self else {
            return None;
        };
        Some(format!(
            "metadata probe for {node}: its launch instances disagree on {}, so the probe leaves \
             {} on the declaration default and seeds every other key; the node cannot be \
             constructed that way ({error}). The instances: {rows}. Give each of those keys a \
             default where the node declares it, or make the instances agree.",
            plural_keys(keys),
            if keys.len() == 1 { "it" } else { "them" }
        ))
    }
}

/// `` `a` `` or `` `a`, `b` `` with "key"/"keys" before it.
fn plural_keys(keys: &[String]) -> String {
    let named: Vec<String> = keys.iter().map(|k| format!("`{k}`")).collect();
    let noun = if keys.len() == 1 { "key" } else { "keys" };
    format!("{noun} {}", named.join(", "))
}

/// `sha256` over the canonical text of the DISTINCT value maps: each map its
/// sorted `key=text` lines, the maps sorted. A change to any instance's values
/// moves it, including one that makes disagreeing instances agree.
fn seed_set_digest(distinct: &[&Vec<(String, String)>]) -> String {
    let mut h = sha2::Sha256::new();
    for map in distinct {
        for (k, v) in map.iter() {
            h.update(k.as_bytes());
            h.update(b"\0");
            h.update(v.as_bytes());
            h.update(b"\n");
        }
        h.update(b"\x1e");
    }
    format!("sha256:{:x}", h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inst(fqn: &str, exec: &str, params: &[(&str, &str)], source: usize) -> Instance {
        Instance {
            pkg: "ctrl_pkg".into(),
            exec: Some(exec.into()),
            plugin: None,
            fqn: fqn.into(),
            source,
            params: params
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    fn models(instances: Vec<Instance>) -> SeedModels {
        SeedModels {
            sources: vec![
                SeedSource {
                    path: "build/nros/models/a/system_model.yaml".into(),
                    sha256: "aa".into(),
                },
                SeedSource {
                    path: "build/nros/models/b/system_model.yaml".into(),
                    sha256: "bb".into(),
                },
            ],
            instances,
        }
    }

    #[test]
    fn a_component_no_launch_names_is_not_seeded() {
        let m = models(vec![inst("/x", "other", &[("p", "1")], 0)]);
        assert_eq!(m.seeding("ctrl_pkg", "controller", None), Seeding::None);
        assert_eq!(Seeding::None.digest(), None);
    }

    #[test]
    fn agreeing_instances_seed_their_values() {
        let m = models(vec![
            inst("/a/controller", "controller", &[("ctrl_period", "0.03")], 0),
            inst("/b/controller", "controller", &[("ctrl_period", "0.03")], 1),
        ]);
        let s = m.seeding("ctrl_pkg", "controller", None);
        assert_eq!(s.params(), &[("ctrl_period".into(), "0.03".into())]);
        let Seeding::Seeded {
            instances, sources, ..
        } = &s
        else {
            panic!("{s:?}")
        };
        assert_eq!(instances, &["/a/controller", "/b/controller"]);
        assert_eq!(sources.len(), 2);
        assert!(s.digest().unwrap().starts_with("sha256:"));
    }

    #[test]
    fn disagreeing_instances_seed_the_keys_they_agree_on() {
        let m = models(vec![
            inst(
                "/control/controller",
                "controller",
                &[("ctrl_period", "0.03"), ("gain", "1")],
                0,
            ),
            inst(
                "/carla/controller",
                "controller",
                &[("ctrl_period", "0.05")],
                1,
            ),
        ]);
        let s = m.seeding("ctrl_pkg", "controller", None);
        let Seeding::Disagree { keys, .. } = &s else {
            panic!("{s:?}")
        };
        // `gain` is absent in one instance: its default there, a difference.
        assert_eq!(keys, &["ctrl_period", "gain"]);
        // Issue 2901 -- they agree on nothing else, so nothing is seeded.
        assert!(s.params().is_empty());
        let msg = s.refusal("ctrl_pkg::controller", "boom").unwrap();
        assert!(
            msg.contains("/carla/controller (build/nros/models/b/system_model.yaml) has ctrl_period = 0.05, gain = its default"),
            "{msg}"
        );
        assert!(matches!(
            s.record(),
            Some(ProbeSeedRecord::PartlySeeded { seeded, .. }) if seeded.is_empty()
        ));
    }

    /// Issue 2901 -- the island's controller: two models that differ on three
    /// keys and agree on the rest. The agreed keys are seeded, the three stay
    /// on their declaration defaults, and only they are named.
    #[test]
    fn a_partial_disagreement_seeds_every_agreed_key_and_names_only_the_others() {
        let shared = [("ctrl_period", "0.03"), ("gain", "1.5")];
        let with = |extra: &[(&'static str, &'static str)]| {
            let mut v: Vec<(&str, &str)> = shared.to_vec();
            v.extend_from_slice(extra);
            v.sort();
            v
        };
        let m = models(vec![
            inst(
                "/control/controller",
                "controller",
                &with(&[("wheel_base", "2.74"), ("max_pitch_rad", "0.1")]),
                0,
            ),
            inst(
                "/carla/controller",
                "controller",
                &with(&[("wheel_base", "3.005"), ("max_pitch_rad", "0.2")]),
                1,
            ),
        ]);
        let s = m.seeding("ctrl_pkg", "controller", None);
        assert_eq!(
            s.params(),
            &[
                ("ctrl_period".to_string(), "0.03".to_string()),
                ("gain".to_string(), "1.5".to_string())
            ]
        );
        let msg = s.refusal("ctrl_pkg::controller", "boom").unwrap();
        assert!(msg.contains("keys `max_pitch_rad`, `wheel_base`"), "{msg}");
        assert!(
            !msg.contains("ctrl_period =") && !msg.contains("gain ="),
            "{msg}"
        );
        let note = s.unseeded_note("ctrl_pkg::controller").unwrap();
        assert!(
            note.contains("the 2 keys they agree on were seeded"),
            "{note}"
        );
        let Some(ProbeSeedRecord::PartlySeeded { seeded, keys, .. }) = s.record() else {
            panic!("{:?}", s.record())
        };
        assert_eq!(seeded, ["ctrl_period", "gain"]);
        assert_eq!(keys, ["max_pitch_rad", "wheel_base"]);
    }

    #[test]
    fn the_digest_moves_with_any_value() {
        let one = |v: &str| {
            models(vec![inst("/c", "controller", &[("ctrl_period", v)], 0)])
                .seeding("ctrl_pkg", "controller", None)
                .digest()
                .map(str::to_string)
        };
        assert_eq!(one("0.03"), one("0.03"));
        assert_ne!(one("0.03"), one("0.05"));
    }

    #[test]
    fn a_composable_instance_is_matched_by_its_plugin() {
        let mut i = inst("/c", "Controller", &[("p", "1")], 0);
        i.exec = Some("Controller".into());
        i.plugin = Some("ctrl_pkg::Controller".into());
        let m = models(vec![i]);
        assert!(matches!(
            m.seeding("ctrl_pkg", "controller", Some("ctrl_pkg::Controller")),
            Seeding::Seeded { .. }
        ));
    }
}
