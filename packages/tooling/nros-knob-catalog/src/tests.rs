use std::collections::BTreeMap;

use super::*;

fn read(p: &Path) -> std::io::Result<String> {
    std::fs::read_to_string(p)
}

fn provider(dir: &Path, name: &str) -> PackageDir {
    rmw_package(dir, name, "acme", 0)
}

/// An RMW package announcing `rmw`, with its descriptor, in root `root`.
fn rmw_package(dir: &Path, name: &str, rmw: &str, root: usize) -> PackageDir {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join("nros-rmw.toml"), "[rmw]\n").unwrap();
    PackageDir {
        package: name.to_string(),
        dir: dir.to_path_buf(),
        root_index: root,
        root: Some(format!("root{root}")),
        provides: vec![("rmw".into(), rmw.into())],
    }
}

fn write_rows(dir: &Path, text: &str) -> PathBuf {
    let f = dir.join(PACKAGE_FILE);
    std::fs::write(&f, text).unwrap();
    f
}

const ACME: &str = "owner = \"acme\"\n\n[[knob]]\nkey = \"acme.frame_bytes\"\n";

const ACME_TYPED: &str = "owner = \"acme\"\n\n\
    [[knob]]\nkey = \"acme.pool_slots\"\nscope = \"image\"\nclass = \"public\"\n\
    type = \"int\"\nprompt = \"Acme pool slots\"\n\
    kconfig_attrs = [\"default 2\", \"range 1 64\"]\nhelp = \"How many slots.\"\n\
    retired = [{ spelling = \"env:ACME_SLOTS\" }]\n\n\
    [[env]]\nkey = \"acme.trace\"\ndisposition = \"listed\"\nparse = \"bool\"\n";

/// C1: two different packages, one owner word. The message names both files.
#[test]
fn two_packages_with_one_owner_word_are_refused_naming_both_files() {
    let tmp = tempfile::tempdir().unwrap();
    let a = tmp.path().join("acme_a");
    let b = tmp.path().join("acme_b");
    let pa = rmw_package(&a, "acme_a", "acme", 0);
    let pb = rmw_package(&b, "acme_b", "acme", 1);
    let fa = write_rows(&a, ACME);
    let fb = write_rows(&b, "owner = \"acme\"\n\n[[knob]]\nkey = \"acme.other\"\n");
    let msg = Catalog::build(&[pa, pb], &read).unwrap_err().to_string();
    assert!(
        msg.contains("owner word `acme` is claimed by two packages"),
        "{msg}"
    );
    assert!(msg.contains(&fa.display().to_string()), "{msg}");
    assert!(msg.contains(&fb.display().to_string()), "{msg}");
    // An in-tree word is claimed by its package too.
    let z = tmp.path().join("my_zenoh");
    let pz = rmw_package(&z, "my_zenoh", "zenoh", 1);
    write_rows(&z, "owner = \"zenoh\"\n\n[[knob]]\nkey = \"zenoh.mine\"\n");
    let msg = Catalog::build(&[pz], &read).unwrap_err().to_string();
    assert!(
        msg.contains("nros_rmw_zenoh (packages/rmw/zenoh/nros-rmw-zenoh/nros-knobs.toml"),
        "{msg}"
    );
}

