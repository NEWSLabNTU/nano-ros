//! phase-454 W7 (RFC-0100 D8) -- the contract and `qos_overrides.*` must agree,
//! through the whole channel a build takes.
//!
//! The unit tests in `nros_orchestration_ir::qos_agreement` feed `check_model` a
//! hand-built model. This one produces the model the way a build does: the
//! pinned `nros-launch-resolve` over a launch file and its contract sidecar,
//! exactly as `nros sync` would. Both halves have to work for a divergence to
//! be caught -- a `<param>` has to survive resolution into
//! `structure.nodes.<n>.params` and a `qos:` block has to survive into
//! `contracts.{pub,sub}_endpoints`, and until this wave nothing had ever read
//! the two together.
//!
//! Three fixtures, one per outcome of the ruling:
//!
//! * `agrees` -- every override restates a policy the contract states, with the
//!   same value. BUILDS.
//! * `diverges` -- the same policy with different values, in BOTH directions (a
//!   widening depth and a narrowing reliability). REFUSED.
//! * `params_only` -- the override states a policy the contract omits. REFUSED,
//!   naming the contract line to add.
//!
//! The fourth case -- a contract-only policy, which is every in-tree contract --
//! is asserted against W3's own `qos_policies` fixture, so the normal case is
//! checked against a file this wave did not author.
//!
//! Run with:
//! `cargo test --manifest-path packages/cli/Cargo.toml --test contract_qos_override_agreement`

use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use nros_orchestration_ir::qos_agreement::{QosAgreementError, check_model};
use ros_launch_manifest_model::SystemModel;

/// ACCEPTANCE 2 -- an agreeing pair builds.
///
/// Also the guard on scope: the talker carries a `deadline` override, which is
/// modelled, is not a capacity, and has no contract key. A comparison that
/// widened past the four capacity policies would refuse this file.
#[test]
fn an_agreeing_pair_builds() {
    let m = resolve("agrees");
    // The precondition, asserted rather than assumed: both halves must have
    // survived resolution, or a green here would mean the check found nothing
    // to compare. That is the vacuous shape `check-no-vacuous-tests` names.
    let params = &m.structure.nodes["/listener"].params;
    assert_eq!(
        params
            .get("qos_overrides./chatter.subscription.depth")
            .map(|v| v.to_bake_string())
            .as_deref(),
        Some("3"),
        "the launch `<param>` must reach the model: {params:#?}"
    );
    assert_eq!(
        m.contracts.sub_endpoints["/listener/chatter"]
            .qos
            .as_ref()
            .and_then(|q| q.depth),
        Some(3),
        "the contract `qos:` must reach the model"
    );
    assert_eq!(check_model(&m), Ok(()), "an agreeing pair must build");
}

/// ACCEPTANCE 1 -- a divergent pair fails, naming BOTH sites and BOTH values.
///
/// Both directions in one model: the subscription WIDENS (contract 3,
/// parameter 64 -- issue 1190's under-size) and the publisher NARROWS (contract
/// `reliable`, parameter `best_effort`). The ruling is not a bound check, so
/// both are red.
#[test]
fn a_divergent_pair_is_refused_naming_both_sites() {
    let m = resolve("diverges");
    let e = check_model(&m).expect_err("a divergent pair must be refused");
    let QosAgreementError::Divergent(ds) = &e else {
        panic!("expected divergences, got: {e}");
    };
    let mut seen: Vec<(&str, &str, Option<&str>, &str)> = ds
        .iter()
        .map(|d| {
            (
                d.role,
                d.policy,
                d.contract_value.as_deref(),
                d.param_value.as_str(),
            )
        })
        .collect();
    seen.sort();
    assert_eq!(
        seen,
        vec![
            ("publisher", "reliability", Some("reliable"), "best_effort"),
            ("subscription", "depth", Some("3"), "64"),
        ],
        "both directions must be reported, each with both values"
    );

    let text = e.to_string();
    for want in [
        // The PARAMETER site, verbatim -- someone has to be able to grep for it.
        "qos_overrides./chatter.subscription.depth",
        "qos_overrides./chatter.publisher.reliability",
        // The CONTRACT site: the file and the endpoint.
        "diverges.contract.yaml",
        "/listener/chatter",
        "/talker/chatter",
        // Both values, on both rows.
        "depth: 3",
        "`64`",
        "reliability: reliable",
        "`best_effort`",
    ] {
        assert!(text.contains(want), "missing `{want}` in:\n{text}");
    }
}

