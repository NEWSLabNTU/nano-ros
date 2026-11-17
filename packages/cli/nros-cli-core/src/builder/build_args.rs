//! One model for the native arguments a user hands `nros build` (RFC-0117
//! D7–D9, phase-568 W1).
//!
//! Every source (`[image.<id>.build] cmake_args`, `--cmake-args=`,
//! `--cargo-args=`, an `@file`) goes through the same two steps, so a
//! source cannot be a way around the allowlist:
//! 1. [`split_posix`]: plain POSIX quoting, with no expansion of any kind;
//! 2. [`parse_cmake`] / [`parse_cargo`]: the D9 allowlist, building typed
//!    arguments or refusing with the setting that owns the option.
//!
//! Not wired to any flag yet. phase-568 W2 adds the sources, W3 the routing.

use std::{collections::BTreeSet, fmt};

/// A validated CMake argument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CmakeArg {
    /// `-D<name>[:<type>]=<value>`.
    Define {
        name: String,
        ty: Option<String>,
        value: String,
    },
    /// `-U<name>`.
    Undefine(String),
    /// `-W<…>`, a warning switch (`-Wno-dev`, `-Werror=deprecated`, …).
    Warning(String),
    /// `--log-level=<level>`.
    LogLevel(String),
    /// `--log-context`.
    LogContext,
    /// `--debug-find`.
    DebugFind,
}

impl CmakeArg {
    /// The argv spelling cmake takes.
    pub fn to_argv(&self) -> String {
        match self {
            CmakeArg::Define {
                name,
                ty: Some(t),
                value,
            } => {
                // owned-define: user (a name `parse_cmake` already validated)
                format!("-D{name}:{t}={value}")
            }
            // owned-define: user (a name `parse_cmake` already validated)
            CmakeArg::Define { name, value, .. } => format!("-D{name}={value}"),
            CmakeArg::Undefine(n) => format!("-U{n}"),
            CmakeArg::Warning(w) => format!("-W{w}"),
            CmakeArg::LogLevel(l) => format!("--log-level={l}"),
            CmakeArg::LogContext => "--log-context".into(),
            CmakeArg::DebugFind => "--debug-find".into(),
        }
    }
}

/// A validated cargo argument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CargoArg {
    /// `--features=<a>,<b>`, one entry per feature.
    Features(Vec<String>),
    NoDefaultFeatures,
    AllFeatures,
    Verbose,
    Quiet,
    Timings,
    Locked,
    Offline,
    Frozen,
}

impl CargoArg {
    /// The argv spelling cargo takes.
    pub fn to_argv(&self) -> String {
        match self {
            CargoArg::Features(f) => format!("--features={}", f.join(",")),
            CargoArg::NoDefaultFeatures => "--no-default-features".into(),
            CargoArg::AllFeatures => "--all-features".into(),
            CargoArg::Verbose => "--verbose".into(),
            CargoArg::Quiet => "--quiet".into(),
            CargoArg::Timings => "--timings".into(),
            CargoArg::Locked => "--locked".into(),
            CargoArg::Offline => "--offline".into(),
            CargoArg::Frozen => "--frozen".into(),
        }
    }
}

