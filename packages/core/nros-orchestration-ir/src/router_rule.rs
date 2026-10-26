//! phase-516 W1a (RFC-0074 enforcement point 1, "As lowered") — the router
//! `downsampling` rule a bringup's declared ingress lowers to.
//!
//! A subscriber's `ingress: { rate_hz, burst }` is enforced in two places. The
//! device half (phase-462) budgets the read task; this is the router half,
//! which is the only place the device's CPU can be saved, because it drops
//! the excess before the device decodes anything. The rule runs in ROS's
//! `rmw_zenohd`, not on the device, so it is an ARTIFACT deployed with the
//! router's configuration: `<model-stem>.router.json5`, beside the model.
//!
//! What zenoh's interceptor can carry decides the lowering (RFC-0074 "As
//! lowered"):
//!
//! - **burst 1.** The interceptor enforces a minimum interval per rule and per
//!   transport; it has no burst term. So `freq = rate_hz` and the burst is not
//!   carried — the device budget still compiles it.
//! - **the type segment is named, the hash is `*`.** `<domain>/<topic>/
//!   <pkg::msg::dds_::Name_>/*`, the shape `TopicKeyExpr::to_key_wildcard`
//!   builds for a subscriber. `**` would match a deeper topic too.
//! - **co-subscribers take the lowest rate.** One stream per topic per
//!   transport, so the tightest declaration wins.
//! - **untargeted unless a fact targets it.** A rule has no peer selector.
//!   When every zenoh image of the bringup rides a serial link the rule says
//!   `link_protocols: ["serial"]`; otherwise it applies to every peer of the
//!   router, and the file says so.
//!
//! No declaration ⇒ no file, and a stale one is deleted (RFC-0100's
//! 2026-10-10 rule: a reader acts on whatever file sits at the path).

use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

use ros_launch_manifest_model::SystemModel;

/// Suffix of the router artifact, after the model's file stem.
pub const ROUTER_RULE_SUFFIX: &str = ".router.json5";

/// The ROS domain when nothing states one.
const DEFAULT_DOMAIN: u32 = 0;

/// What one topic's rule is lowered from: the tightest subscriber, and every
/// subscriber that declared one (for the provenance comment).
#[derive(Debug, Clone, PartialEq)]
pub struct TopicRate {
    /// Topic FQN, as the model keys it (`/chatter`).
    pub topic: String,
    /// ROS message type, `pkg/msg/Name`.
    pub msg_type: String,
    /// The lowest declared `rate_hz` among the topic's subscribers.
    pub freq: f64,
    /// `(endpoint ref, rate_hz, burst)` of every declaring subscriber, in
    /// endpoint order.
    pub declared_by: Vec<(String, f64, u32)>,
}

/// Every topic whose subscribers declare an ingress, in topic order. A
/// declaration whose endpoint is wired to no topic, or a rate no rule can
/// express (non-finite or non-positive), is skipped — the contract crate
/// refuses both, so this is defence.
pub fn topic_rates(model: &SystemModel) -> Vec<TopicRate> {
    let mut by_topic: BTreeMap<&str, TopicRate> = BTreeMap::new();
    for (topic, wiring) in &model.structure.topics {
        for sub in &wiring.subscribers {
            let Some(ingress) = model
                .contracts
                .sub_endpoints
                .get(sub)
                .and_then(|c| c.ingress)
            else {
                continue;
            };
            if !(ingress.rate_hz.is_finite() && ingress.rate_hz > 0.0) {
                continue;
            }
            let row = by_topic.entry(topic).or_insert_with(|| TopicRate {
                topic: topic.clone(),
                msg_type: wiring.msg_type.clone(),
                freq: ingress.rate_hz,
                declared_by: Vec::new(),
            });
            row.freq = row.freq.min(ingress.rate_hz);
            row.declared_by
                .push((sub.clone(), ingress.rate_hz, ingress.burst));
        }
    }
    let mut out: Vec<TopicRate> = by_topic.into_values().collect();
    for r in &mut out {
        r.declared_by.sort_by(|a, b| a.0.cmp(&b.0));
    }
    out
}