/// C2: a package row under another owner's word, and a package on a core word.
#[test]
fn a_package_row_under_another_owner_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let p = provider(tmp.path(), "acme_rmw");
    write_rows(
        tmp.path(),
        "owner = \"acme\"\n\n[[knob]]\nkey = \"zenoh.max_sessions_mine\"\n",
    );
    let msg = Catalog::build(std::slice::from_ref(&p), &read)
        .unwrap_err()
        .to_string();
    assert!(
        msg.contains("key `zenoh.max_sessions_mine` is not under this package's owner word `acme`"),
        "{msg}"
    );
    write_rows(tmp.path(), "owner = \"acme\"\n\n[[env]]\nkey = \"log.x\"\n");
    let msg = Catalog::build(std::slice::from_ref(&p), &read)
        .unwrap_err()
        .to_string();
    assert!(msg.contains("key `log.x` is not under"), "{msg}");
    // The `owner` line is checked against the derived word.
    write_rows(tmp.path(), "owner = \"zenoh\"\n");
    let msg = Catalog::build(&[p], &read).unwrap_err().to_string();
    assert!(msg.contains("derived owner word is `acme`"), "{msg}");
    // A package whose word is a core owner's.
    let l = tmp.path().join("log_rmw");
    let pl = rmw_package(&l, "log_rmw", "log", 0);
    write_rows(&l, "owner = \"log\"\n");
    let msg = Catalog::build(&[pl], &read).unwrap_err().to_string();
    assert!(msg.contains("owner word `log` is a core owner"), "{msg}");
}

/// C4: a package declares only its own library's foreign variables.
#[test]
fn a_package_declaring_another_projects_variable_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    write_rows(
        tmp.path(),
        "owner = \"acme\"\n\n[[env]]\nname = \"ROS_DOMAIN_ID\"\nforeign = \"ros2\"\n",
    );
    let msg = Catalog::build(&[provider(tmp.path(), "acme_rmw")], &read)
        .unwrap_err()
        .to_string();
    assert!(msg.contains("its own library's variables"), "{msg}");
}

/// C1's exception: the same package in two roots is shadowed, not refused.
#[test]
fn the_same_package_in_two_roots_is_shadowed_not_refused() {
    let tmp = tempfile::tempdir().unwrap();
    let under = tmp.path().join("underlay/acme_rmw");
    let over = tmp.path().join("overlay/acme_rmw");
    let pu = rmw_package(&under, "acme_rmw", "acme", 0);
    let po = rmw_package(&over, "acme_rmw", "acme", 1);
    let fu = write_rows(&under, ACME);
    let fo = write_rows(
        &over,
        "owner = \"acme\"\n\n[[knob]]\nkey = \"acme.frame_bytes\"\n\n\
         [[knob]]\nkey = \"acme.overlay_only\"\n",
    );
    let cat = Catalog::build(&[pu, po], &read).expect("one package in two roots is no collision");
    assert_eq!(
        cat.shadowed(),
        &[Shadowed {
            package: "acme_rmw".into(),
            winner: fo,
            shadowed: vec![fu],
        }]
    );
    // The later root's rows are the ones the catalog holds.
    assert!(cat.rows().iter().any(|r| r.name == "acme.overlay_only"));
    assert_eq!(
        cat.rows()
            .iter()
            .filter(|r| r.name == "acme.frame_bytes")
            .count(),
        1
    );
    // An overlay of an in-tree package shadows its compiled rows.
    let z = tmp.path().join("overlay/nros_rmw_zenoh");
    let pz = rmw_package(&z, "nros_rmw_zenoh", "zenoh", 1);
    let fz = write_rows(
        &z,
        "owner = \"zenoh\"\n\n[[knob]]\nkey = \"zenoh.only_knob\"\n",
    );
    let cat = Catalog::build(&[pz], &read).expect("an overlay of an in-tree package");
    assert_eq!(cat.shadowed()[0].winner, fz);
    assert_eq!(
        cat.shadowed()[0].shadowed,
        vec![PathBuf::from(
            "packages/rmw/zenoh/nros-rmw-zenoh/nros-knobs.toml"
        )]
    );
    assert!(!cat.rows().iter().any(|r| r.name == "zenoh.max_queryables"));
}