/// A refused argument: what was given, why, and where the setting lives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Refusal {
    pub arg: String,
    pub reason: String,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` is refused: {}", self.arg, self.reason)
    }
}

impl std::error::Error for Refusal {}

fn refuse(arg: &str, reason: impl Into<String>) -> Refusal {
    Refusal {
        arg: arg.to_string(),
        reason: reason.into(),
    }
}

/// Split one argument string with plain POSIX quoting.
///
/// `'…'` is literal; inside `"…"` a backslash escapes only `"`, `\`, `$` and
/// `` ` ``; outside quotes a backslash escapes the next character. Whitespace
/// separates words. Nothing is expanded: no `$var`, no `$(…)`, no `~`, no
/// globs. An unbalanced quote, or a trailing backslash, is refused.
pub fn split_posix(s: &str) -> Result<Vec<String>, Refusal> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut in_word = false;
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('\'') => break,
                        Some(x) => cur.push(x),
                        None => return Err(refuse(s, "an unbalanced single quote")),
                    }
                }
            }
            '"' => {
                in_word = true;
                loop {
                    match chars.next() {
                        Some('"') => break,
                        Some('\\') => match chars.next() {
                            Some(x @ ('"' | '\\' | '$' | '`')) => cur.push(x),
                            Some(x) => {
                                cur.push('\\');
                                cur.push(x);
                            }
                            None => return Err(refuse(s, "an unbalanced double quote")),
                        },
                        Some(x) => cur.push(x),
                        None => return Err(refuse(s, "an unbalanced double quote")),
                    }
                }
            }
            '\\' => match chars.next() {
                Some(x) => {
                    in_word = true;
                    cur.push(x);
                }
                None => return Err(refuse(s, "a trailing backslash")),
            },
            c if c.is_whitespace() => {
                if in_word {
                    words.push(std::mem::take(&mut cur));
                    in_word = false;
                }
            }
            c => {
                in_word = true;
                cur.push(c);
            }
        }
    }
    if in_word {
        words.push(cur);
    }
    Ok(words)
}

/// What a `-D` name collides with, if anything.
fn define_refusal(arg: &str, name: &str) -> Option<Refusal> {
    // A knob first, by every spelling its row has: the remedy names its home.
    if let Some(row) = nros_zephyr_build::KNOBS
        .iter()
        .find(|r| r.env == name || r.kconfig == name || r.legacy.contains(&name) || r.id == name)
    {
        let (group, key) = row.image_key.split_once('.').unwrap_or((row.image_key, ""));
        return Some(refuse(
            arg,
            format!(
                "`{name}` is the knob `{}`; set it in system.toml as \
                 `[image.<id>.knobs.{group}] {key} = …`, its one committed place",
                row.env
            ),
        ));
    }
    if name.starts_with("CONFIG_") {
        return Some(refuse(
            arg,
            "a `-DCONFIG_*` sets a Kconfig symbol directly on the west road, \
             around the Kconfig fragments and the knob ladder; put Zephyr-native \
             Kconfig in the image's `conf = [..]` fragment",
        ));
    }
    if let Some(row) = super::owned_defines::owned(name) {
        return Some(refuse(
            arg,
            format!("`nros build` passes `{name}` itself ({})", row.why),
        ));
    }
    None
}

/// The CMake options refused by name, each with what to use instead.
const CMAKE_REFUSED: &[(&str, &str)] = &[
    ("-C", "it runs a CMake script before the configure"),
    ("-P", "it runs a CMake script instead of a configure"),
    ("-G", "the generator is the build's choice"),
    ("-T", "the toolset is the board's choice"),
    ("-A", "the platform is the board's choice"),
    (
        "--toolchain",
        "the toolchain is the board's (`[board] cmake` in its descriptor)",
    ),
    (
        "--preset",
        "a preset would replace the build's own arguments",
    ),
    ("-S", "the source dir is the build's choice"),
    ("-B", "the build dir is the build's choice"),
];