/// The DDS spelling rmw_zenoh puts in a key: `pkg/msg/Name` →
/// `pkg::msg::dds_::Name_`. The same rule as nros-rmw-zenoh's `DdsTypeName`
/// (which this host crate cannot depend on); anything not `a/b/c` passes
/// through.
pub fn dds_type_name(msg_type: &str) -> String {
    if msg_type.contains("::") {
        return msg_type.to_string();
    }
    let parts: Vec<&str> = msg_type.split('/').collect();
    match parts.as_slice() {
        [pkg, kind, name] if !pkg.is_empty() && !kind.is_empty() && !name.is_empty() => {
            let sep = if name.ends_with('_') { "" } else { "_" };
            format!("{pkg}::{kind}::dds_::{name}{sep}")
        }
        _ => msg_type.to_string(),
    }
}

/// The rule's key expression for one topic on one domain: the subscriber
/// wildcard, `<domain>/<topic>/<dds type>/*`.
pub fn key_expr(domain: u32, topic: &str, msg_type: &str) -> String {
    format!(
        "{domain}/{}/{}/*",
        topic.trim_matches('/'),
        dds_type_name(msg_type)
    )
}

/// The deployment facts the rule needs from `system.toml`: which domains the
/// bringup's zenoh images speak, and whether every one of them rides serial.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deployment {
    pub domains: BTreeSet<u32>,
    /// True only when there is at least one zenoh image and every zenoh image
    /// dials a `serial/…` locator: then `link_protocols` singles the device
    /// out. Anything else is untargeted.
    pub serial_only: bool,
}

impl Deployment {
    /// Read from the bringup directory holding `system.toml`, through
    /// `leaf_system::read_image` (the one image-over-defaults-over-system
    /// reader). A bringup with no `[image.*]` is read from `[system]`.
    /// `Ok(None)` when no image is ROUTED — no router rule then.
    ///
    /// `routed` answers, for an image's stated RMW (`None` when it states
    /// none and `[system]` states none either), whether that image's traffic
    /// crosses the router this rule configures. The caller supplies it: a
    /// core crate names no backend (RFC-0071 D2).
    pub fn from_bringup(
        dir: &Path,
        routed: &dyn Fn(Option<&str>) -> bool,
    ) -> Result<Option<Self>, String> {
        let path = dir.join(crate::leaf_system::SYSTEM_TOML);
        let text = std::fs::read_to_string(&path)
            .map_err(|e| format!("{}: cannot read: {e}", path.display()))?;
        let doc: toml::Table =
            toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
        let system = doc.get("system").and_then(|v| v.as_table());
        let images: Vec<String> = doc
            .get("image")
            .and_then(|v| v.as_table())
            .map(|t| t.keys().cloned().collect())
            .unwrap_or_default();
        let is_serial = |loc: Option<&str>| {
            loc.and_then(crate::leaf_system::link_kind_of_locator) == Some("serial")
        };
        let mut domains = BTreeSet::new();
        let mut serial_only = true;
        if images.is_empty() {
            let rmw = system.and_then(|s| s.get("rmw")).and_then(|v| v.as_str());
            if !routed(rmw) {
                return Ok(None);
            }
            let domain = system
                .and_then(|s| s.get("domain_id"))
                .and_then(|v| v.as_integer())
                .and_then(|d| u32::try_from(d).ok())
                .unwrap_or(DEFAULT_DOMAIN);
            domains.insert(domain);
            let loc = system
                .and_then(|s| s.get("locator"))
                .and_then(|v| v.as_str());
            serial_only = is_serial(loc);
        } else {
            for id in &images {
                let Some(img) = crate::leaf_system::read_image(dir, id)? else {
                    continue;
                };
                if !routed(img.rmw.as_deref()) {
                    continue;
                }
                domains.insert(img.network.domain_id.unwrap_or(DEFAULT_DOMAIN));
                serial_only &= is_serial(img.network.locator.as_deref());
            }
        }
        if domains.is_empty() {
            return Ok(None);
        }
        Ok(Some(Self {
            domains,
            serial_only,
        }))
    }
}

/// The lowered rule: one zenoh `downsampling` item.
#[derive(Debug, Clone, PartialEq)]
pub struct RouterRule {
    /// The item's `id` — `nros:<model stem>`.
    pub id: String,
    pub rates: Vec<TopicRate>,
    pub deployment: Deployment,
}

impl RouterRule {
    /// `None` when no subscriber declares an ingress.
    pub fn derive(model: &SystemModel, model_stem: &str, deployment: Deployment) -> Option<Self> {
        let rates = topic_rates(model);
        (!rates.is_empty()).then(|| Self {
            id: format!("nros:{model_stem}"),
            rates,
            deployment,
        })
    }