#[test]
fn core_catalog_holds_every_row_kind() {
    let cat = Catalog::build(&[], &read).expect("the in-tree catalog has one namespace");
    assert_eq!(
        cat.rows().len(),
        KNOBS.len() + STATEMENTS.len() + ENV_ROWS.len()
    );
    assert!(
        cat.rows()
            .iter()
            .any(|r| r.kind == RowKind::Statement && r.name == "domain_id")
    );
    assert!(
        cat.rows()
            .iter()
            .any(|r| r.kind == RowKind::Env && r.name == "ROS_DOMAIN_ID")
    );
    // `NROS_ENTRY_SPIN_MS` is a run-time variable now, not a build knob.
    assert!(
        cat.rows()
            .iter()
            .any(|r| r.kind == RowKind::Env && r.name == "NROS_ENTRY_SPIN_MS")
    );
    assert!(
        !cat.rows()
            .iter()
            .any(|r| r.kind == RowKind::Knob && r.name == "entry.spin_ms")
    );
    // No compiled row is a per-build (package) row.
    assert_eq!(cat.package_rows().count(), 0);
}

#[test]
fn retired_spelling_answers_every_road_core_holds() {
    let cat = Catalog::core();
    // A build-env rename (one of the 32 phase-484 W8 spellings).
    let (row, e) = cat
        .retired_spelling(RetiredRoad::Env, "ZPICO_MAX_QUERYABLES")
        .expect("a retired build spelling");
    assert_eq!(row.kind, RowKind::Knob);
    assert!(
        row.spellings
            .contains(&(RetiredRoad::Env, "NROS_ZENOH_MAX_QUERYABLES".into()))
    );
    assert_eq!((e.factor, e.is_manual()), (1, false));
    // A run-time alias: the generated `RETIRED_ENV_VARS` view.
    let (row, e) = cat
        .retired_spelling(RetiredRoad::Run, "ZENOH_LOCATOR")
        .expect("a retired run-time spelling");
    assert_eq!(row.name, "NROS_LOCATOR");
    assert!(!e.is_manual());
    // A manual retirement: no replacement name, only text.
    let (row, e) = cat
        .retired_spelling(RetiredRoad::Run, "ZENOH_LISTEN")
        .expect("a manual retirement");
    assert_eq!(row.name, "ZENOH_CONFIG_OVERRIDE");
    assert!(e.is_manual());
    // The retired knob: every spelling refuses, with the row's reason.
    let (_, e) = cat
        .retired_spelling(RetiredRoad::Env, "ZPICO_LEASE_TASK_PRIORITY")
        .expect("the retired lease knob");
    assert!(e.instead.as_deref().is_some_and(|t| t.contains("lease")));
    // The road is part of the question.
    assert!(
        cat.retired_spelling(RetiredRoad::Run, "ZPICO_MAX_QUERYABLES")
            .is_none()
    );
    assert!(
        cat.retired_spelling(RetiredRoad::Env, "NROS_ZENOH_MAX_QUERYABLES")
            .is_none()
    );
}

