//! The ONE entry renderer for every pack whose context is `LoweredEntry`
//! (phase-474 W3/W4, RFC-0091 §6b).
//!
//! There used to be one Rust emitter per entry language — `emit_c.rs` (340
//! non-test lines, 4 view structs) and `emit_cpp.rs` (797, 7) — each building
//! its own projection of the plan. They are gone. What a language contributes
//! now is DATA in its `pack.toml` plus, only if it needs a spelling nobody
//! provides yet, a row in `filters.rs`:
//!
//! 1. **Routing** — which pack renders a language on a board
//!    ([`pack::entry_pack_for`](super::pack::entry_pack_for)).
//! 2. **Admission** — what the pack declares it can do, checked against the
//!    plan BEFORE lowering: the component kinds it constructs, whether it
//!    needs the board's C-ABI runners, whether it renders a probe tail.
//! 3. **Lowering** — [`lower_image`], shared by every pack.
//! 4. **Rendering** — the pack's `entry_template`, with the `LoweredEntry` as
//!    its context.
//!
//! None of the four names a language.

use nros_entry_lower::{ComponentKind, LoweredEntry};

use super::{
    Lang, Plan,
    lower::{LowerOptions, component_kind, lower_image},
    pack::{PackContext, PackManifest, entry_pack_for, manifests},
};
use crate::orchestration::model_ingest::{AgeRow, MonitorRow};

pub use super::lower::ProbeExport;

/// Render `plan` as a typed entry in `language`, through the pack that
/// renders it on the plan's board.
pub fn emit_typed(language: Lang, plan: &Plan) -> Result<String, String> {
    emit(language, plan, &LowerOptions::default())
}

/// [`emit_typed`] plus the contract monitor rows (phase-462 W1).
pub fn emit_typed_monitored(
    language: Lang,
    plan: &Plan,
    monitors: &[MonitorRow],
    ages: &[AgeRow],
) -> Result<String, String> {
    emit(
        language,
        plan,
        &LowerOptions {
            monitors,
            ages,
            probe: None,
        },
    )
}

/// phase-308 W1 — the metadata probe: the same TU an entry would be, with a
/// recording tail. Rendered by THE pack that declares `metadata_probe` (the
/// C++ one today), whatever the component's language, because the probe is a
/// HOST binary that constructs the component through whichever seam it has.
/// Found by its declaration rather than named here, so moving the probe tail
/// to another pack is a manifest edit.
pub fn emit_typed_probe(plan: &Plan, export: &ProbeExport) -> Result<String, String> {
    let all = manifests()?;
    let mut probes = all.iter().filter(|(_, m)| m.metadata_probe);
    let (pack, manifest) = probes
        .next()
        .ok_or("no entry pack declares `metadata_probe`")?;
    if let Some((other, _)) = probes.next() {
        return Err(format!(
            "entry packs `{pack}` and `{other}` both declare `metadata_probe` — the \
             probe would render through whichever sorts first"
        ));
    }
    render_pack(
        pack,
        manifest,
        &all,
        plan,
        &LowerOptions {
            probe: Some(export),
            ..Default::default()
        },
    )
}

/// Route, admit, lower, render.
pub fn emit(language: Lang, plan: &Plan, opts: &LowerOptions<'_>) -> Result<String, String> {
    let info =
        entry_pack_for(language, &plan.board).map_err(|e| format!("typed entry emit: {e}"))?;
    let all = manifests()?;
    let manifest = all
        .get(info.pack.as_str())
        .ok_or_else(|| format!("no entry pack `{}`", info.pack))?;
    render_pack(&info.pack, manifest, &all, plan, opts)
}

/// Admit `plan` against `manifest`, lower it, and render the pack's entry
/// template — the whole of what a `LoweredEntry` pack needs from Rust.
///
/// `all` is every pack, for one purpose: a refused component names the packs
/// that WOULD construct it, read from their manifests rather than written here.
pub(crate) fn render_pack(
    pack: &str,
    manifest: &PackManifest,
    all: &std::collections::BTreeMap<&'static str, PackManifest>,
    plan: &Plan,
    opts: &LowerOptions<'_>,
) -> Result<String, String> {
    let lowered = admit_and_lower(pack, manifest, all, plan, opts)?;
    let template = manifest
        .entry_template
        .as_deref()
        .ok_or_else(|| format!("entry pack `{pack}` declares no entry_template"))?;
    super::render::render(template, &lowered)
}

