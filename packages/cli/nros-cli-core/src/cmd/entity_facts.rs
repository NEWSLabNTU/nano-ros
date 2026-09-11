//! phase-392 W5.b/W5.c — the entity figures a backend must size its tables
//! from, resolved from the declaration and printed as env lines.
//!
//! Sibling of [`crate::cmd::board_facts`], and deliberately shaped like it: one
//! resolution, one implementation, delivered through the process environment
//! because that is the only carrier that reaches the cargo invocation a
//! workspace member is built by (issue 0460, phase-349 W2.0).
//!
//! **What it answers.** `ZPICO_MAX_QUERYABLES` sizes `SERVICE_BUFFERS` (as
//! `ZPICO_MAX_SESSIONS * ZPICO_MAX_QUERYABLES`) and the C shim's per-session
//! queryable table. Its default was `if hosted { 32 } else { 8 }` — a literal
//! picked for headroom in `nros-zpico-build`, because nothing at that point
//! knows the answer. It cost a native talker 144,128 B of service buffers for
//! services it does not have. Two halves decide the real number and this verb
//! carries both:
//!
//! * `NROS_DECLARED_SERVICE_SERVERS` — the APPLICATION's own count, from the
//!   model's `structure.services` / `structure.actions` wiring. An action
//!   server is three services on the wire, so actions multiply.
//! * `NROS_DECLARED_INFRA_QUERYABLES` — whether the ROS parameter services and
//!   the REP-2002 lifecycle services are in the image, from
//!   `execution.features`. This is the half a build script cannot see any other
//!   way: cargo exposes no other crate's features.
//!
//! The consumer adds the infrastructure COST (`PARAM_SERVICE_QUERYABLES`,
//! `LIFECYCLE_SERVICE_QUERYABLES`, both defined beside the code that creates
//! them). This verb never states those numbers — it says which features are on,
//! not what they cost, which is the split that keeps issue 0460 closed.
//!
//! **Why the model and not a knob.** The launch declaration is the contract
//! (phase-392 W5.b2). A floor-plus-headroom rule would re-create in miniature
//! the guess this wave exists to delete, and a guess derived from something
//! real is still a guess nobody can audit.

use std::{collections::BTreeMap, path::PathBuf};

use clap::Args as ClapArgs;
use eyre::{Result, WrapErr, bail};
use ros_launch_manifest_model::SystemModel;

/// One action server is three zenoh queryables (`send_goal`, `cancel_goal`,
/// `get_result`; feedback and status are topics).
///
/// MIRROR of `nros_node::executor::action::ACTION_SERVER_QUERYABLES`, held to
/// its definition by `check-infra-queryable-counts`. The CLI cannot depend on
/// `nros-node` — that crate is `no_std`, platform-gated and built for the
/// target, not the host — so the number is restated here and gated, which is
/// the whole difference between this and the seven prose spellings issue 0827
/// found.
const ACTION_SERVER_QUERYABLES: usize = 3;

#[derive(Debug, ClapArgs)]
pub struct EntityFactsArgs {
    /// Resolved SystemModel to read. Mutually exclusive with `--bringup-dir`.
    #[arg(long, value_name = "PATH")]
    pub model: Option<PathBuf>,

    /// The bringup package DIRECTORY, addressed the way `nano_ros_entry(LAUNCH
    /// …)` and `nros::main!(launch = …)` address it. The model path is derived
    /// by `nros_orchestration_ir::model_location`, never spelled here.
    #[arg(long = "bringup-dir", value_name = "DIR", conflicts_with = "model")]
    pub bringup_dir: Option<PathBuf>,

    /// Launch file relative to `<bringup>/launch/`. Defaults to the bringup's
    /// `[system] default_launch`.
    #[arg(long, value_name = "FILE", requires = "bringup_dir")]
    pub launch: Option<String>,

    /// Launch argument binding `key=value` (repeatable), for an arg-bound
    /// model variant.
    #[arg(long = "arg", value_name = "K=V", requires = "bringup_dir")]
    pub args: Vec<String>,

