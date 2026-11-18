//! Issue 2714 — a `linux` image's connect facts: the domain and locator its
//! `system.toml` states are the BAKED rung, below `$ROS_DOMAIN_ID` /
//! `$NROS_LOCATOR`, on every road; and the environment overriding a stated
//! value warns at boot, naming both (user ruling).
//!
//! One fixture workspace (`fixtures/connect_facts_ws`), one bringup per
//! language, each with a `stated` image (its own `domain_id` and a `locator`)
//! and an `unstated` one, which states nothing of its own: it inherits
//! `[system] domain_id = 0` (a REQUIRED key, so a domain is always stated, and
//! a stated 0 must not read as "not stated") and no locator. The images run
//! under environments that are unset, equal to the statement, and differing. The boot line
//! (`nros::env::report`) names each field's value AND the rung it came from,
//! so the test compares the precedence, not only the value, and it compares
//! the three languages with each other, so the roads cannot drift apart.
//!
//! No router: the stated locator is a closed port, and every line read here
//! is printed before the session opens.

use std::{process::Command, time::Duration};

use nros_tests::{
    fixtures::{self, RequireFixture},
    output,
    process::ManagedProcess,
    project_root,
};

const WS: &str = "packages/testing/nros-tests/fixtures/connect_facts_ws";

/// A domain and locator no statement uses.
const OTHER_DOMAIN: u32 = 5;
const OTHER_LOCATOR: &str = "tcp/127.0.0.1:19";

/// What one run printed, reduced to what the ruling is about.
#[derive(Debug, PartialEq, Eq, Clone)]
struct Boot {
    domain: String,
    domain_source: String,
    locator: String,
    locator_source: String,
    domain_warning: Option<String>,
    locator_warning: Option<String>,
}

/// What `<lang>_bringup` states, read from the file so the test cannot pass
/// against a fixture that changed under it.
struct Statement {
    /// `stated`'s own domain and locator.
    stated: (u32, String),
    /// `[system] domain_id`, which `unstated` inherits.
    system_domain: u32,
}

fn statement(lang: &str) -> Statement {
    let path = project_root()
        .join(WS)
        .join(format!("src/{lang}_bringup/system.toml"));
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let v: toml::Table =
        toml::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
    let img = &v["image"][&format!("{lang}_stated")];
    let unstated = &v["image"][&format!("{lang}_unstated")];
    assert!(
        v["system"].get("locator").is_none()
            && unstated.get("domain_id").is_none()
            && unstated.get("locator").is_none(),
        "the premise: `unstated` states nothing of its own, and nothing states its locator"
    );
    let system_domain = u32::try_from(
        v["system"]["domain_id"]
            .as_integer()
            .expect("[system] domain_id"),
    )
    .unwrap();
    assert_eq!(
        system_domain, 0,
        "the premise: `unstated` inherits a stated 0"
    );
    let domain = u32::try_from(img["domain_id"].as_integer().expect("a domain")).unwrap();
    assert_ne!(domain, system_domain);
    assert_ne!(domain, OTHER_DOMAIN);
    let locator = img["locator"].as_str().expect("a locator").to_string();
    assert_ne!(locator, OTHER_LOCATOR);
    Statement {
        stated: (domain, locator),
        system_domain,
    }
}

fn binary(lang: &str, image: &str) -> std::path::PathBuf {
    let row = format!("workspace-connect-facts-{lang}-{image}");
    // Image ids are per bringup (`<lang>_<image>`): ids shared across
    // bringups would generate into one entry dir (issue 1582).
    let entry = format!("{lang}_{image}_entry");
    match lang {
        "rust" => fixtures::build_workspace_rust_generated_entry(&row, &entry),
        _ => fixtures::build_workspace_cmake_generated_entry(&row, &entry),
    }
    .require(&format!("prebuilt connect_facts_ws {lang} {image}"))
}