/// Steps 2 and 3, split out so a pack loaded from outside the bundle (the
/// phase-474 W5 data-only pack) is admitted and lowered by the SAME code.
pub(crate) fn admit_and_lower(
    pack: &str,
    manifest: &PackManifest,
    all: &std::collections::BTreeMap<&'static str, PackManifest>,
    plan: &Plan,
    opts: &LowerOptions<'_>,
) -> Result<LoweredEntry, String> {
    if manifest.context != Some(PackContext::LoweredEntry) {
        return Err(format!(
            "entry pack `{pack}` does not render a LoweredEntry (its context is {:?}), \
             so `nros codegen entry` cannot drive it",
            manifest.context
        ));
    }
    let family = nros_entry_lower::board_family(&plan.board)
        .map_err(|e| format!("typed entry emit: {e}"))?;

    // The pack calls the board's C-ABI runners. The routing sends such a
    // language elsewhere on a family that exports none, so this is reached
    // only when a caller names the pack directly — and then it must refuse
    // rather than render a call to a symbol the target does not have (the
    // `c_freertos_one` golden once recorded exactly that).
    if manifest.c_abi_runners && !family.has_c_run_components() {
        return Err(format!(
            "typed entry emit (`{pack}` pack): board `{}` has no C-ABI `run_components` \
             (`BoardFamily::c_abi_runners`), so the `{pack}` pack has no runner to call. \
             `nros codegen entry-pack --board {}` reports which pack renders it.",
            plan.board, plan.board,
        ));
    }

    for n in &plan.nodes {
        let kind = component_kind(n);
        if !manifest.components.contains(&kind) {
            return Err(refuse_component(pack, manifest, all, n, kind));
        }
    }

    if opts.probe.is_some() && !manifest.metadata_probe {
        return Err(format!(
            "entry pack `{pack}` does not render a metadata-probe tail (`metadata_probe`)"
        ));
    }

    let lowered = lower_image(plan, opts)?;

    if manifest.c_abi_runners
        && lowered.tiers.is_some()
        && lowered
            .boot
            .as_ref()
            .is_some_and(|b| b.runners.run_tiers.is_none())
    {
        return Err(format!(
            "typed entry emit (`{pack}` pack): board `{}` has a C-ABI `run_components` \
             but no C-ABI `run_tiers`, so a tiered entry has no runner to call",
            plan.board
        ));
    }
    Ok(lowered)
}

/// The refusal for a node the pack cannot construct, naming what can.
fn refuse_component(
    pack: &str,
    manifest: &PackManifest,
    all: &std::collections::BTreeMap<&'static str, PackManifest>,
    n: &super::PlanNode,
    kind: ComponentKind,
) -> String {
    let accepted = manifest
        .components
        .iter()
        .map(|k| format!("`{}`", k.as_str()))
        .collect::<Vec<_>>()
        .join(" or ");
    let others: Vec<String> = all
        .iter()
        .filter(|(name, m)| {
            **name != pack
                && m.context == Some(PackContext::LoweredEntry)
                && m.components.contains(&kind)
        })
        .filter_map(|(_, m)| m.language.map(|l| format!("`--lang {l}`")))
        .collect();
    let hint = if others.is_empty() {
        String::new()
    } else {
        format!(
            " (use {} for a `{}` component)",
            others.join(" or "),
            kind.as_str()
        )
    };
    format!(
        "typed entry emit (`{pack}` pack): node pkg `{}` exec `{}` is lang `{}` (a `{}` \
         component), not {accepted} — the `{pack}` entry pack constructs {accepted} \
         components only{hint}",
        n.pkg,
        n.exec,
        n.lang.map_or("<unset>", Lang::as_str),
        kind.as_str(),
    )
}

#[cfg(test)]
mod tests_c;
#[cfg(test)]
mod tests_cpp;
#[cfg(test)]
mod tests_data_pack;
