//! Issue 2400 W2b — an image that opts in to a stack RAISE is rebuilt once at
//! its measured floor instead of failing.
//!
//! `[image.<id>] stack_floor = "raise"`. The post-build walk of such an image
//! (`scripts/nros-stack-floor.py --raise`) does not fail when a MEASURED floor
//! is above the stated stack: it records the knob value in the build's
//! report (`<build>/nros/stack-floor/report.json` on the cmake road,
//! `<target dir>/nros-stack-floor/report.json` on the cargo road).
//! [`after_build`] reads it and, when there is a raise, builds ONCE more with
//! the value in the environment: on the cmake road the configure and the
//! build, as `NROS_STACK_FLOOR_RAISE=NAME=VALUE[;...]`; on the cargo road the
//! build and its walk, as the knob itself (`NAME=VALUE`).
//!
//! The raise is never stored. Every opted-in build derives it again from its
//! own walk and passes it fresh. A value kept in the cmake cache would be
//! state that `system.toml` does not show: it would survive an edit that
//! changes the demand, and vanish with a clean build. So the build says what
//! it used, and prints the line a user writes to pin it.
//!
//! Nothing lowers a stack: the cmake configure takes the larger of the stated
//! value and the raise, and on the cargo road a raise exists only where the
//! floor is above the stated stack. An unmeasured image only warns, raised or
//! not. The heap follows the stack on both FreeRTOS roads.
//!
//! FreeRTOS only, for now (the one platform whose boards state a stack). A
//! west image's knob is Kconfig, which this channel does not reach, so it
//! keeps the CHECK.

use std::path::{Path, PathBuf};

use eyre::{Result, WrapErr};

use crate::{
    builder::{handoff::Handoff, plan::Driver},
    cmd::{
        build::ResolvedBuild,
        build_next::BuildOutput,
        stack_floor::{self, StackFloorStep},
    },
};

/// The environment variable a raise reaches the configure in.
pub const RAISE_ENV: &str = "NROS_STACK_FLOOR_RAISE";

/// One knob value a build's walk asked to raise, from its report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raise {
    pub role: String,
    pub stack: u64,
    pub floor: u64,
    pub name: String,
    pub value: u64,
}

/// Give every cmake configure the image's two stack-floor facts, on every
/// configure and empty included, so neither outlives an edit to `system.toml`
/// in the cmake cache: the key its allowance is filed under, and whether it
/// opted in to a raise.
pub fn arm_cmake(plans: &mut [ResolvedBuild]) {
    for p in plans.iter_mut().filter(|p| p.driver == Driver::CMake) {
        let Some(cfg) = p.configure.as_mut() else {
            continue;
        };
        let key = p.stack_floor_image.clone().unwrap_or_default();
        let mode = if p.stack_floor_raise {
            "raise"
        } else {
            "check"
        };
        cfg.args
            .push(format!("-DNROS_IMAGE_STACK_FLOOR_IMAGE={key}").into());
        cfg.args
            .push(format!("-DNROS_IMAGE_STACK_FLOOR_MODE={mode}").into());
    }
}

/// Where the build's walk wrote its report, and the executable it walked.
fn report_and_exe(p: &ResolvedBuild) -> Option<(PathBuf, Option<PathBuf>)> {
    match (&p.output, &p.stack_floor) {
        (
            Some(BuildOutput::CMake {
                build_dir,
                executable,
            }),
            _,
        ) => Some((
            build_dir.join("nros/stack-floor/report.json"),
            executable.as_ref().map(|e| build_dir.join(e)),
        )),
        (_, Some(StackFloorStep::Check(c))) => {
            Some((c.dir.join("report.json"), Some(c.exe.clone())))
        }
        _ => None,
    }
}

/// The raises a report records (`"raise": [...]`).
pub fn parse_raises(text: &str) -> Result<Vec<Raise>> {
    let v: serde_json::Value = serde_json::from_str(text)?;
    let Some(rows) = v.get("raise").and_then(|r| r.as_array()) else {
        return Ok(Vec::new());
    };
    rows.iter()
        .map(|r| {
            let s = |k: &str| {
                r[k].as_str()
                    .map(str::to_string)
                    .ok_or_else(|| eyre::eyre!("a raise has no `{k}`"))
            };
            let n = |k: &str| {
                r[k].as_u64()
                    .ok_or_else(|| eyre::eyre!("a raise has no `{k}`"))
            };
            Ok(Raise {
                role: s("role")?,
                stack: n("stack")?,
                floor: n("floor")?,
                name: s("name")?,
                value: n("value")?,
            })
        })
        .collect()
}

/// The environment value for a set of raises: `NAME=VALUE[;NAME=VALUE]`, the
/// larger value when two roles raise one knob.
pub fn raise_env(raises: &[Raise]) -> String {
    let mut by: std::collections::BTreeMap<&str, u64> = Default::default();
    for r in raises {
        let v = by.entry(&r.name).or_default();
        *v = (*v).max(r.value);
    }
    by.iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(";")
}