    /// The `downsampling` item as JSON (valid json5, and the value half of a
    /// `ZENOH_CONFIG_OVERRIDE` entry). Only fields zenoh 1.x's
    /// `DownsamplingItemConf` defines — it is `deny_unknown_fields`.
    pub fn item(&self) -> serde_json::Value {
        let mut rules = Vec::new();
        for d in &self.deployment.domains {
            for r in &self.rates {
                rules.push(serde_json::json!({
                    "key_expr": key_expr(*d, &r.topic, &r.msg_type),
                    "freq": r.freq,
                }));
            }
        }
        let mut item = serde_json::json!({
            "id": self.id,
            "messages": ["put"],
            "flows": ["egress"],
            "rules": rules,
        });
        if self.deployment.serial_only {
            item["link_protocols"] = serde_json::json!(["serial"]);
        }
        item
    }

    /// The `ZENOH_CONFIG_OVERRIDE` entry (`downsampling=[…]`). Compact, and
    /// with no `;`, which separates entries.
    pub fn config_override(&self) -> String {
        format!(
            "downsampling={}",
            serde_json::Value::Array(vec![self.item()])
        )
    }

    /// The artifact: a json5 config fragment headed by its provenance.
    pub fn render(&self) -> String {
        let mut out = String::from(
            "// Generated by `nros sync` (RFC-0074 enforcement point 1, phase-516 W1).\n\
             // Do not edit: re-run `nros sync`. Deploy with the router's config, e.g.\n\
             // `ZENOH_CONFIG_OVERRIDE=\"$(nros ws router-config --model <model>)\"`.\n\
             //\n\
             // Lowered at burst 1: the router enforces only `freq`, a minimum\n\
             // interval; each declared burst is enforced by the device budget.\n",
        );
        if self.deployment.serial_only {
            out.push_str("// Targets serial links only: every routed image here dials serial.\n");
        } else {
            out.push_str(
                "// UNTARGETED: the rule caps this router's egress to EVERY peer on these\n\
                 // topics, host ROS subscribers included. Run it on the device's edge router.\n",
            );
        }
        out.push_str("//\n");
        for r in &self.rates {
            out.push_str(&format!("// {} = {} Hz, from:\n", r.topic, r.freq));
            for (ep, rate, burst) in &r.declared_by {
                out.push_str(&format!("//   {ep}: rate_hz {rate}, burst {burst}\n"));
            }
        }
        let doc = serde_json::json!({ "downsampling": [self.item()] });
        out.push_str(&serde_json::to_string_pretty(&doc).unwrap_or_default());
        out.push('\n');
        out
    }
}

/// The `ZENOH_CONFIG_OVERRIDE` string for an artifact [`RouterRule::render`]
/// wrote: its JSON body under the `//` provenance lines, re-emitted compact.
/// Reading the ARTIFACT, not re-lowering, so what is deployed is byte-for-byte
/// the rule `nros sync` wrote.
pub fn config_override_from_artifact(text: &str) -> Result<String, String> {
    let body: String = text
        .lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n");
    let doc: serde_json::Value =
        serde_json::from_str(&body).map_err(|e| format!("not a router rule artifact: {e}"))?;
    let items = doc
        .get("downsampling")
        .filter(|v| v.is_array())
        .ok_or("not a router rule artifact: no `downsampling` list")?;
    Ok(format!("downsampling={items}"))
}

/// Where the router artifact for `model_path` lives: beside it, named after
/// the model's stem.
pub fn router_rule_path_for_model(model_path: &Path) -> PathBuf {
    let stem = model_stem(model_path);
    model_path
        .parent()
        .map(|d| d.join(format!("{stem}{ROUTER_RULE_SUFFIX}")))
        .unwrap_or_else(|| PathBuf::from(format!("{stem}{ROUTER_RULE_SUFFIX}")))
}

fn model_stem(model_path: &Path) -> String {
    model_path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "model".to_string())
}