    /// Issue 1142 — a STANDALONE leaf directory (`system.toml` beside its
    /// `CMakeLists.txt` / `Cargo.toml`), answered from what that file
    /// DECLARES rather than from a resolved model.
    ///
    /// Prints NOTHING and exits 0 when the leaf declares no entities: the
    /// caller then has no facts to carry, which is the state every standalone
    /// leaf was in before this. An absent or unreadable `system.toml` is an
    /// error, because the caller named one.
    #[arg(
        long,
        value_name = "DIR",
        conflicts_with = "model",
        conflicts_with = "bringup_dir"
    )]
    pub leaf: Option<PathBuf>,
}

/// The two facts, in emission order.
///
/// A pure function over the model, with file IO and the environment lifted out:
/// a sizing rule verified by reading is how this campaign's other defects
/// survived (phase-392 W5.d).
pub fn facts_from_model(model: &SystemModel) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    if let Some(n) = declared_service_servers(model) {
        out.insert("NROS_DECLARED_SERVICE_SERVERS".to_string(), n.to_string());
    }
    out.insert(
        "NROS_DECLARED_INFRA_QUERYABLES".to_string(),
        declared_infra(model).to_string(),
    );
    // phase-426 W3 — the third fact, and it is a COUNT rather than a cost, for
    // the same reason the two above split that way: the ROS parameter services
    // are registered once per node, so the consumer multiplies its own
    // `PARAM_SERVICE_QUERYABLES` by this. Stating "the image needs 18 slots"
    // here would put the number in two places again.
    //
    // Always emitted, never abstained: unlike the wiring an application
    // declares, `structure.nodes` is what a launch file IS. A model that
    // resolves has nodes; one with none is an empty system, and a zero here
    // would be a claim the consumer must not act on — so it floors at one, on
    // the consumer's side, where the pool is.
    out.insert(
        "NROS_DECLARED_NODES".to_string(),
        model.structure.nodes.len().to_string(),
    );
    out
}

/// Whether this model DESCRIBES the graph's wiring at all.
///
/// **Abstaining here is the DESIGNED outcome, not a symptom (issue 0973).**
/// Endpoint wiring is AUTHORED, never derived. A plain `<node>` launch file
/// names a node; it does not say what that node publishes or serves, and
/// nothing else in the resolver's inputs does either. `model_builder` fills
/// `structure.{topics,services,actions}` from a `ManifestIndex`, and
/// `manifest_loader` builds that index from a CONTRACT — the provider sidecar
///
/// ```text
/// <bringup>/launch/<stem>.contract.yaml     beside <stem>.launch.xml
/// ```
///
/// or an overlay root passed as `--contracts <dir>`. That file is the ONE input
/// a user authors to make this function true, and naming it is the difference
/// between "the model does not say" being actionable and being merely true.
///
/// So an absent `services` map does NOT mean "this system has no service
/// servers" — it means nobody stated the answer. Reporting 0 there would size
/// the queryable table to the infrastructure alone and exhaust it the moment a
/// node registers: a confident wrong number, sized exactly, which is the
/// failure shape this campaign keeps finding rather than a new one. The
/// discriminator is whether ANY wiring was described. If it was, an empty
/// `services` map is a real zero. If nothing was, the question is unanswered
/// and this verb says nothing rather than guessing.
///
/// A caller must therefore not read the abstain as "the resolver lost
/// something". Nothing is lost; the input does not exist. (There WAS one real
/// instance of loss — the loader silently dropped `actions:` — and R1-P2 fixed
/// it. Check for a contract file before searching the resolver again.)
///
/// **Re-measured 2026-09-06, and the count moves — the correspondence does
/// not.** Resolving every launch file in the tree: 122 `*.launch.xml`, 5
/// `*.contract.yaml`, and of the 114 that resolve standalone exactly 5 describe
/// wiring — the same 5, in `examples/workspaces/cpp/src/demo_bringup/launch/`.
/// It read `0 of 119` when issue 0973 was answered on 2026-09-03 and changed
/// the day phase-412 landed the first contracts. Quote the invariant (wiring
/// <=> an authored contract), never the number.
///
/// A consumer wanting per-image entity counts writes a contract; there is no
/// second source. The per-component `nano_ros_node_register(... ENTITIES ...)`
/// route issue 0900 took is RETIRED (phase-412) — it is now a `FATAL_ERROR`
/// naming this same file, because a list hand-maintained beside the code
/// drifted from it on the safety island and every derived pool came out short.
///
/// Sibling predicate, and it is NOT identical: `EntityInventory::from_model`
/// also accepts a non-empty `contracts.node_paths`, which is where a contract's
/// timer paths land. A timer-only contract is therefore wiring to that
/// consumer and silence to this one — issue 1140.
fn describes_wiring(model: &SystemModel) -> bool {
    !model.structure.topics.is_empty()
        || !model.structure.services.is_empty()
        || !model.structure.actions.is_empty()
}