/// The raises of a report written for THIS build: a report older than the
/// executable describes an earlier link, and is not read.
fn fresh_raises(report: &Path, exe: Option<&Path>) -> Result<Option<Vec<Raise>>> {
    let Ok(meta) = std::fs::metadata(report) else {
        return Ok(None);
    };
    if let (Some(exe), Ok(rm)) = (exe, meta.modified()) {
        if let Ok(em) = std::fs::metadata(exe).and_then(|m| m.modified()) {
            if rm < em {
                return Ok(None);
            }
        }
    }
    let text = std::fs::read_to_string(report)
        .wrap_err_with(|| format!("reading {}", report.display()))?;
    parse_raises(&text)
        .map(Some)
        .wrap_err_with(|| format!("reading {}", report.display()))
}

/// The handoff with the raise in its environment.
fn with_raise(h: &Handoff, value: &str) -> Handoff {
    let mut h = h.clone();
    h.env.retain(|(k, _)| k != RAISE_ENV);
    h.env.push((RAISE_ENV.into(), value.into()));
    h
}

/// The handoff with each raised knob in its environment, as the variable the
/// board's build script and `option_env!` read. A cargo `[env]` row is never
/// `force`d, so the process environment wins over a value the image states.
fn with_knobs(h: &Handoff, raises: &[Raise]) -> Handoff {
    let mut h = h.clone();
    for kv in raise_env(raises).split(';') {
        if let Some((k, v)) = kv.split_once('=') {
            h.env.retain(|(e, _)| e != k);
            h.env.push((k.into(), v.into()));
        }
    }
    h
}

/// After an opted-in image built: rebuild it once at its measured floor when
/// its walk asked for a raise. `configure` and `handover` run a configure and
/// a build the way the first pass did.
pub fn after_build(
    p: &ResolvedBuild,
    configure: &dyn Fn(&Handoff) -> Result<()>,
    handover: &(dyn Fn(&Handoff) -> Result<()> + Sync),
) -> Result<()> {
    if !p.stack_floor_raise || !matches!(p.driver, Driver::CMake | Driver::Cargo) {
        return Ok(());
    }
    let (Some((report, exe)), Some(hand)) = (report_and_exe(p), p.handoff.as_ref()) else {
        return Ok(());
    };
    let Some(raises) = fresh_raises(&report, exe.as_deref())? else {
        eprintln!(
            "nros build: `{}` opted in to a stack raise, but this build wrote no stack-floor \
             report, so there is nothing to raise from",
            p.qualified
        );
        return Ok(());
    };
    if raises.is_empty() {
        return Ok(());
    }
    let value = raise_env(&raises);
    eprintln!(
        "nros build: `{}`: its measured stack floor is above the stack it states, and it opted in \
         to a raise; building it once more with {value}",
        p.qualified
    );
    match (&p.driver, &p.stack_floor) {
        // Cargo: the knob itself, on the build and on the walk's own cargo
        // rerun, which would otherwise rebuild the image back without it.
        (Driver::Cargo, Some(StackFloorStep::Check(c))) => {
            handover(&with_knobs(hand, &raises))?;
            let mut c = c.clone();
            c.rerun = with_knobs(&c.rerun, &raises);
            stack_floor::check(&p.qualified, &StackFloorStep::Check(c))?;
        }
        (Driver::Cargo, _) => return Ok(()),
        // CMake: the configure takes the larger of the stated value and the
        // raise, and sizes the heap from it.
        _ => {
            if let Some(cfg) = &p.configure {
                configure(&with_raise(cfg, &value))?;
            }
            handover(&with_raise(hand, &value))?;
        }
    }
    match fresh_raises(&report, exe.as_deref())? {
        Some(again) if !again.is_empty() => eyre::bail!(
            "`{}`: rebuilt with {value}, and its walk still asks for {}: the raise did not reach \
             the stack. This is a bug in the raise, not in the image",
            p.qualified,
            raise_env(&again)
        ),
        _ => {}
    }
    for r in &raises {
        eprintln!("{}", raised_message(&p.qualified, p.driver, r));
    }
    Ok(())
}

/// The thread a walker role's stack belongs to, in a user's words: OPEN is
/// the task that registers the nodes and opens the executor, which on
/// FreeRTOS is the app task.
fn thread_of(role: &str) -> String {
    match role {
        "OPEN" => "app task".to_string(),
        other => format!("{} thread", other.to_ascii_lowercase()),
    }
}

