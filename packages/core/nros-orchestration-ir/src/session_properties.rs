//! RFC-0109 D8 (phase-514 W3) — the session properties an image bakes, as
//! the one text the CLI hands its build.
//!
//! A backend whose configuration file the target cannot parse has it lowered
//! on the host (`[rmw.config] format = "session-json5"`), to an ordered list
//! of `(key, value)` session properties. Which file, which keys and which
//! backend are the CLI's business (`nros_cli_core::orchestration::rmw_config`,
//! keyed by the backend's descriptor); a core consumer such as `nros::main!`
//! only receives the list, so it names no backend (RFC-0071).
//!
//! The text is the JSON array `[["key", "value"], …]`, carried in
//! [`ENV`] the way `NROS_IMAGE_NET` carries the image's network: written into
//! the build's settings by `nros sync` / `nros build`, read at expansion time.
//! Absent or empty means the image bakes none.

/// The variable that carries an image's session properties to its build.
pub const ENV: &str = "NROS_SESSION_PROPERTIES";

/// The text for `props`.
#[must_use]
pub fn encode(props: &[(String, String)]) -> String {
    let rows: Vec<[&str; 2]> = props
        .iter()
        .map(|(k, v)| [k.as_str(), v.as_str()])
        .collect();
    serde_json::to_string(&rows).unwrap_or_default()
}

/// The properties `text` carries; empty text carries none.
///
/// # Errors
///
/// The text is not the array [`encode`] writes.
pub fn decode(text: &str) -> Result<Vec<(String, String)>, String> {
    if text.trim().is_empty() {
        return Ok(Vec::new());
    }
    let rows: Vec<[String; 2]> = serde_json::from_str(text)
        .map_err(|e| format!("{ENV}: not a `[[\"key\", \"value\"], …]` list ({e})"))?;
    Ok(rows.into_iter().map(|[k, v]| (k, v)).collect())
}

/// The first endpoint the session dials or listens on, whose scheme is the
/// image's link (RFC-0109 D6) — the role a locator plays without a file.
#[must_use]
pub fn endpoint(props: &[(String, String)]) -> Option<&str> {
    props
        .iter()
        .find(|(k, _)| k == "connect" || k == "listen")
        .map(|(_, v)| v.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_text_round_trips_in_order_and_names_its_endpoint() {
        let props = vec![
            ("mode".to_string(), "client".to_string()),
            ("connect".to_string(), "tcp/10.0.2.2:7447".to_string()),
            (
                "connect".to_string(),
                "serial//dev/ttyS0#baudrate=115200".to_string(),
            ),
        ];
        let text = encode(&props);
        assert_eq!(decode(&text).unwrap(), props);
        assert_eq!(endpoint(&props), Some("tcp/10.0.2.2:7447"));
        assert_eq!(decode("").unwrap(), Vec::new());
        assert!(decode("mode=client").unwrap_err().contains(ENV));
    }
}