/// Every service server the model declares, counted as QUERYABLES.
///
/// `ServiceWiring::server` lists the endpoint refs serving a service
/// (`"<node FQN>/<endpoint>"`), so the count is the number of refs, not the
/// number of services: two nodes serving the same name are two queryables.
///
/// Actions are the same wiring type in a separate map and cost
/// [`ACTION_SERVER_QUERYABLES`] each.
///
/// The whole model is counted rather than one node's share: an entry image
/// realizes its model, and the queryable table is per SESSION, which the image
/// has one of.
fn declared_service_servers(model: &SystemModel) -> Option<usize> {
    if !describes_wiring(model) {
        return None;
    }
    let services: usize = model
        .structure
        .services
        .values()
        .map(|s| s.server.len())
        .sum();
    let actions: usize = model
        .structure
        .actions
        .values()
        .map(|a| a.server.len())
        .sum();
    Some(services + actions * ACTION_SERVER_QUERYABLES)
}

/// Which infrastructure service families the image carries.
///
/// UNKNOWN feature names are ignored on purpose. `execution.features` is an
/// open axis — `safety` is in the tree today and is not a queryable question —
/// so a name this verb does not recognise means "not one of mine", never an
/// error. The consumer refuses an unknown SPELLING of this verb's own output,
/// which is a different thing: that would be a broken channel, not an
/// unrelated feature.
fn declared_infra(model: &SystemModel) -> &'static str {
    // Issue 1270 -- the SAME predicate the entity inventory counts from, so
    // the cmake road and the inventory cannot disagree about whether a family
    // is in the image. Issue 1142 moved the four SPELLINGS there too, for the
    // same reason one level down: the leaf road emits them as well.
    crate::entity_inventory::InfraServices::from_model(model).token()
}