/// What a raised build says: the value it used, and how to make it the
/// project's own. A pin is a line in `system.toml`, never a command-line
/// flag; where the road has no such line, it says so.
pub fn raised_message(qualified: &str, driver: Driver, r: &Raise) -> String {
    let image = qualified.rsplit(':').next().unwrap_or(qualified);
    let head = format!(
        "nros build: `{qualified}` was built with its {} stack raised to {}={} (measured floor \
         {} B; the image states {} B). The raise is derived again by every build and stored \
         nowhere.",
        thread_of(&r.role),
        r.name,
        r.value,
        r.floor,
        r.stack
    );
    match driver {
        // A registry knob is written under `[image.<id>.knobs.<group>]`: an
        // `env` row naming it is refused (phase-484 W8). An application's own
        // variable is still an `env` row.
        Driver::Cargo => match nros_zephyr_build::KNOBS.iter().find(|k| k.env == r.name) {
            Some(k) => {
                let (group, name) = k.image_group_name();
                format!(
                    "{head} To make it the image's own, write in system.toml:\n    \
                     [image.{image}.knobs.{group}]\n    {name} = {}",
                    r.value
                )
            }
            None => format!(
                "{head} To make it the image's own, write in system.toml:\n    [image.{image}]\n    \
                 env = {{ {} = \"{}\" }}",
                r.name, r.value
            ),
        },
        // Issue 2496: the cmake road's carrier reads this knob as a cmake
        // variable, which `[image.<id>.knobs]` does not reach (measured), so
        // there is no system.toml line to give. No issue id in the text (ruling A).
        _ => format!(
            "{head} This image's stack cannot be set from system.toml on this road yet; to set \
             it yourself, pass it at configure: -D{}={}",
            r.name, r.value
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_report_lists_its_raises_and_an_old_report_none() {
        let text = r#"{"schema":1,"raise":[{"role":"OPEN","stack":65536,"floor":70000,
            "knob":"NROS_FREERTOS_APP_STACK_KB=69","name":"NROS_FREERTOS_APP_STACK_KB","value":69}]}"#;
        let r = parse_raises(text).unwrap();
        assert_eq!(
            r,
            vec![Raise {
                role: "OPEN".into(),
                stack: 65536,
                floor: 70000,
                name: "NROS_FREERTOS_APP_STACK_KB".into(),
                value: 69,
            }]
        );
        // A report from before W2b has no `raise` field: nothing to raise.
        assert!(parse_raises(r#"{"schema":1}"#).unwrap().is_empty());
        assert!(parse_raises(r#"{"raise":[{"role":"OPEN"}]}"#).is_err());
    }

    #[test]
    fn a_raised_build_names_the_value_and_a_pin_only_where_one_exists() {
        let r = Raise {
            role: "OPEN".into(),
            stack: 8192,
            floor: 15216,
            name: "NROS_FREERTOS_APP_STACK_KB".into(),
            value: 15,
        };
        let cargo = raised_message("b:mps2-an385-freertos", Driver::Cargo, &r);
        assert!(
            cargo.contains("app task stack raised to NROS_FREERTOS_APP_STACK_KB=15"),
            "{cargo}"
        );
        assert!(
            cargo.contains("[image.mps2-an385-freertos.knobs.freertos]\n    app_stack_kb = 15"),
            "{cargo}"
        );
        let cmake = raised_message("b:mps2-an385-freertos", Driver::CMake, &r);
        assert!(
            cmake.contains("cannot be set from system.toml")
                && cmake.contains("-DNROS_FREERTOS_APP_STACK_KB=15"),
            "{cmake}"
        );
        // A user reads this: no issue, phase or RFC id in it, on either road.
        for text in [&cargo, &cmake] {
            let lower = text.to_ascii_lowercase();
            for id in ["issue ", "phase-", "phase ", "rfc-", "rfc ", " #"] {
                assert!(
                    !lower
                        .split(id)
                        .skip(1)
                        .any(|t| t.starts_with(|c: char| c.is_ascii_digit())),
                    "names an id (`{id}<n>`): {text}"
                );
            }
        }
    }

    #[test]
    fn one_knob_raised_by_two_roles_takes_the_larger() {
        let r = |role: &str, name: &str, value| Raise {
            role: role.into(),
            stack: 0,
            floor: 0,
            name: name.into(),
            value,
        };
        assert_eq!(
            raise_env(&[r("OPEN", "K", 30), r("SPIN", "K", 40), r("OPEN", "A", 2)]),
            "A=2;K=40"
        );
    }

    #[test]
    fn the_cargo_road_hands_the_knob_itself_replacing_an_older_value() {
        let r = |name: &str, value| Raise {
            role: "OPEN".into(),
            stack: 0,
            floor: 0,
            name: name.into(),
            value,
        };
        let h = Handoff::new("cargo", ["build"]).with_env("NROS_FREERTOS_APP_STACK_KB", "16");
        let h = with_knobs(&h, &[r("NROS_FREERTOS_APP_STACK_KB", 26)]);
        let got: Vec<_> = h
            .env
            .iter()
            .filter(|(k, _)| k == "NROS_FREERTOS_APP_STACK_KB")
            .collect();
        assert_eq!(
            got,
            vec![&("NROS_FREERTOS_APP_STACK_KB".into(), "26".into())]
        );
    }

    #[test]
    fn the_raise_replaces_an_older_one_in_the_environment() {
        let h = Handoff::new("cmake", ["--build", "b"]).with_env(RAISE_ENV, "K=1");
        let h = with_raise(&h, "K=2");
        let got: Vec<_> = h.env.iter().filter(|(k, _)| k == RAISE_ENV).collect();
        assert_eq!(got, vec![&(RAISE_ENV.into(), "K=2".into())]);
    }
}