#[test]
fn retired_env_vars_view_is_the_registry_run_road() {
    let cat = Catalog::core();
    let pairs: Vec<(String, String)> = cat
        .rows()
        .iter()
        .filter(|r| r.kind == RowKind::Env)
        .flat_map(|r| {
            r.retired
                .iter()
                .filter(|e| e.road == RetiredRoad::Run && !e.is_manual())
                .map(move |e| (e.spelling.clone(), r.name.clone()))
        })
        .collect();
    // Sorted by the replacing row's name; phase-581 W6 added the three
    // run-time renames of RFC-0140 Appendix A.3.
    let want: Vec<(String, String)> = [
        (
            "NROS_CYCLONE_MATCH_TIMEOUT_MS",
            "NROS_CYCLONEDDS_MATCH_TIMEOUT_MS",
        ),
        (
            "NROS_RMW_CYCLONEDDS_SKIP_PREFIX",
            "NROS_CYCLONEDDS_SKIP_PREFIX",
        ),
        ("ZENOH_LOCATOR", "NROS_LOCATOR"),
        ("ZENOH_MODE", "NROS_SESSION_MODE"),
        ("XRCE_SERIAL_BAUD", "NROS_XRCE_SERIAL_BAUD"),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect();
    assert_eq!(pairs, want);
}

#[test]
fn a_package_row_joins_the_catalog_and_its_retired_spellings() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join(PACKAGE_FILE),
        "owner = \"acme\"\n\n[[knob]]\nkey = \"acme.frame_bytes\"\ntype = \"int\"\n\
         retired = [{ spelling = \"env:NROS_ACME_FRAME_KB\", factor = 1024, from_unit = \"KiB\" }]\n\n\
         [[env]]\nkey = \"acme.trace\"\n",
    )
    .unwrap();
    let cat = Catalog::build(&[provider(tmp.path(), "acme_rmw")], &read).unwrap();
    let (row, e) = cat
        .retired_spelling(RetiredRoad::Env, "NROS_ACME_FRAME_KB")
        .expect("a package's retired spelling");
    assert_eq!(row.name, "acme.frame_bytes");
    assert!(matches!(&row.origin, Origin::Package { package, .. } if package == "acme_rmw"));
    assert_eq!((e.factor, e.from_unit.as_deref()), (1024, Some("KiB")));
    assert!(
        row.spellings
            .contains(&(RetiredRoad::Kconfig, "CONFIG_NROS_ACME_FRAME_BYTES".into()))
    );
    assert!(
        cat.rows()
            .iter()
            .any(|r| r.kind == RowKind::Env && r.name == "NROS_ACME_TRACE")
    );
    let text = cat.render_toml();
    assert!(text.contains("name = \"acme.frame_bytes\""), "{text}");
    assert!(
        text.contains("spelling = \"env:NROS_ACME_FRAME_KB\""),
        "{text}"
    );
    let back: toml::Table = toml::from_str(&text).expect("catalog.toml parses");
    assert_eq!(back["row"].as_array().unwrap().len(), cat.rows().len());
}

#[test]
fn a_spelling_two_rows_use_refuses_the_catalog_naming_both() {
    let tmp = tempfile::tempdir().unwrap();
    // The package retires a name the zenoh package's max_queryables already retired.
    std::fs::write(
        tmp.path().join(PACKAGE_FILE),
        "owner = \"acme\"\n\n[[knob]]\nkey = \"acme.queryables\"\n\
         retired = [{ spelling = \"env:ZPICO_MAX_QUERYABLES\" }]\n",
    )
    .unwrap();
    let msg = Catalog::build(&[provider(tmp.path(), "acme_rmw")], &read)
        .unwrap_err()
        .to_string();
    let old = "ZPICO_MAX_QUERYABLES"; // a retired spelling of core's row
    let want = format!(
        "`{old}` is spelled by knob `zenoh.max_queryables` (package nros_rmw_zenoh, \
         packages/rmw/zenoh/nros-rmw-zenoh/nros-knobs.toml)"
    );
    assert!(msg.contains(&want), "{msg}");
    assert!(msg.contains("package acme_rmw"), "{msg}");
}

#[test]
fn an_entry_with_factor_and_instead_is_refused() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(
        tmp.path().join(PACKAGE_FILE),
        "owner = \"acme\"\n[[knob]]\nkey = \"acme.x\"\nretired = [{ spelling = \"env:NROS_ACME_Y\", factor = 2, instead = \"no\" }]\n",
    )
    .unwrap();
    let msg = Catalog::build(&[provider(tmp.path(), "acme_rmw")], &read)
        .unwrap_err()
        .to_string();
    assert!(msg.contains("states both `instead` and `factor`"), "{msg}");
}