/// Issue 1142 — the same three facts, for a STANDALONE leaf that has no model.
///
/// A copy-out CMake project (`find_package(nano_ros)` +
/// `nano_ros_add_executable`) has no bringup and no resolved SystemModel, so
/// `nros_record_entity_facts` returns early and the RMW sizes its queryable
/// table from `if hosted { 32 } else { 8 }` — a guess by construction. RFC-0098
/// D8 already decided where such a leaf states its surface: `entities = [...]`
/// on its `system.toml` `[[component]]` rows, in the `EntityDecl::parse`
/// grammar, read by the same reader every other road uses.
///
/// **The counting rule is the model road's, not a second one.** A service
/// server is one queryable and an action server is [`ACTION_SERVER_QUERYABLES`]
/// of them — the same constant `declared_service_servers` applies to a model's
/// `structure.services` / `structure.actions`. A client of either costs no
/// queryable, here as there.
///
/// **`Ok(None)` is the leaf that declares nothing**, and it is not an error:
/// the caller carries no facts and the fallback decides, which is exactly where
/// every standalone leaf already was. The CMake side says so out loud rather
/// than saying nothing (`nros_record_leaf_entity_facts`).
///
/// **Nodes** are the `[[component]]` rows: one component is one
/// `Node::create`, which is the same thing `structure.nodes` counts. The
/// consumer floors it at one.
pub fn facts_from_leaf(dir: &std::path::Path) -> Result<Option<BTreeMap<String, String>>> {
    let leaf = nros_orchestration_ir::leaf_system::read(dir)
        .map_err(|e| eyre::eyre!(e))?
        .ok_or_else(|| {
            eyre::eyre!(
                "{}: declares no deployment — write {} (RFC-0098 D3)",
                dir.display(),
                dir.join(nros_orchestration_ir::leaf_system::SYSTEM_TOML)
                    .display()
            )
        })?;
    // The DECLARATION is the opt-in. Without it this verb would be stating an
    // infrastructure answer ("none") about a hand-written `main` nobody
    // described, and a queryable table short of what an image registers is a
    // boot failure rather than a smaller pool (issue 0460).
    let Some(decls) = crate::leaf_entity_env::declared_entities(dir)? else {
        return Ok(None);
    };

    let servers: usize = decls
        .iter()
        .map(|d| match d.kind {
            crate::entity_inventory::EntityKind::ServiceServer => 1,
            crate::entity_inventory::EntityKind::ActionServer => ACTION_SERVER_QUERYABLES,
            _ => 0,
        })
        .sum();
    let nodes = leaf.components.len();
    let infra = crate::entity_inventory::InfraServices::from_features(&leaf.features, nodes);

    let mut out = BTreeMap::new();
    out.insert(
        "NROS_DECLARED_SERVICE_SERVERS".to_string(),
        servers.to_string(),
    );
    out.insert(
        "NROS_DECLARED_INFRA_QUERYABLES".to_string(),
        infra.token().to_string(),
    );
    out.insert("NROS_DECLARED_NODES".to_string(), nodes.to_string());
    Ok(Some(out))
}