/// Validate split CMake words against the D9 allowlist.
pub fn parse_cmake(words: &[String]) -> Result<Vec<CmakeArg>, Refusal> {
    let mut out = Vec::new();
    let mut it = words.iter().peekable();
    while let Some(w) = it.next() {
        let w = w.as_str();
        if let Some(rest) = w.strip_prefix("-D") {
            let (shown, def) = if rest.is_empty() {
                let v = it
                    .next()
                    .ok_or_else(|| refuse(w, "`-D` with nothing after it"))?;
                (format!("{w} {v}"), v.clone())
            } else {
                (w.to_string(), rest.to_string())
            };
            let (lhs, value) = def
                .split_once('=')
                .ok_or_else(|| refuse(&shown, "a `-D` needs `<name>=<value>`"))?;
            let (name, ty) = match lhs.split_once(':') {
                Some((n, t)) => (n, Some(t.to_string())),
                None => (lhs, None),
            };
            if name.is_empty() {
                return Err(refuse(&shown, "a `-D` with an empty name"));
            }
            if let Some(r) = define_refusal(&shown, name) {
                return Err(r);
            }
            out.push(CmakeArg::Define {
                name: name.to_string(),
                ty,
                value: value.to_string(),
            });
        } else if let Some(rest) = w.strip_prefix("-U") {
            let name = if rest.is_empty() {
                it.next()
                    .ok_or_else(|| refuse(w, "`-U` with nothing after it"))?
                    .clone()
            } else {
                rest.to_string()
            };
            if let Some(r) = define_refusal(w, &name) {
                return Err(r);
            }
            out.push(CmakeArg::Undefine(name));
        } else if let Some(rest) = w.strip_prefix("-W") {
            if rest.is_empty() {
                return Err(refuse(w, "`-W` with nothing after it"));
            }
            out.push(CmakeArg::Warning(rest.to_string()));
        } else if let Some(level) = w.strip_prefix("--log-level=") {
            out.push(CmakeArg::LogLevel(level.to_string()));
        } else if w == "--log-level" {
            let level = it
                .next()
                .ok_or_else(|| refuse(w, "`--log-level` with nothing after it"))?;
            out.push(CmakeArg::LogLevel(level.clone()));
        } else if w == "--log-context" {
            out.push(CmakeArg::LogContext);
        } else if w == "--debug-find" {
            out.push(CmakeArg::DebugFind);
        } else if let Some((opt, why)) = CMAKE_REFUSED.iter().find(|(o, _)| {
            w == *o || (o.len() == 2 && w.starts_with(o)) || w.starts_with(&format!("{o}="))
        }) {
            return Err(refuse(w, format!("`{opt}` is not a build argument: {why}")));
        } else {
            return Err(refuse(
                w,
                "not an allowed CMake argument (allowed: -D, -U, -W*, --log-level, \
                 --log-context, --debug-find)",
            ));
        }
    }
    Ok(out)
}

/// What the cargo checks need to know about the build.
#[derive(Clone, Debug, Default)]
pub struct CargoPolicy {
    /// Features the build derives itself (RMW, platform, capability), from the
    /// backend's `nros-rmw.toml` and `[system] rmw`, the board descriptor's
    /// `platform`, and the board and system declarations.
    pub derived_features: BTreeSet<String>,
    /// The cargo root holds more than one package, so a bare feature name is
    /// ambiguous and must be `<pkg>/<feat>`.
    pub multi_package: bool,
}

/// The cargo options refused by name, each with what to use instead.
const CARGO_REFUSED: &[(&str, &str)] = &[
    (
        "--config",
        "it overrides the build's generated cargo config",
    ),
    ("-Z", "unstable flags change what the build is"),
    ("--target", "the target is the board's"),
    ("--target-dir", "the target dir is the build's"),
    ("--manifest-path", "the manifest is the build's"),
    (
        "--profile",
        "use `--profile=` on `nros build`, or `[image.<id>] profile`",
    ),
    (
        "--release",
        "use `--profile=release` on `nros build`, or `[image.<id>] profile`",
    ),
    (
        "-r",
        "use `--profile=release` on `nros build`, or `[image.<id>] profile`",
    ),
];

fn features(arg: &str, list: &str, policy: &CargoPolicy) -> Result<Vec<String>, Refusal> {
    let mut out = Vec::new();
    for f in list.split([',', ' ']).filter(|f| !f.is_empty()) {
        let feat = match f.split_once('/') {
            Some((_, feat)) => feat,
            None if policy.multi_package => {
                return Err(refuse(
                    arg,
                    format!(
                        "this workspace has more than one package, so write the \
                         feature as `<pkg>/{f}`: a bare name applies to whichever \
                         packages define it"
                    ),
                ));
            }
            None => f,
        };
        if policy.derived_features.contains(feat) {
            return Err(refuse(
                arg,
                format!(
                    "`{feat}` is derived by the build (RMW, platform or capability, \
                     from the descriptors); choose it in system.toml, not as a feature"
                ),
            ));
        }
        out.push(f.to_string());
    }
    Ok(out)
}