/// phase-581 W3 — the knob ladder (`nros-zephyr-build`, dependency-free) reads
/// back the `catalog.toml` this crate writes: a package knob's Kconfig symbol
/// and its retired build spellings, which it refuses.
#[test]
fn the_ladder_reads_the_catalog_this_writes() {
    let tmp = tempfile::tempdir().unwrap();
    write_rows(tmp.path(), ACME_TYPED);
    let cat = Catalog::build(&[provider(tmp.path(), "acme_rmw")], &read).unwrap();
    let text = cat.render_toml();
    let row = nros_zephyr_build::catalog_knob(&text, "NROS_ACME_POOL_SLOTS")
        .expect("the ladder finds a package knob in the catalog");
    assert_eq!(row.kconfig, "CONFIG_NROS_ACME_POOL_SLOTS");
    assert_eq!(row.retired_env, vec!["ACME_SLOTS".to_string()]);
    // A name in no row, and an env row, are not knobs.
    assert!(nros_zephyr_build::catalog_knob(&text, "NROS_ACME_NOPE").is_none());
    assert!(nros_zephyr_build::catalog_knob(&text, "NROS_ACME_TRACE").is_none());
    // Every compiled knob reads back too: the reader is not acme-shaped.
    for k in KNOBS.iter().take(20) {
        let back = nros_zephyr_build::catalog_knob(&text, k.env).expect(k.env);
        assert_eq!(back.kconfig, k.kconfig, "{}", k.env);
    }
    // And the full reader agrees with the writer on every row.
    let parsed = Catalog::parse_toml(&text).unwrap();
    assert_eq!(parsed.rows().len(), cat.rows().len());
    let pool = parsed.knob("acme.pool_slots").unwrap();
    assert_eq!(pool.meta.ty.as_deref(), Some("int"));
    assert_eq!(pool.meta.default.as_deref(), Some("2"));
    assert_eq!(pool.meta.range, Some((1, 64)));
    assert!(!pool.origin.is_compiled());
    // A view rendered from the file equals the one rendered in memory.
    let stated: BTreeMap<String, String> = [("NROS_ACME_POOL_SLOTS".into(), "4".into())].into();
    assert_eq!(
        kconfig_packages(&parsed, &["acme"], &stated),
        kconfig_packages(&cat, &["acme"], &stated)
    );
    assert_eq!(registry_cmake(&parsed), registry_cmake(&cat));
    assert_eq!(parsed.owner_package("acme"), Some("acme_rmw"));
}

#[test]
fn a_package_knob_carries_what_its_views_render() {
    let tmp = tempfile::tempdir().unwrap();
    write_rows(tmp.path(), ACME_TYPED);
    let cat = Catalog::build(&[provider(tmp.path(), "acme_rmw")], &read).unwrap();
    let row = cat.knob("acme.pool_slots").unwrap();
    assert_eq!(row.meta.default.as_deref(), Some("2"));
    assert_eq!(row.meta.range, Some((1, 64)));
    assert_eq!(row.owner(), "acme");
    assert_eq!(cat.owner_package("acme"), Some("acme_rmw"));
    assert!(matches!(&row.origin, Origin::Package { root: Some(r), .. } if r == "root0"));

    let mut stated = BTreeMap::new();
    stated.insert("NROS_ACME_POOL_SLOTS".to_string(), "4".to_string());
    let k = kconfig_packages(&cat, &["acme"], &stated);
    assert!(
        k.contains("config NROS_ACME_POOL_SLOTS\n    int\n    default 4\n    default 2\n"),
        "{k}"
    );
    // Not linked: nothing defined.
    assert!(!kconfig_packages(&cat, &[], &stated).contains("config "));
    // A guest's own module gets the prompt back.
    let rows: Vec<&Row> = cat
        .package_rows()
        .filter(|r| r.kind == RowKind::Knob)
        .collect();
    let g = kconfig_rows(&rows, &BTreeMap::new(), true);
    assert!(g.contains("    int \"Acme pool slots\"\n"), "{g}");

    let c = registry_cmake(&cat);
    assert!(
        c.contains("list(APPEND NROS_KNOB_KCONFIG_IDS ACME_POOL_SLOTS)"),
        "{c}"
    );
    assert!(c.contains("set(NROS_KNOB_KCONFIG_SYMBOL_ACME_POOL_SLOTS NROS_ACME_POOL_SLOTS)"));
    assert!(c.contains("set(NROS_KNOB_IMAGE_TABLE_ACME_POOL_SLOTS acme)"));
    assert!(c.contains("set(NROS_KNOB_CANONICAL_ACME_SLOTS NROS_ACME_POOL_SLOTS)"));

    let h = env_header(&cat, "acme");
    assert!(
        h.contains("#define NROS_ENV_NROS_ACME_TRACE \"NROS_ACME_TRACE\""),
        "{h}"
    );
}