/// Lower the model at `model_path` with the bringup's deployment and write
/// the rule beside it — or delete a stale one when nothing declares an
/// ingress. `system.toml` is read only when a declaration exists, so a bringup
/// that declares none is untouched by this. Write-if-changed, like the
/// overlay. Returns the path when a file now exists.
pub fn write_beside(
    model_path: &Path,
    bringup_dir: &Path,
    routed: &dyn Fn(Option<&str>) -> bool,
) -> Result<Option<PathBuf>, String> {
    let path = router_rule_path_for_model(model_path);
    let rule = lower(model_path, bringup_dir, routed)?;
    let Some(rule) = rule else {
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                return Err(format!(
                    "{}: cannot remove stale router rule: {e}",
                    path.display()
                ));
            }
        }
        return Ok(None);
    };
    let text = rule.render();
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        std::fs::write(&path, &text)
            .map_err(|e| format!("{}: cannot write: {e}", path.display()))?;
    }
    Ok(Some(path))
}

/// The rule the model at `model_path` lowers to under the bringup at
/// `bringup_dir`, or `None` (no declared ingress, or no routed image).
/// `routed` as for [`Deployment::from_bringup`].
pub fn lower(
    model_path: &Path,
    bringup_dir: &Path,
    routed: &dyn Fn(Option<&str>) -> bool,
) -> Result<Option<RouterRule>, String> {
    let loaded = crate::overlay::LoadedModel::load(model_path)?;
    if topic_rates(&loaded.model).is_empty() {
        return Ok(None);
    }
    let Some(deployment) = Deployment::from_bringup(bringup_dir, routed)? else {
        return Ok(None);
    };
    Ok(RouterRule::derive(
        &loaded.model,
        &model_stem(model_path),
        deployment,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ros_launch_manifest_model::{IngressContract, SubContract, TopicWiring};

    /// `(topic, subscriber endpoint, ingress as (rate_hz, burst))`.
    type Sub<'a> = (&'a str, &'a str, Option<(f64, u32)>);

    fn model(subs: &[Sub<'_>]) -> SystemModel {
        let mut m = SystemModel::default();
        for (topic, ep, ingress) in subs {
            m.structure
                .topics
                .entry((*topic).to_string())
                .or_insert_with(|| TopicWiring {
                    msg_type: "std_msgs/msg/Int32".into(),
                    ..Default::default()
                })
                .subscribers
                .push((*ep).to_string());
            m.contracts.sub_endpoints.insert(
                (*ep).to_string(),
                SubContract {
                    ingress: ingress.map(|(rate_hz, burst)| IngressContract { rate_hz, burst }),
                    ..Default::default()
                },
            );
        }
        m
    }

    /// The test's stand-in for the caller's backend question: `"routed"`, or
    /// nothing stated.
    fn routed(rmw: Option<&str>) -> bool {
        rmw.is_none_or(|r| r == "routed")
    }

    fn untargeted(domains: &[u32]) -> Deployment {
        Deployment {
            domains: domains.iter().copied().collect(),
            serial_only: false,
        }
    }

    #[test]
    fn key_expr_matches_the_subscriber_wildcard() {
        // nros-rmw-zenoh's `to_key_wildcard` for the same topic.
        assert_eq!(
            key_expr(0, "/chatter", "std_msgs/msg/Int32"),
            "0/chatter/std_msgs::msg::dds_::Int32_/*"
        );
        assert_eq!(
            key_expr(7, "/ns/scan", "sensor_msgs/msg/LaserScan"),
            "7/ns/scan/sensor_msgs::msg::dds_::LaserScan_/*"
        );
        assert_eq!(dds_type_name("a::msg::dds_::B_"), "a::msg::dds_::B_");
        assert_eq!(
            dds_type_name("pkg/action/F_SendGoal_"),
            "pkg::action::dds_::F_SendGoal_"
        );
    }

    #[test]
    fn no_declaration_no_rule() {
        let m = model(&[("/chatter", "/listener/sub", None)]);
        assert!(RouterRule::derive(&m, "s", untargeted(&[0])).is_none());
    }

    #[test]
    fn co_subscribers_take_the_lowest_rate() {
        let m = model(&[
            ("/chatter", "/a/sub", Some((50.0, 4))),
            ("/chatter", "/b/sub", Some((20.0, 1))),
            ("/chatter", "/c/sub", None),
        ]);
        let rule = RouterRule::derive(&m, "sys", untargeted(&[0])).unwrap();
        assert_eq!(rule.rates.len(), 1);
        assert_eq!(rule.rates[0].freq, 20.0);
        assert_eq!(rule.rates[0].declared_by.len(), 2);
    }

    #[test]
    fn item_is_zenoh_shaped_and_has_one_rule_per_topic_and_domain() {
        let m = model(&[
            ("/a", "/n/a", Some((50.0, 4))),
            ("/b", "/n/b", Some((10.0, 1))),
        ]);
        let rule = RouterRule::derive(&m, "sys", untargeted(&[0, 3])).unwrap();
        let item = rule.item();
        assert_eq!(item["id"], "nros:sys");
        assert_eq!(item["messages"], serde_json::json!(["put"]));
        assert_eq!(item["flows"], serde_json::json!(["egress"]));
        assert!(item.get("link_protocols").is_none());
        let keys: Vec<&str> = item["rules"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["key_expr"].as_str().unwrap())
            .collect();
        assert_eq!(
            keys,
            [
                "0/a/std_msgs::msg::dds_::Int32_/*",
                "0/b/std_msgs::msg::dds_::Int32_/*",
                "3/a/std_msgs::msg::dds_::Int32_/*",
                "3/b/std_msgs::msg::dds_::Int32_/*",
            ]
        );
        // Only `DownsamplingItemConf`'s fields — zenoh refuses any other.
        let known = [
            "id",
            "interfaces",
            "link_protocols",
            "messages",
            "rules",
            "flows",
        ];
        for k in item.as_object().unwrap().keys() {
            assert!(known.contains(&k.as_str()), "unknown field `{k}`");
        }
        let o = rule.config_override();
        assert!(o.starts_with("downsampling=[{"), "{o}");
        assert!(!o.contains(';'), "{o}");
        assert!(rule.render().contains("UNTARGETED"));
        // The deploy door reads the artifact back to the same override.
        assert_eq!(config_override_from_artifact(&rule.render()).unwrap(), o);
    }

    #[test]
    fn serial_only_targets_the_serial_link() {
        let m = model(&[("/a", "/n/a", Some((50.0, 4)))]);
        let rule = RouterRule::derive(
            &m,
            "sys",
            Deployment {
                domains: [0].into(),
                serial_only: true,
            },
        )
        .unwrap();
        assert_eq!(rule.item()["link_protocols"], serde_json::json!(["serial"]));
        assert!(!rule.render().contains("UNTARGETED"));
    }

    #[test]
    fn write_beside_writes_a_declared_rule_and_deletes_a_stale_one() {
        let tmp = std::env::temp_dir().join(format!("nros-router-wb-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(
            tmp.join("system.toml"),
            "[system]\nname=\"d\"\nrmw=\"routed\"\ndomain_id=0\n",
        )
        .unwrap();
        let model_path = tmp.join("sys.yaml");
        let yaml = |ingress: &str| {
            format!(
                "meta: {{ version: 1 }}\nstructure:\n  topics:\n    /chatter:\n      \
                 type: std_msgs/msg/Int32\n      sub: [/listener/chatter]\n\
                 contracts:\n  sub_endpoints:\n    /listener/chatter:\n      {ingress}\n"
            )
        };
        std::fs::write(&model_path, yaml("ingress: { rate_hz: 50, burst: 4 }")).unwrap();
        let written = write_beside(&model_path, &tmp, &routed).unwrap().unwrap();
        assert_eq!(written, tmp.join("sys.router.json5"));
        let text = std::fs::read_to_string(&written).unwrap();
        assert!(
            text.contains("0/chatter/std_msgs::msg::dds_::Int32_/*"),
            "{text}"
        );
        // The declaration goes away: so does the rule.
        std::fs::write(&model_path, yaml("required: true")).unwrap();
        assert!(write_beside(&model_path, &tmp, &routed).unwrap().is_none());
        assert!(!written.exists());
        std::fs::remove_dir_all(&tmp).unwrap();
    }

    #[test]
    fn deployment_reads_the_system_header_without_images() {
        let tmp = std::env::temp_dir().join(format!("nros-router-rule-{}", std::process::id()));
        std::fs::create_dir_all(&tmp).unwrap();
        let toml = tmp.join("system.toml");
        std::fs::write(&toml, "[system]\nname=\"d\"\nrmw=\"routed\"\ndomain_id=5\n").unwrap();
        let d = Deployment::from_bringup(&tmp, &routed).unwrap().unwrap();
        assert_eq!(d.domains, [5].into());
        assert!(!d.serial_only);
        std::fs::write(&toml, "[system]\nname=\"d\"\nrmw=\"direct\"\n").unwrap();
        assert!(Deployment::from_bringup(&tmp, &routed).unwrap().is_none());
        std::fs::remove_dir_all(&tmp).unwrap();
    }
}