/// Boot `bin` under `env` (`None` removes the variable) and read its lines.
fn boot(bin: &std::path::Path, domain: Option<String>, locator: Option<&str>, label: &str) -> Boot {
    let mut cmd = Command::new(bin);
    match &domain {
        Some(d) => cmd.env("ROS_DOMAIN_ID", d),
        None => cmd.env_remove("ROS_DOMAIN_ID"),
    };
    match locator {
        Some(l) => cmd.env("NROS_LOCATOR", l),
        None => cmd.env_remove("NROS_LOCATOR"),
    };
    let mut p =
        ManagedProcess::spawn_image(cmd, label).unwrap_or_else(|e| panic!("spawn {label}: {e}"));
    let out = p
        .wait_for_output_pattern(output::BOOT_CONFIG_LINE, Duration::from_secs(20))
        .unwrap_or_else(|e| panic!("{label} never printed its boot config line: {e}"));
    let line = out
        .lines()
        .find(|l| l.contains(output::BOOT_CONFIG_LINE))
        .expect("the line just matched");
    // `boot config: domain <n> (<src>), locator <l> (<src>), rmw …`
    let rest =
        &line[line.find(output::BOOT_CONFIG_LINE).unwrap() + output::BOOT_CONFIG_LINE.len()..];
    let mut parts = rest.split(", ");
    let pair = |s: Option<&str>, what: &str| -> (String, String) {
        let s = s.unwrap_or_else(|| panic!("{label}: no {what} in {line:?}"));
        let s = s.strip_prefix(what).unwrap_or(s);
        let (v, src) = s
            .rsplit_once(" (")
            .unwrap_or_else(|| panic!("{label}: no source on {what} in {line:?}"));
        (v.trim().to_string(), src.trim_end_matches(')').to_string())
    };
    let (domain_v, domain_src) = pair(parts.next(), "");
    let (locator_v, locator_src) = pair(parts.next(), "locator ");
    // `<VAR>=<env> overrides system.toml's <key>=<stated>`, from the VAR on.
    let warning = |marker: &str, var: &str| {
        out.lines().find(|l| l.contains(marker)).map(|l| {
            let start = l
                .find(var)
                .unwrap_or_else(|| panic!("{label}: the warning names no `{var}`: {l:?}"));
            l[start..].trim().to_string()
        })
    };
    Boot {
        domain: domain_v,
        domain_source: domain_src,
        locator: locator_v,
        locator_source: locator_src,
        domain_warning: warning(output::ENV_OVERRIDES_DOMAIN, "ROS_DOMAIN_ID="),
        locator_warning: warning(output::ENV_OVERRIDES_LOCATOR, "NROS_LOCATOR="),
    }
}

/// What the ruling says each cell must print, for an image stating
/// `domain` and, when `Some`, `locator`: env > stated > default per field, and
/// a warning exactly where the environment sets a stated field differently.
fn expected(domain: u32, locator: Option<&str>, env: Option<(u32, &str)>) -> Boot {
    let (d, d_src) = match env {
        Some((e, _)) => (e.to_string(), "env"),
        None => (domain.to_string(), "baked"),
    };
    let (l, l_src) = match (env, locator) {
        (Some((_, e)), _) => (e.to_string(), "env"),
        (None, Some(s)) => (s.to_string(), "baked"),
        (None, None) => ("(backend default)".to_string(), "default"),
    };
    let domain_warning = env
        .filter(|(e, _)| *e != domain)
        .map(|(e, _)| format!("ROS_DOMAIN_ID={e} overrides system.toml's domain_id={domain}"));
    let locator_warning = match (env, locator) {
        (Some((_, e)), Some(s)) if e != s => Some(format!(
            "NROS_LOCATOR={e} overrides system.toml's locator={s}"
        )),
        _ => None,
    };
    Boot {
        domain: d,
        domain_source: d_src.to_string(),
        locator: l,
        locator_source: l_src.to_string(),
        domain_warning,
        locator_warning,
    }
}

#[test]
fn a_linux_image_takes_its_stated_connect_facts_below_the_environment_and_warns_when_overridden() {
    let mut per_lang: Vec<(&str, Vec<(String, Boot)>)> = Vec::new();
    for lang in ["c", "cpp", "rust"] {
        let st = statement(lang);
        let stated = binary(lang, "stated");
        let unstated = binary(lang, "unstated");
        let (sd, sl) = (st.stated.0, st.stated.1.as_str());
        let other = (OTHER_DOMAIN, OTHER_LOCATOR);
        let mut cells = Vec::new();
        for (name, bin, domain, locator, env) in [
            ("stated, env unset", &stated, sd, Some(sl), None),
            ("stated, env equal", &stated, sd, Some(sl), Some((sd, sl))),
            ("stated, env differs", &stated, sd, Some(sl), Some(other)),
            (
                "unstated, env unset",
                &unstated,
                st.system_domain,
                None,
                None,
            ),
            (
                "unstated, env set",
                &unstated,
                st.system_domain,
                None,
                Some(other),
            ),
        ] {
            let label = format!("{lang} {name}");
            let got = boot(
                bin,
                env.map(|(d, _)| d.to_string()),
                env.map(|(_, l)| l),
                &label,
            );
            assert_eq!(got, expected(domain, locator, env), "{label}");
            cells.push((name.to_string(), got));
        }
        per_lang.push((lang, cells));
    }
    // One rule on every road: the three languages print the same thing in
    // every cell (the stated values are the same in all three bringups).
    let (first, first_cells) = &per_lang[0];
    for (lang, cells) in &per_lang[1..] {
        assert_eq!(
            cells, first_cells,
            "{lang} and {first} resolve the connect facts differently"
        );
    }
}