#[test]
fn a_build_script_reads_its_own_file_without_the_scan() {
    let tmp = tempfile::tempdir().unwrap();
    let f = write_rows(tmp.path(), ACME_TYPED);
    let (owner, rows) = parse_own_file("acme_rmw", &f, ACME_TYPED).unwrap();
    assert_eq!(owner, "acme");
    assert_eq!(rows.len(), 2);
    // C2 holds without the scan, against the stated word.
    let bad = "owner = \"acme\"\n[[knob]]\nkey = \"zenoh.x\"\n";
    let msg = parse_own_file("acme_rmw", &f, bad).unwrap_err().to_string();
    assert!(
        msg.contains("is not under this package's owner word `acme`"),
        "{msg}"
    );
}

// --- phase-581 W4: the naming rule (RFC-0140 D3, D13 G1) ---------------------------

/// The data file's `[[case]]` corpus, which the gate's self-test runs too
/// (`scripts/lib/config_names.py`): one rule logic per language, one corpus.
#[test]
fn naming_cases_match_the_corpus() {
    let doc: toml::Table = toml::from_str(naming::DATA).unwrap();
    let data = naming::Data::get();
    let c = &doc["case_context"];
    let set = |k: &str| -> std::collections::BTreeSet<String> {
        c[k].as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect()
    };
    let ctx = naming::Context {
        owners: set("owners"),
        env_names: set("env_names"),
        retired: set("retired"),
        resources: set("resources"),
    };
    let cases = doc["case"].as_array().unwrap();
    assert!(cases.len() >= 8, "the corpus holds a case per rule");
    let mut seen = std::collections::BTreeSet::new();
    for case in cases {
        let s = |k: &str| {
            case.get(k)
                .and_then(toml::Value::as_str)
                .map(str::to_string)
        };
        let kind = s("kind").unwrap();
        let key = s("key");
        let subject = s("name").or_else(|| key.clone()).unwrap();
        let spellings: Vec<(String, String)> = match case.get("spellings") {
            Some(v) => v
                .as_array()
                .unwrap()
                .iter()
                .map(|x| {
                    let (r, n) = x.as_str().unwrap().split_once(':').unwrap();
                    (r.to_string(), n.to_string())
                })
                .collect(),
            None => {
                let k = key.clone().unwrap_or_default();
                let id = derive_id(&k);
                match kind.as_str() {
                    "knob" => vec![
                        ("toml".into(), k.clone()),
                        ("env".into(), format!("NROS_{id}")),
                        ("kconfig".into(), format!("CONFIG_NROS_{id}")),
                        ("cmake".into(), format!("NROS_OVERRIDE_{id}")),
                    ],
                    "statement" => vec![
                        ("toml".into(), k.clone()),
                        ("env".into(), format!("NROS_ENTRY_{id}")),
                    ],
                    "env" => vec![("run".into(), subject.clone())],
                    _ => vec![("kconfig".into(), subject.clone())],
                }
            }
        };
        let subj = naming::Subject {
            kind,
            subject: subject.clone(),
            key,
            spellings,
            foreign: case.get("foreign").and_then(toml::Value::as_bool) == Some(true),
        };
        let got: Vec<&str> = naming::violations(&subj, &ctx, data)
            .into_iter()
            .map(|(r, _)| r)
            .collect();
        let want: Vec<&str> = case["expect"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(got, want, "[[case]] {subject}");
        seen.extend(got);
    }
    assert_eq!(
        seen.into_iter().collect::<Vec<_>>(),
        naming::RULES,
        "a case per rule"
    );
}

/// The in-tree catalog loads: every in-tree row that breaks the rule is one
/// the baseline holds (RFC-0140 Appendix A).
#[test]
fn naming_the_in_tree_catalog_breaks_only_what_the_baseline_holds() {
    let cat = Catalog::core();
    let ctx = naming::Context::of(cat.rows(), cat.owners.keys().cloned());
    let bad = naming::unheld(cat.rows(), &ctx);
    assert!(bad.is_empty(), "{}", bad.join("\n"));
}

/// The baseline's row entries are exactly what the lint finds: an entry, or
/// a rule of one, that no longer applies is a stale exemption. The gate
/// checks the same on the Python side, so the two agree on every in-tree row.
#[test]
fn naming_every_baseline_row_entry_is_a_live_violation() {
    let cat = Catalog::core();
    let ctx = naming::Context::of(cat.rows(), cat.owners.keys().cloned());
    let data = naming::Data::get();
    let mut found = BTreeMap::new();
    for r in cat.rows().iter().filter(|r| naming::judged(r)) {
        let s = naming::Subject::of_row(r);
        let rules: std::collections::BTreeSet<String> = naming::violations(&s, &ctx, data)
            .into_iter()
            .map(|(r, _)| r.to_string())
            .collect();
        if !rules.is_empty() {
            found.insert((s.kind, s.subject), rules);
        }
    }
    assert_eq!(found, data.baseline);
}

fn naming_refusal(text: &str) -> String {
    let tmp = tempfile::tempdir().unwrap();
    write_rows(tmp.path(), text);
    Catalog::build(&[provider(tmp.path(), "acme_rmw")], &read)
        .unwrap_err()
        .to_string()
}

/// A downstream package with a bad name fails to load, naming the row and
/// the rule. (N3 cannot be broken by a package: C2 already holds its keys
/// under its own owner word.)
#[test]
fn naming_a_package_breaking_the_rule_fails_to_load_naming_row_and_rule() {
    let cases = [
        (
            "N1",
            "knob `acme.deep.name`",
            "[[knob]]\nkey = \"acme.deep.name\"\n",
        ),
        (
            "N2",
            "knob `acme.symbol`",
            "[[knob]]\nkey = \"acme.symbol\"\ntransitional = { id = \"ACME_SYM\" }\n",
        ),
        (
            "N4",
            "knob `acme.ring_size`",
            "[[knob]]\nkey = \"acme.ring_size\"\n",
        ),
        (
            "N5",
            "knob `acme.trace`",
            "[[knob]]\nkey = \"acme.trace\"\n\n[[env]]\nkey = \"acme.trace\"\n",
        ),
        (
            "N6",
            "env `NROS_ACME_LIB`",
            "[[env]]\nname = \"NROS_ACME_LIB\"\nforeign = \"acme\"\n",
        ),
        (
            "N7",
            "knob `acme.old`",
            "[[knob]]\nkey = \"acme.new\"\nretired = [{ spelling = \"env:NROS_ACME_OLD\" }]\n\n\
             [[knob]]\nkey = \"acme.old\"\n",
        ),
        (
            "N8",
            "env `FREERTOS_DIR`",
            "[[env]]\nname = \"FREERTOS_DIR\"\nforeign = \"acme\"\n",
        ),
    ];
    for (rule, row, rows) in cases {
        let msg = naming_refusal(&format!("owner = \"acme\"\n\n{rows}"));
        assert!(msg.contains("breaks the naming rule"), "{rule}: {msg}");
        assert!(
            msg.lines()
                .any(|l| l.contains(row) && l.contains(&format!(": {rule} - "))),
            "{rule} on {row}: {msg}"
        );
    }
    // A package's build script refuses the same, without the scan.
    let tmp = tempfile::tempdir().unwrap();
    let text = "owner = \"acme\"\n[[knob]]\nkey = \"acme.heap_kb\"\n";
    let f = write_rows(tmp.path(), text);
    let msg = parse_own_file("acme_rmw", &f, text)
        .unwrap_err()
        .to_string();
    assert!(
        msg.contains("knob `acme.heap_kb`") && msg.contains(": N4 - "),
        "{msg}"
    );
}