pub fn run(args: EntityFactsArgs) -> Result<()> {
    // Issue 1142 — the standalone-leaf road. Handled before the model paths
    // because it reads a different input, not a different location of the same
    // one.
    if let Some(leaf) = &args.leaf {
        let dir = leaf
            .canonicalize()
            .wrap_err_with(|| format!("leaf dir `{}`", leaf.display()))?;
        if let Some(facts) = facts_from_leaf(&dir)? {
            for (k, v) in facts {
                println!("{k}={v}");
            }
        }
        return Ok(());
    }
    let model_path = match (&args.model, &args.bringup_dir) {
        (Some(m), _) => m.clone(),
        (None, Some(b)) => {
            let bringup = b
                .canonicalize()
                .wrap_err_with(|| format!("bringup dir `{}`", b.display()))?;
            let mut launch_args: Vec<(String, String)> = Vec::new();
            for kv in &args.args {
                let Some((k, v)) = kv.split_once('=') else {
                    bail!("--arg takes `key=value`, got `{kv}`");
                };
                launch_args.push((k.to_string(), v.to_string()));
            }
            let rel = nros_orchestration_ir::model_location::launch_to_model_rel(
                &bringup,
                args.launch.as_deref(),
                &launch_args,
            )
            .map_err(|e| eyre::eyre!(e))?;
            nros_orchestration_ir::model_location::resolve_model_path(&bringup, &rel)
        }
        (None, None) => {
            bail!("entity-facts needs --model <path>, --bringup-dir <dir> or --leaf <dir>")
        }
    };

    let model = crate::orchestration::model_ingest::load_model(&model_path)?;
    for (k, v) in facts_from_model(&model) {
        println!("{k}={v}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model(yaml: &str) -> SystemModel {
        SystemModel::from_yaml_str(yaml).expect("test model parses")
    }

    const EMPTY: &str = "meta:\n  version: 1\nstructure: {}\n";

    #[test]
    fn a_model_that_describes_no_wiring_abstains_on_the_app_count() {
        // NOT zero. 109 of the tree's 114 resolvable models are this shape
        // (measured 2026-09-06), including `examples/workspaces/c`'s, whose
        // node is literally called `add_server` and whose model says nothing
        // about services because that workspace authors no contract.
        let m = model(EMPTY);
        assert_eq!(declared_service_servers(&m), None);
        assert_eq!(declared_infra(&m), "none");
        let f = facts_from_model(&m);
        assert!(!f.contains_key("NROS_DECLARED_SERVICE_SERVERS"));
        // The infrastructure half is still answered — that is W5.b1, and it is
        // the half a build script cannot see any other way.
        assert_eq!(f["NROS_DECLARED_INFRA_QUERYABLES"], "none");
    }

    #[test]
    fn wiring_described_with_no_service_server_is_a_real_zero() {
        // A model that describes topics has been through a resolver that
        // describes wiring, so an empty `services` map means what it says.
        let m = model(
            "meta:\n  version: 1\nstructure:\n  topics:\n    /chatter:\n      type: std_msgs/msg/String\n\
             \n      pub: [\"/a/chatter\"]\n",
        );
        assert_eq!(declared_service_servers(&m), Some(0));
        assert_eq!(facts_from_model(&m)["NROS_DECLARED_SERVICE_SERVERS"], "0");
    }

    #[test]
    fn service_servers_are_counted_per_endpoint_not_per_service() {
        // Two nodes serving the same service name are two queryables. Counting
        // the map's keys would say one and under-size the table.
        let m = model(
            "meta:\n  version: 1\nstructure:\n  services:\n    /add:\n      type: example/srv/Add\n\
             \n      server: [\"/a/add\", \"/b/add\"]\n",
        );
        assert_eq!(declared_service_servers(&m), Some(2));
    }

    #[test]
    fn a_client_only_service_costs_no_queryable() {
        let m = model(
            "meta:\n  version: 1\nstructure:\n  services:\n    /add:\n      type: example/srv/Add\n\
             \n      client: [\"/a/add\"]\n",
        );
        assert_eq!(declared_service_servers(&m), Some(0));
    }

    #[test]
    fn an_action_server_costs_three() {
        let m = model(
            "meta:\n  version: 1\nstructure:\n  actions:\n    /fib:\n      type: example/action/Fib\n\
             \n      server: [\"/a/fib\"]\n",
        );
        assert_eq!(declared_service_servers(&m), Some(ACTION_SERVER_QUERYABLES));
    }

    #[test]
    fn services_and_actions_add() {
        let m = model(
            "meta:\n  version: 1\nstructure:\n  services:\n    /add:\n      type: example/srv/Add\n\
             \n      server: [\"/a/add\"]\n  actions:\n    /fib:\n      type: example/action/Fib\n\
             \n      server: [\"/a/fib\"]\n",
        );
        assert_eq!(
            declared_service_servers(&m),
            Some(1 + ACTION_SERVER_QUERYABLES)
        );
    }

    #[test]
    fn infra_reads_both_flags_independently() {
        let f = |feats: &str| {
            let m = model(&format!(
                "meta:\n  version: 1\nstructure: {{}}\nexecution:\n  features:\n{feats}"
            ));
            declared_infra(&m)
        };
        assert_eq!(f("  - param_services\n  - lifecycle\n"), "param+lifecycle");
        assert_eq!(f("  - param_services\n"), "param");
        assert_eq!(f("  - lifecycle\n"), "lifecycle");
    }

    #[test]
    fn an_unrecognised_feature_is_not_an_error_and_not_a_queryable() {
        // `safety` is a real feature in this tree and has nothing to do with
        // queryables. A verb that refused it would break every safety
        // workspace to answer a question it was not asked.
        let m = model("meta:\n  version: 1\nstructure: {}\nexecution:\n  features:\n  - safety\n");
        assert_eq!(declared_infra(&m), "none");
        let m = model(
            "meta:\n  version: 1\nstructure: {}\nexecution:\n  features:\n  - safety\n  - lifecycle\n",
        );
        assert_eq!(declared_infra(&m), "lifecycle");
    }

    #[test]
    fn the_emitted_shape_is_exactly_what_the_consumer_reads() {
        // `nros-zpico-build::queryable_default_from` reads these two names and
        // PANICS on a spelling it does not know, so the pair is a contract.
        let m = model(
            "meta:\n  version: 1\nstructure:\n  services:\n    /add:\n      type: example/srv/Add\n\
             \n      server: [\"/a/add\"]\nexecution:\n  features:\n  - lifecycle\n",
        );
        let f = facts_from_model(&m);
        assert_eq!(f["NROS_DECLARED_SERVICE_SERVERS"], "1");
        assert_eq!(f["NROS_DECLARED_INFRA_QUERYABLES"], "lifecycle");
        // phase-426 W3 — a third name in the contract; the consumer watches
        // and parses all three.
        assert_eq!(f["NROS_DECLARED_NODES"], "0");
        assert_eq!(f.len(), 3);
    }

    /// phase-426 W3 — the node count reaches the queryable pool, because the
    /// ROS parameter services are registered once per node.
    #[test]
    fn the_node_count_is_reported_for_the_parameter_service_pool() {
        let m = model(
            "meta:\n  version: 1\nstructure:\n  nodes:\n    /talker:\n      scope: root\n\
             \n      pkg: demo\n      exec: talker\n    /listener:\n      scope: root\n\
             \n      pkg: demo\n      exec: listener\nexecution:\n  features:\n  - param_services\n",
        );
        let f = facts_from_model(&m);
        assert_eq!(f["NROS_DECLARED_NODES"], "2");
        assert_eq!(f["NROS_DECLARED_INFRA_QUERYABLES"], "param");
    }

    // ---- issue 1142: the standalone-leaf road ---------------------------

    fn leaf_dir(system: &str) -> tempfile::TempDir {
        let td = tempfile::tempdir().unwrap();
        std::fs::write(td.path().join("CMakeLists.txt"), "project(x C)\n").unwrap();
        std::fs::write(td.path().join("system.toml"), system).unwrap();
        td
    }

    const LEAF_HEAD: &str = "[system]\nname = \"x\"\nrmw = \"zenoh\"\n";
    const LEAF_IMAGE: &str = "\n[image.i]\nboard = \"qemu-armv7a-nuttx\"\n";

    /// The image issue 1142 opened on: one node, one ACTION CLIENT. A client
    /// opens no queryable, so the application count is zero — which is an
    /// ANSWER, and the whole difference from the guessed budget.
    #[test]
    fn an_action_client_leaf_declares_zero_service_servers() {
        let td = leaf_dir(&format!(
            "{LEAF_HEAD}\n[[component]]\npkg = \"p\"\nname = \"fibonacci_action_client\"\n\
             entities = [\"action_client:example_interfaces/action/Fibonacci:/fibonacci\"]\n\
             {LEAF_IMAGE}"
        ));
        let f = facts_from_leaf(td.path()).unwrap().expect("declared");
        assert_eq!(f["NROS_DECLARED_SERVICE_SERVERS"], "0");
        assert_eq!(f["NROS_DECLARED_INFRA_QUERYABLES"], "none");
        assert_eq!(f["NROS_DECLARED_NODES"], "1");
        assert_eq!(f.len(), 3, "the leaf road emits the model road's three");
    }

    /// The counting rule is the model road's: a service server is one
    /// queryable, an action server is three, and both clients are zero.
    #[test]
    fn the_leaf_road_counts_servers_exactly_as_the_model_road_does() {
        let td = leaf_dir(&format!(
            "{LEAF_HEAD}\n[[component]]\npkg = \"p\"\nname = \"n\"\n\
             entities = [\"service_server\", \"action_server\", \"service_client\", \
             \"action_client\", \"publisher\", \"sub\", \"timer\"]\n{LEAF_IMAGE}"
        ));
        let f = facts_from_leaf(td.path()).unwrap().expect("declared");
        assert_eq!(
            f["NROS_DECLARED_SERVICE_SERVERS"],
            (1 + ACTION_SERVER_QUERYABLES).to_string()
        );
    }

    /// `[system] features` is the infrastructure half, the same key and the
    /// same four tokens the model road emits.
    #[test]
    fn the_leaf_states_its_infrastructure_families_in_system_features() {
        for (feats, want) in [
            ("[\"param_services\", \"lifecycle\"]", "param+lifecycle"),
            ("[\"param_services\"]", "param"),
            ("[\"lifecycle\"]", "lifecycle"),
            // An unrecognised feature is not one of these and is ignored, as
            // the model road has always done.
            ("[\"safety\"]", "none"),
            ("[]", "none"),
        ] {
            let td = leaf_dir(&format!(
                "{LEAF_HEAD}features = {feats}\n\n[[component]]\npkg = \"p\"\nname = \"n\"\n\
                 entities = [\"timer\"]\n{LEAF_IMAGE}"
            ));
            let f = facts_from_leaf(td.path()).unwrap().expect("declared");
            assert_eq!(f["NROS_DECLARED_INFRA_QUERYABLES"], want, "{feats}");
        }
    }

    /// A leaf that declares NOTHING says nothing. Not a zero: an entity list is
    /// what opts a leaf into stating its own surface, and without it an
    /// infrastructure answer would be a claim about a hand-written `main`
    /// nobody described.
    #[test]
    fn a_leaf_with_no_entity_declaration_abstains_entirely() {
        let td = leaf_dir(&format!(
            "{LEAF_HEAD}\n[[component]]\npkg = \"p\"\nname = \"n\"\n{LEAF_IMAGE}"
        ));
        assert!(facts_from_leaf(td.path()).unwrap().is_none());
    }

    /// One node per `[[component]]`, so a two-component leaf pays the
    /// parameter family twice — the consumer multiplies.
    #[test]
    fn the_node_count_is_the_component_count() {
        let td = leaf_dir(&format!(
            "{LEAF_HEAD}\n[[component]]\npkg = \"p\"\nname = \"a\"\nentities = [\"timer\"]\n\
             \n[[component]]\npkg = \"p\"\nname = \"b\"\nentities = [\"timer\"]\n{LEAF_IMAGE}"
        ));
        let f = facts_from_leaf(td.path()).unwrap().expect("declared");
        assert_eq!(f["NROS_DECLARED_NODES"], "2");
    }

    /// A malformed entity string names the entry rather than deriving a budget
    /// from the rows it could read.
    #[test]
    fn a_malformed_leaf_declaration_refuses() {
        let td = leaf_dir(&format!(
            "{LEAF_HEAD}\n[[component]]\npkg = \"p\"\nname = \"n\"\n\
             entities = [\"nonsense\"]\n{LEAF_IMAGE}"
        ));
        let e = facts_from_leaf(td.path()).unwrap_err().to_string();
        assert!(e.contains("nonsense"), "{e}");
    }

    /// It is a COUNT, not a cost. This verb never states
    /// `PARAM_SERVICE_QUERYABLES` — the consumer owns that number, beside the
    /// code that creates the servers (issue 0827's split).
    #[test]
    fn the_node_count_is_stated_even_when_no_capability_uses_it() {
        let m = model(
            "meta:\n  version: 1\nstructure:\n  nodes:\n    /solo:\n      scope: root\n\
             \n      pkg: demo\n      exec: solo\n",
        );
        let f = facts_from_model(&m);
        assert_eq!(f["NROS_DECLARED_NODES"], "1");
        assert_eq!(f["NROS_DECLARED_INFRA_QUERYABLES"], "none");
    }
}