/// ACCEPTANCE 3 -- a params-only policy fails, naming the contract line to add.
#[test]
fn a_params_only_policy_is_refused_naming_the_contract_line() {
    let m = resolve("params_only");
    let e = check_model(&m).expect_err("a params-only policy must be refused");
    let QosAgreementError::Divergent(ds) = &e else {
        panic!("expected divergences, got: {e}");
    };
    assert_eq!(ds.len(), 1, "{ds:#?}");
    assert_eq!(ds[0].policy, "reliability");
    assert_eq!(
        ds[0].contract_value, None,
        "the contract states nothing -- that is the whole case"
    );
    assert_eq!(ds[0].endpoint.as_deref(), Some("/listener/chatter"));

    let text = e.to_string();
    // The contract line to add, as a user would paste it. Asserted as ONE
    // block: the nesting is what makes it pasteable, and asserting the keys
    // separately would pass on a message that named them in any order.
    assert!(
        text.contains(
            "    nodes:\n      listener:\n        sub:\n          chatter:\n            \
             qos:\n              reliability: reliable"
        ),
        "the contract line to add is not in:\n{text}"
    );
    assert!(
        text.contains("params_only.contract.yaml"),
        "the file to edit is not named in:\n{text}"
    );
}

/// ACCEPTANCE 4 -- a contract-only policy builds. That is the normal case, and
/// this asserts it against W3's fixture rather than one written for this wave:
/// `qos_policies` states all four policies on both sides of a topic and sets no
/// parameter at all, which is every contract in the tree today.
#[test]
fn a_contract_only_policy_builds() {
    let m = resolve_in("qos_policies", "policies");
    assert!(
        m.structure
            .nodes
            .values()
            .all(|n| n.resolved_params("/x").is_empty()),
        "the precondition of this test is that NOTHING states an override"
    );
    assert!(
        m.contracts.sub_endpoints["/listener/chatter"]
            .qos
            .as_ref()
            .is_some_and(|q| q.reliability.is_some()),
        "…and that the contract states one"
    );
    assert_eq!(check_model(&m), Ok(()));
}

/// A model with no contract abstains: nobody authored one, so there is a single
/// producer and nothing to disagree with.
///
/// This is what keeps every existing image building. Measured against the real
/// file -- `multi-node-workspace-cpp`'s bringup sets
/// `qos_overrides./chatter.publisher.reliability` and has no contract sidecar,
/// and it is one of the two in-tree producers of a `qos_overrides.*` parameter.
#[test]
fn an_image_with_no_contract_is_untouched() {
    let repo = repo_root();
    let bringup = repo.join("examples/templates/multi-node-workspace-cpp/src/demo_bringup");
    assert!(
        bringup.join("launch/system.launch.xml").is_file(),
        "the in-tree producer this test measures has moved: {}",
        bringup.display()
    );
    let m = resolve_at(&bringup, "system");
    assert!(
        m.structure.nodes["/talker"]
            .params
            .contains_key("qos_overrides./chatter.publisher.reliability"),
        "the override must be present, or this test proves nothing"
    );
    assert!(
        m.structure.topics.is_empty(),
        "…and no contract describes wiring, which is why the check abstains"
    );
    assert_eq!(check_model(&m), Ok(()));
}

/// Resolve `qos_agreement/launch/<stem>.launch.xml` through the pinned
/// resolver, by ABSOLUTE path (issue 0285).
fn resolve(stem: &str) -> SystemModel {
    resolve_in("qos_agreement", stem)
}

fn resolve_in(fixture: &str, stem: &str) -> SystemModel {
    let bringup = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(fixture);
    resolve_at(&bringup, stem)
}

fn resolve_at(bringup: &Path, stem: &str) -> SystemModel {
    let repo = repo_root();
    let resolver = repo.join("packages/cli/nros-launch-resolve/target/release/nros-launch-resolve");
    assert!(
        resolver.is_file(),
        "nros-launch-resolve not built at {} -- run `just setup-launch-resolve`",
        resolver.display()
    );
    let out = temp_output(&repo, stem);
    fs::create_dir_all(&out).expect("create model out dir");
    let model = out.join("system_model.yaml");
    let output = std::process::Command::new(&resolver)
        .arg(bringup.join(format!("launch/{stem}.launch.xml")))
        .arg("--bringup-root")
        .arg(bringup)
        .arg("-o")
        .arg(&model)
        .output()
        .expect("spawn nros-launch-resolve");
    assert!(
        output.status.success(),
        "nros-launch-resolve failed for {stem}:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let text = fs::read_to_string(&model).expect("read the resolved model");
    let _ = fs::remove_dir_all(&out);
    SystemModel::from_yaml_str(&text).expect("the resolved model parses")
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("repo root")
        .to_path_buf()
}

/// Unique scratch dir under the repo's gitignored `tmp/` (repo rule: temp
/// files live in `$project/tmp/`, not the system temp dir).
fn temp_output(repo: &Path, name: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = repo.join("tmp").join(format!(
        "qos-agreement-{name}-{}-{stamp}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&dir);
    dir
}
