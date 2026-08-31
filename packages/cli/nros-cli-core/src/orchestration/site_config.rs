//! phase-351 W1 — the SITE half of a BOARD.
//!
//! It was the site half of a DEPLOY TARGET until issue 0951. Issue 0842 had
//! already named that as a hazard in its title — "site config keys on deploy
//! target, not board" — for a workspace with two FreeRTOS boards whose site
//! block was reachable from the wrong one. Its root cause turned out to lie
//! elsewhere, so the keying survived; this is the fix it described.
//!
//! [RFC-0072](../../../../../docs/design/0072-rtos-integration-nano-ros-is-a-guest.md)
//! §5. Board information splits three ways:
//!
//! * **A — board facts** (`nros-board.toml`): identity, platform, target,
//!   capabilities, entry shape, toolchain file. Reusable, shippable by us, a
//!   vendor, or the user.
//! * **B — site config** (here): where *this* project's SDK lives, which
//!   network stack it chose, where its config headers are, how it flashes.
//! * **C — test harness**: how *we* run a payload. Neither of the above.
//!
//! B lives in `[board_config.<board>]` of the bringup's `system.toml`. It is
//! keyed by BOARD because that is what the fact is about: 30 authored blocks
//! held exactly 3 distinct value-sets, and the 25 duplicates existed only
//! because the old `[deploy.<name>.nros]` key was sometimes the friendly name
//! and sometimes the board spelling (issue 0951). No new
//! file: a second location would undercut the one-file debuggability the split
//! exists to provide.
//!
//! The spec crate (`ros-launch-manifest`, v0.1.6) carries the block as an
//! opaque `toml::Value` and does not interpret it — RTOS SDK paths are not a
//! launch-manifest's vocabulary, and declaring them there would mean a spec
//! release per key. This module applies nano-ros's schema to that value,
//! including its own `deny_unknown_fields`.

use std::collections::BTreeMap;

use serde::Deserialize;

/// `[board_config.<board>]` — the site half of one board.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SiteConfig {
    /// Which network stack this deployment uses (`lwip`, `freertos_plus_tcp`,
    /// `netxduo`, …).
    ///
    /// A FACT the user states, not a choice nano-ros makes: every vendor has
    /// already welded its stack. The board package declares which stacks it
    /// supports and the resolver checks membership (W4) — the pairing has a
    /// validity domain, because NetX Duo ships a smaller port table than
    /// ThreadX (24 arches against 47).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub netstack: Option<String>,

    /// SDK roots by name — `freertos`, `lwip`, `cube`, `threadx`, `netxduo`,
    /// `nuttx`, `idf`, `px4`. Values may interpolate `{env:VAR}`.
    ///
    /// Machine paths, which is why they are env-interpolated rather than
    /// written literally: the DECISION (which stack, which board) is worth
    /// committing and reviewing; the LOCATION is not the same in two checkouts.
    #[serde(default)]
    pub sdk: BTreeMap<String, String>,

    /// Config headers this project owns, by role.
    ///
    /// A NAMED MAP rather than a directory, because one deployment can need
    /// several with different roles: ThreadX wants `TX_USER_FILE` *and*
    /// `NX_USER_FILE`, FreeRTOS+TCP wants `FreeRTOSConfig.h` *and*
    /// `FreeRTOSIPConfig.h`. A single `config_dir` cannot express that, and
    /// guessing filenames from a directory is how the wrong header gets picked.
    #[serde(default)]
    pub config_files: BTreeMap<String, String>,

    /// Include directories nano-ros must compile its own C against.
    ///
    /// Stated, never derived. ST's Cortex-M7 examples really do use the
    /// `ARM_CM4F` port directory, and two different files are both named
    /// `cmsis_os.h` — so `-I` order decides silently unless the set is written
    /// out. May interpolate `{env:VAR}` and `{sdk.<name>}`.
    #[serde(default)]
    pub include_dirs: Vec<String>,

    /// Preprocessor defines for the same compilation.
    #[serde(default)]
    pub defines: Vec<String>,

    /// Flashing INSTANCE parameters — port, host, probe serial.
    ///
    /// The *mechanism* (ST-LINK, DFU, `rsync` to a Pi) is a board fact and
    /// lives in the board package; only the parameters are the project's. PX4
    /// draws the same line: `cmake/upload.cmake` sits in the board directory
    /// while `AUTOPILOT_HOST` is supplied per invocation.
    #[serde(default)]
    pub upload: BTreeMap<String, String>,
}

