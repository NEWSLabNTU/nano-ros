//! Reporter — render a [`BuildProfile`] (+ hints) as a terminal table, an
//! optional per-unit drill-down, and a machine-readable JSON document.
//!
//! Formatting is deterministic (fixed widths, ASCII) so the output is
//! golden-testable.

use serde::Serialize;

use crate::model::BuildProfile;

/// Reporter options.
#[derive(Debug, Clone, Copy)]
pub struct Opts {
    /// Show the per-unit drill-down.
    pub deep: bool,
    /// Number of units to list in the drill-down.
    pub top_n: usize,
    /// Render hints.
    pub hints: bool,
}

impl Default for Opts {
    fn default() -> Self {
        Opts {
            deep: false,
            top_n: 8,
            hints: true,
        }
    }
}

/// Render the full text report.
pub fn render(p: &BuildProfile, hints: &[String], opts: Opts) -> String {
    let mut out = String::new();
    out.push_str(&render_header(p));
    out.push('\n');
    out.push_str(&render_table(p));
    if opts.deep {
        out.push('\n');
        out.push_str(&render_deep(p, opts.top_n));
    }
    if opts.hints && !hints.is_empty() {
        out.push('\n');
        out.push_str(&render_hints(hints));
    }
    out
}

fn render_header(p: &BuildProfile) -> String {
    format!("Backend: {:<16} Total: {:.1}s\n", p.backend.label(), p.total_s)
}

fn render_table(p: &BuildProfile) -> String {
    let mut s = format!("{:<10} {:>8} {:>4}\n", "Stage", "Duration", "%");
    for st in &p.stages {
        s.push_str(&format!(
            "{:<10} {:>8} {:>4}\n",
            st.name,
            format!("{:.1}s", st.dur_s),
            format!("{}%", st.pct.round() as i64),
        ));
    }
    s
}

fn render_deep(p: &BuildProfile, top_n: usize) -> String {
    if !p.captured_deep || p.units.is_empty() {
        return "note: no per-unit timing captured (coarse only) \u{2014} for cargo, \
                build with `cargo build --timings`\n"
            .to_string();
    }
    let mut units: Vec<&crate::model::Unit> = p.units.iter().collect();
    units.sort_by(|a, b| b.dur_s.total_cmp(&a.dur_s));
    let shown = units.len().min(top_n);
    let max = units.first().map(|u| u.dur_s).unwrap_or(0.0).max(1e-9);

    let mut s = String::from("slowest units:\n");
    for u in &units[..shown] {
        let bar_len = ((u.dur_s / max) * 8.0).round() as usize;
        let bar = "#".repeat(bar_len.max(if u.dur_s > 0.0 { 1 } else { 0 }));
        s.push_str(&format!("  {:<18} {:>7} {}\n", u.name, format!("{:.1}s", u.dur_s), bar));
    }
    if units.len() > shown {
        let rest: f64 = units[shown..].iter().map(|u| u.dur_s).sum();
        s.push_str(&format!(
            "  {:<18} {:>7}\n",
            format!("<{} more>", units.len() - shown),
            format!("{rest:.1}s")
        ));
    }
    s
}

fn render_hints(hints: &[String]) -> String {
    let mut s = String::from("hints:\n");
    for h in hints {
        s.push_str(&format!("  - {h}\n"));
    }
    s
}

/// JSON document shape (profile fields flattened + hints), for CI diffing.
#[derive(Serialize)]
struct ReportJson<'a> {
    #[serde(flatten)]
    profile: &'a BuildProfile,
    hints: &'a [String],
}

/// Serialize the profile + hints to pretty JSON.
pub fn to_json(p: &BuildProfile, hints: &[String]) -> String {
    serde_json::to_string_pretty(&ReportJson { profile: p, hints })
        .unwrap_or_else(|e| format!("{{\"error\":\"json serialize failed: {e}\"}}"))
}