/// Validate split cargo words against the D9 allowlist.
pub fn parse_cargo(words: &[String], policy: &CargoPolicy) -> Result<Vec<CargoArg>, Refusal> {
    let mut out = Vec::new();
    let mut it = words.iter();
    while let Some(w) = it.next() {
        let w = w.as_str();
        let arg = match w {
            "--no-default-features" => CargoArg::NoDefaultFeatures,
            "--all-features" => CargoArg::AllFeatures,
            "-v" | "--verbose" => CargoArg::Verbose,
            "-vv" => {
                out.push(CargoArg::Verbose);
                CargoArg::Verbose
            }
            "-q" | "--quiet" => CargoArg::Quiet,
            "--timings" => CargoArg::Timings,
            "--locked" => CargoArg::Locked,
            "--offline" => CargoArg::Offline,
            "--frozen" => CargoArg::Frozen,
            "--features" | "-F" => {
                let list = it
                    .next()
                    .ok_or_else(|| refuse(w, format!("`{w}` with nothing after it")))?;
                CargoArg::Features(features(&format!("{w} {list}"), list, policy)?)
            }
            _ => {
                if let Some(list) = w.strip_prefix("--features=") {
                    CargoArg::Features(features(w, list, policy)?)
                } else if let Some(list) = w.strip_prefix("-F").filter(|l| !l.is_empty()) {
                    CargoArg::Features(features(w, list, policy)?)
                } else if let Some((opt, why)) = CARGO_REFUSED.iter().find(|(o, _)| {
                    w == *o
                        || w.starts_with(&format!("{o}="))
                        || (*o == "-Z" && w.starts_with("-Z"))
                }) {
                    return Err(refuse(w, format!("`{opt}` is not a build argument: {why}")));
                } else {
                    return Err(refuse(
                        w,
                        "not an allowed cargo argument (allowed: --features, \
                         --no-default-features, --all-features, -v, -q, --timings, \
                         --locked, --offline, --frozen)",
                    ));
                }
            }
        };
        out.push(arg);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(s: &str) -> Vec<String> {
        split_posix(s).unwrap()
    }

    fn cmake(s: &str) -> Result<Vec<CmakeArg>, Refusal> {
        parse_cmake(&words(s))
    }

    #[test]
    fn posix_splitting_expands_nothing() {
        assert_eq!(
            words("a 'b c' \"d e\" f\\ g"),
            vec!["a", "b c", "d e", "f g"]
        );
        assert_eq!(
            words("'$(touch x)' \"$HOME\" ~ *"),
            vec!["$(touch x)", "$HOME", "~", "*"]
        );
        assert_eq!(words(r#""a\"b" "c\d""#), vec![r#"a"b"#, r"c\d"]);
        assert_eq!(words("  "), Vec::<String>::new());
        assert_eq!(words("''"), vec![""]);
    }

    #[test]
    fn an_unbalanced_quote_is_refused() {
        for s in ["'a", "\"a", "a\\"] {
            assert!(split_posix(s).is_err(), "{s}");
        }
    }

    #[test]
    fn a_command_substitution_runs_nothing() {
        let dir = std::env::temp_dir().join(format!("nros-build-args-{}", std::process::id()));
        let marker = dir.join("touched");
        let s = format!(
            "'-DX=$(touch {m})' \"-DY=`touch {m}`\"",
            m = marker.display()
        );
        let args = cmake(&s).unwrap();
        assert_eq!(args.len(), 2);
        assert!(!marker.exists());
    }

    #[test]
    fn defines_parse_in_every_spelling() {
        let d = |n: &str, t: Option<&str>, v: &str| CmakeArg::Define {
            name: n.into(),
            ty: t.map(Into::into),
            value: v.into(),
        };
        assert_eq!(
            cmake("-DA=1 -D B=2 -DC:BOOL=ON -DE= -UF -U G").unwrap(),
            vec![
                d("A", None, "1"),
                d("B", None, "2"),
                d("C", Some("BOOL"), "ON"),
                d("E", None, ""),
                CmakeArg::Undefine("F".into()),
                CmakeArg::Undefine("G".into()),
            ]
        );
        assert_eq!(
            cmake("-Wno-dev --log-level=DEBUG --log-level TRACE --log-context --debug-find")
                .unwrap(),
            vec![
                CmakeArg::Warning("no-dev".into()),
                CmakeArg::LogLevel("DEBUG".into()),
                CmakeArg::LogLevel("TRACE".into()),
                CmakeArg::LogContext,
                CmakeArg::DebugFind,
            ]
        );
    }

    #[test]
    fn a_value_holding_a_flag_stays_a_value() {
        let a = cmake("'-DX=--packages-skip foo'").unwrap();
        assert_eq!(a[0].to_argv(), "-DX=--packages-skip foo");
    }

    #[test]
    fn every_refused_cmake_option_is_refused_joined_and_separate() {
        for s in [
            "-C x",
            "-Cx",
            "-P x",
            "-G Ninja",
            "-GNinja",
            "-T v",
            "-A x64",
            "--toolchain f",
            "--toolchain=f",
            "--preset p",
            "--preset=p",
            "-S .",
            "-B b",
            "--fresh",
            "-L",
            "x",
        ] {
            let r = cmake(s);
            assert!(r.is_err(), "{s} was accepted: {r:?}");
        }
    }

    #[test]
    fn a_kconfig_define_is_refused() {
        let r = cmake("-DCONFIG_FOO=y").unwrap_err();
        assert!(r.reason.contains("conf = [..]"), "{r}");
    }

    #[test]
    fn an_owned_name_is_refused() {
        for s in [
            "-DNROS_IMAGE=x",
            "-DCMAKE_TOOLCHAIN_FILE=t",
            "-D CMAKE_BUILD_TYPE=Debug",
            "-UNROS_RMW",
        ] {
            let r = cmake(s).unwrap_err();
            assert!(r.reason.contains("passes"), "{s}: {r}");
        }
    }

    #[test]
    fn a_knob_is_refused_in_every_spelling_pointing_at_its_home() {
        let row = nros_zephyr_build::KNOBS
            .iter()
            .find(|r| !r.legacy.is_empty())
            .expect("a knob with a legacy spelling");
        for name in [row.env, row.kconfig, row.legacy[0]] {
            let r = cmake(&format!("-D{name}=1")).unwrap_err();
            assert!(r.reason.contains("[image.<id>.knobs."), "{name}: {r}");
        }
    }

    #[test]
    fn a_user_override_and_an_ordinary_define_pass() {
        assert!(
            cmake("-DNROS_OVERRIDE_EXECUTOR_MAX_CBS=4 -DCMAKE_EXPORT_COMPILE_COMMANDS=ON").is_ok()
        );
    }

    #[test]
    fn cargo_arguments_parse_and_refuse() {
        let p = CargoPolicy::default();
        let w = |s: &str| words(s);
        assert_eq!(
            parse_cargo(&w("--features a,b -F c --features=d --no-default-features -v -q --timings --locked"), &p)
                .unwrap()
                .len(),
            8
        );
        for s in [
            "--config x",
            "--config=x",
            "-Zbuild-std",
            "-Z x",
            "--target t",
            "--target=t",
            "--target-dir d",
            "--manifest-path m",
            "--profile p",
            "--release",
            "-r",
            "--jobs 4",
        ] {
            assert!(parse_cargo(&w(s), &p).is_err(), "{s} was accepted");
        }
    }

    #[test]
    fn a_derived_feature_is_refused() {
        let p = CargoPolicy {
            derived_features: ["rmw-zenoh".to_string()].into(),
            multi_package: false,
        };
        assert!(parse_cargo(&words("--features rmw-zenoh"), &p).is_err());
        assert!(parse_cargo(&words("--features=app/rmw-zenoh"), &p).is_err());
        assert!(parse_cargo(&words("--features fast"), &p).is_ok());
    }

    #[test]
    fn a_multi_package_workspace_needs_a_qualified_feature() {
        let p = CargoPolicy {
            derived_features: BTreeSet::new(),
            multi_package: true,
        };
        let r = parse_cargo(&words("--features=fast"), &p).unwrap_err();
        assert!(r.reason.contains("<pkg>/fast"), "{r}");
        assert!(parse_cargo(&words("--features=app/fast"), &p).is_ok());
    }
}