/// Which rung of the ladder supplied a value.
///
/// The point of the SSoT is debuggability, and a file that cannot say where a
/// value came from only relocates the mystery. Mirrors RFC-0049's `KnobSource`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SiteSource {
    /// An authored site block — `[board_config.<board>]`.
    Config,
    /// Interpolated from the environment.
    Env,
}

impl SiteSource {
    pub fn as_str(self) -> &'static str {
        match self {
            SiteSource::Config => "config",
            SiteSource::Env => "env",
        }
    }
}

/// One resolved value plus where it came from.
#[derive(Debug, Clone, PartialEq)]
pub struct Resolved {
    pub value: String,
    pub source: SiteSource,
    /// `system.toml` path and the section, for `nros config explain`.
    pub origin: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SiteError {
    #[error("{origin}: [{section}] is not valid: {source}")]
    Parse {
        origin: String,
        section: String,
        /// Boxed because `toml::de::Error` is ~144 bytes on its own, which made
        /// every `Result<_, SiteError>` in this module trip clippy's
        /// `result_large_err` under rust 1.96. The error path is cold; the Ok
        /// path is what should stay small.
        source: Box<toml::de::Error>,
    },
    #[error(
        "{origin}: [{section}] refers to {reference}, which is not set.\n  \
         `{{env:VAR}}` reads the environment; `{{sdk.NAME}}` reads a key of the same \
         block's `sdk` table."
    )]
    Unresolved {
        origin: String,
        section: String,
        reference: String,
    },
}

impl SiteConfig {
    /// Parse an authored site block with nano-ros's schema.
    ///
    /// `origin` is the `system.toml` path, carried only so an error names the
    /// file rather than the value.
    pub fn from_value(value: &toml::Value, section: &str, origin: &str) -> Result<Self, SiteError> {
        value
            .clone()
            .try_into::<SiteConfig>()
            .map_err(|source| SiteError::Parse {
                origin: origin.to_string(),
                section: section.to_string(),
                source: Box::new(source),
            })
    }

    /// Resolve `{env:VAR}` and `{sdk.NAME}` in one string.
    ///
    /// `sdk` values themselves may use `{env:…}` but not `{sdk.…}` — one level,
    /// so a cycle is impossible by construction rather than by detection.
    pub fn interpolate(
        &self,
        raw: &str,
        section: &str,
        origin: &str,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Result<Resolved, SiteError> {
        let mut out = String::with_capacity(raw.len());
        let mut rest = raw;
        let mut used_env = false;

        while let Some(open) = rest.find('{') {
            out.push_str(&rest[..open]);
            let after = &rest[open + 1..];
            let Some(close) = after.find('}') else {
                // A lone `{` is literal; a path may legitimately contain one.
                out.push('{');
                rest = after;
                continue;
            };
            let token = &after[..close];
            let replacement = if let Some(var) = token.strip_prefix("env:") {
                used_env = true;
                env(var)
            } else if let Some(name) = token.strip_prefix("sdk.") {
                match self.sdk.get(name) {
                    // One level: an sdk value may reference env, never sdk.
                    Some(v) => {
                        let (resolved, hit_env) =
                            resolve_env_only(v, env).ok_or_else(|| SiteError::Unresolved {
                                origin: origin.to_string(),
                                section: section.to_string(),
                                reference: v.clone(),
                            })?;
                        // An sdk value that itself reads the environment makes
                        // the RESULT env-derived. Reporting `config` here would
                        // tell the reader the value is committed when it varies
                        // per machine — the opposite of what explain is for.
                        used_env |= hit_env;
                        Some(resolved)
                    }
                    None => None,
                }
            } else {
                // Not our syntax — pass through untouched.
                out.push('{');
                out.push_str(token);
                out.push('}');
                rest = &after[close + 1..];
                continue;
            };
            let Some(replacement) = replacement else {
                return Err(SiteError::Unresolved {
                    origin: origin.to_string(),
                    section: section.to_string(),
                    reference: format!("{{{token}}}"),
                });
            };
            out.push_str(&replacement);
            rest = &after[close + 1..];
        }
        out.push_str(rest);

        Ok(Resolved {
            value: out,
            source: if used_env {
                SiteSource::Env
            } else {
                SiteSource::Config
            },
            origin: format!("{origin} [{section}]"),
        })
    }
}

/// Returns the resolved string and whether any `{env:…}` was substituted.
fn resolve_env_only(raw: &str, env: &dyn Fn(&str) -> Option<String>) -> Option<(String, bool)> {
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    let mut hit = false;
    while let Some(open) = rest.find("{env:") {
        out.push_str(&rest[..open]);
        let after = &rest[open + 5..];
        let close = after.find('}')?;
        out.push_str(&env(&after[..close])?);
        hit = true;
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    Some((out, hit))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: BTreeMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k: &str| map.get(k).cloned()
    }

    fn parse(toml_src: &str) -> SiteConfig {
        let v: toml::Value = toml_src.parse().expect("valid toml");
        SiteConfig::from_value(&v, "freertos", "system.toml").expect("valid site block")
    }

    #[test]
    fn the_freertos_shape_round_trips() {
        let c = parse(
            r#"
            netstack = "lwip"
            sdk = { freertos = "{env:FREERTOS_DIR}", lwip = "{env:LWIP_DIR}" }
            config_files = { freertos = "boards/mps2/FreeRTOSConfig.h", lwip = "boards/mps2/lwipopts.h" }
            "#,
        );
        assert_eq!(c.netstack.as_deref(), Some("lwip"));
        assert_eq!(c.sdk.len(), 2);
        // A NAMED map: ThreadX needs two, so a directory cannot express it.
        assert_eq!(c.config_files.len(), 2);
    }

    #[test]
    fn env_interpolation_reports_its_source() {
        let c = parse(r#"sdk = { freertos = "{env:FREERTOS_DIR}" }"#);
        let env = env_of(&[("FREERTOS_DIR", "/opt/freertos")]);
        let r = c
            .interpolate(
                "{sdk.freertos}/include",
                "board_config.mps2-an385-freertos",
                "system.toml",
                &env,
            )
            .unwrap();
        assert_eq!(r.value, "/opt/freertos/include");
        assert_eq!(r.source, SiteSource::Env);
        assert!(r.origin.contains("[board_config.mps2-an385-freertos]"));
    }

    /// A literal value is `config`-sourced, so `explain` can distinguish a
    /// committed decision from a machine-dependent one.
    #[test]
    fn a_literal_is_config_sourced() {
        let c = parse(r#"netstack = "lwip""#);
        let env = env_of(&[]);
        let r = c
            .interpolate(
                "boards/mps2/lwipopts.h",
                "board_config.mps2-an385-freertos",
                "system.toml",
                &env,
            )
            .unwrap();
        assert_eq!(r.source, SiteSource::Config);
    }

    /// An unset reference must NAME what is missing. The failure this design
    /// exists to avoid is "no value, defaulted, no diagnostic" (issue 0529).
    #[test]
    fn an_unset_env_var_is_an_error_naming_it() {
        let c = parse(r#"sdk = { cube = "{env:CUBE_PROJECT}" }"#);
        let env = env_of(&[]);
        let e = c
            .interpolate(
                "{sdk.cube}/Core/Inc",
                "board_config.nucleo",
                "my/system.toml",
                &env,
            )
            .unwrap_err()
            .to_string();
        assert!(e.contains("CUBE_PROJECT"), "got: {e}");
        assert!(e.contains("my/system.toml"), "names the file: {e}");
    }

    #[test]
    fn an_unknown_sdk_name_is_an_error() {
        let c = parse(r#"sdk = { freertos = "/opt/frt" }"#);
        let env = env_of(&[]);
        let e = c
            .interpolate("{sdk.typo}/x", "freertos", "system.toml", &env)
            .unwrap_err()
            .to_string();
        assert!(e.contains("sdk.typo"), "got: {e}");
    }

    /// The typo guard: this is why the spec crate carries ONE opaque field
    /// rather than a flattened catch-all — the strictness moves here intact.
    #[test]
    fn an_unknown_key_is_rejected() {
        let v: toml::Value = r#"netstck = "lwip""#.parse().unwrap();
        let e = SiteConfig::from_value(&v, "freertos", "system.toml")
            .unwrap_err()
            .to_string();
        assert!(
            e.contains("netstck") || e.contains("unknown field"),
            "got: {e}"
        );
    }

    /// A brace that is not our syntax passes through — paths and shell
    /// fragments legitimately contain them.
    #[test]
    fn foreign_braces_pass_through() {
        let c = parse(r#"netstack = "lwip""#);
        let env = env_of(&[]);
        let r = c
            .interpolate("${CMAKE_BINARY_DIR}/x", "freertos", "system.toml", &env)
            .unwrap();
        assert_eq!(r.value, "${CMAKE_BINARY_DIR}/x");
    }

    /// An absent block is the common case — every system.toml in the tree today
    /// omits it — and must cost nothing.
    #[test]
    fn an_empty_block_is_valid_and_empty() {
        let c = parse("");
        assert!(c.netstack.is_none() && c.sdk.is_empty() && c.include_dirs.is_empty());
    }
}
