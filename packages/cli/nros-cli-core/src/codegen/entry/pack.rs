//! phase-432 W3.2 — the entry pack manifests, and the one routing decision.
//!
//! ## What a manifest is for
//!
//! CMake has to know two things about an entry pack BEFORE codegen runs,
//! because they decide the output path and the compiler: the generated TU's
//! extension, and whether a C-family toolchain builds it. It used to derive
//! both itself, in `NanoRosEntry.cmake`:
//!
//! ```text
//! if(_NRX_LANG STREQUAL "c" AND NOT NANO_ROS_PLATFORM STREQUAL "posix")
//!     set(_ext "cpp")   # an embedded C entry is rendered as C++
//! ```
//!
//! while the CLI made the same decision from a different input:
//!
//! ```text
//! Lang::C if !board_is_embedded(&plan.board) => emit_c,
//! _                                          => emit_cpp,
//! ```
//!
//! One keyed on `NANO_ROS_PLATFORM`, the other on the board key `plan.board`
//! carries (from the bringup's `system.toml`). Those are related and not equal.
//!
//! **No live disagreement was found**, and that is worth saying plainly rather
//! than dressing the change up: every (platform, board) pair the tree
//! exercises gives the same answer both ways. `freertos-posix` looks like the
//! counterexample — a HOST process whose board family is `Freertos` — and is
//! not, because its fixture rows set `NANO_ROS_PLATFORM = "freertos"`, so both
//! sides say embedded.
//!
//! What was wrong is structural: nothing MADE them agree. They read different
//! inputs, `board_family` answered `Native` for any key it had not learned, and
//! the failure if they ever diverged is a C++ TU written into a `.c` file — a
//! compile error at best, and at worst a `.c` file that happens to compile.
//! The `freertos-posix` row in `nros_entry_lower::BOARD_KEYS` records that the
//! fallback already produced one silent wrong answer of exactly that kind.
//! Issue 1285 removed the fallback: an unknown key is now an error naming the
//! known keys, which `nros codegen entry-pack` returns and CMake reports.
//!
//! So the manifest holds the DATA and this module holds the DECISION, and both
//! answers are served to CMake through `nros codegen entry-pack` — one
//! producer, asked rather than re-derived.
//!
//! ## What is deliberately NOT in a manifest
//!
//! Which pack renders for a given (language, board) is a rule with a reason —
//! a C entry can be rendered as C only where the board exports a C-ABI
//! `run_components` — and a rule belongs in reviewed Rust, not in data a
//! template author can edit. RFC-0091 draws the same line for target facts.

use std::collections::BTreeMap;

use nros_lang::Language;

/// One pack's manifest, as declared in `packs/entry/<surface>/pack.toml`.
///
/// `deny_unknown_fields` (phase-474): a pack is now DATA the renderer acts on,
/// so a misspelt key — `c_abi_runner` for `c_abi_runners` — must fail the
/// parse rather than silently take the default and render a pack that does
/// not do what its author wrote.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct PackManifest {
    /// Absent for the shared partial pack, which renders no TU of its own.
    #[serde(default)]
    pub language: Option<Language>,
    /// True only for the shared pack. The two kinds are distinguished by a
    /// declared field rather than by a directory name, so a new shared pack
    /// cannot be created by accident.
    #[serde(default)]
    pub shared: bool,
    #[serde(default)]
    pub extension: Option<String>,
    #[serde(default)]
    pub c_family: Option<bool>,
    /// The registry key of the template that renders the whole TU — one of
    /// this pack's own `templates`.
    #[serde(default)]
    pub entry_template: Option<String>,
    /// phase-474 W1 — every template this pack contributes. The build script
    /// generates the registry from these rows, so they cannot disagree with it.
    #[serde(default)]
    pub templates: Vec<TemplateRow>,

    // ---- phase-474 W2 — what the generic renderer needs to know. ----
    /// What the entry template renders FROM. Required of a language pack.
    #[serde(default)]
    pub context: Option<PackContext>,
    /// The spelling filters this pack's templates call, by registry name
    /// (`filters.rs`). Declared rather than assumed, so a pack that needs a
    /// spelling no Rust provides fails a test rather than a user's build.
    #[serde(default)]
    pub filters: Vec<String>,
    /// The component kinds this pack's templates know how to construct. A
    /// plan with a node of any other kind is refused before lowering, naming
    /// the node — the C pack's "not `c`" refusal, generated from data.
    #[serde(default)]
    pub components: Vec<nros_entry_lower::ComponentKind>,
    /// The pack calls the board's C-ABI runners
    /// (`BoardFamily::c_abi_runners`), so it can render only for a family that
    /// exports them — and a language whose pack says so is ROUTED to the C++
    /// pack on a family that does not (`entry_pack_for`).
    #[serde(default)]
    pub c_abi_runners: bool,
    /// The pack renders the phase-308 metadata-probe tail.
    #[serde(default)]
    pub metadata_probe: bool,
    /// A pack that exists only as a test FIXTURE, outside the bundle
    /// (`testdata/entry-packs/`), proving a pack can be added as data alone
    /// (phase-474 W5). It declares no `language`, because naming one would be
    /// a `Language` variant — the one piece of Rust a real language still
    /// adds (RFC-0091 §8 step 3).
    #[serde(default)]
    pub fixture: bool,
}

/// What a pack's entry template renders from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum PackContext {
    /// `nros_entry_lower::LoweredEntry` itself, through the generic renderer
    /// (`emit.rs`). A pack with this context needs no Rust of its own beyond
    /// the filters it declares.
    LoweredEntry,
    /// The Rust parity renderer's own view (`emit_rust.rs`). RFC-0091 §7 keeps
    /// it, by decision: it is the second rendering the parity corpus compares
    /// the `nros::main!` proc-macro against, and the shipping Rust entry is
    /// that proc-macro (issue 0083).
    RustParity,
}

/// One `templates = [{ key, file }]` row.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateRow {
    /// The stable registry name a `render` call or an `{% include %}` uses.
    pub key: String,
    /// The template file, beside the manifest.
    pub file: String,
}

// phase-474 W1 — the manifests are DISCOVERED: `ENTRY_PACK_MANIFESTS` is
// generated by `build_entry_packs.rs` from every `packs/entry/<dir>/pack.toml`,
// so a new pack needs no edit here. Bundled at build time for the same reason
// the templates are: `nros` runs from CMake and from build scripts in trees
// that do not contain this checkout.
use super::render::ENTRY_PACK_MANIFESTS as MANIFESTS;

/// Parse every manifest, keyed by surface directory name.
///
/// A malformed manifest is a bug in a file that ships INSIDE the binary, so it
/// surfaces as a plain message rather than being folded into a caller's error
/// vocabulary — the same contract `render::render` uses.
pub fn manifests() -> Result<BTreeMap<&'static str, PackManifest>, String> {
    let mut out = BTreeMap::new();
    for (surface, text) in MANIFESTS {
        let m: PackManifest = toml::from_str(text)
            .map_err(|e| format!("bundled pack manifest `{surface}/pack.toml` is invalid: {e}"))?;
        out.insert(*surface, m);
    }
    Ok(out)
}

/// What CMake needs to know about the pack that will render this entry.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct EntryPackInfo {
    /// The surface directory whose pack renders it — NOT necessarily the
    /// language asked for: an embedded C entry is rendered by the C++ pack.
    pub pack: String,
    /// The generated TU's extension.
    pub extension: String,
    /// Whether a C-family compiler builds it.
    pub c_family: bool,
    /// True when the requested language is not the rendering pack's, i.e. the
    /// routing rule fired. Reported so a caller can SAY so rather than having
    /// to notice the extension changed.
    pub routed: bool,
}

/// The one routing decision: which pack renders `language` on `board`.
///
/// The rule and its reason: a C entry is rendered by the C pack only where the
/// board family exports a C-ABI `run_components`. Where it does not, the C++
/// pack renders it, driving the C++ board runner and calling each C node
/// through its `extern "C"` seam.
///
/// Since issue 1286 every family has one (ThreadX was the last), so the
/// C-to-C++ arm is reached by no board in `BOARD_KEYS`. It stays as the
/// answer for a future family that ships no C runner, rather than being
/// deleted and re-derived.
pub fn entry_pack_for(language: Language, board: &str) -> Result<EntryPackInfo, String> {
    // phase-432 W3.1 — the question is whether this board HAS a C-ABI
    // `run_components`, not whether it is embedded. Those agreed only while
    // `native` was the only family with one; FreeRTOS has one now, so a
    // family-wide assumption would route a C entry to C++ on a board that no
    // longer needs it. One predicate, in `nros-entry-lower`, because four
    // sites must give the same answer.
    let has_c_runner = nros_entry_lower::board_family(board)
        .map_err(|e| e.to_string())?
        .has_c_run_components();
    let all = manifests()?;
    // phase-474 W2 — a language's pack is the manifest that DECLARES it, not
    // a `match` arm here: a new language is found by its directory. The one
    // routing rule stays Rust, and reads the pack's own declaration: a pack
    // that calls the C-ABI runners cannot render where the family has none,
    // and the C++ pack, which drives the C++ board runner and reaches a C node
    // through its `extern "C"` seam, renders it instead.
    let own = pack_for_language(&all, language)?;
    let pack = if all[own].c_abi_runners && !has_c_runner {
        pack_for_language(&all, Language::Cpp)?
    } else {
        own
    };
    let m = &all[pack];
    let extension = m
        .extension
        .clone()
        .ok_or_else(|| format!("pack `{pack}` declares no extension"))?;
    let c_family = m
        .c_family
        .ok_or_else(|| format!("pack `{pack}` declares no c_family"))?;
    Ok(EntryPackInfo {
        pack: pack.to_string(),
        extension,
        c_family,
        routed: m.language != Some(language),
    })
}

/// The pack whose manifest declares `language` — exactly one, or an error.
///
/// Two packs claiming one language would make the answer depend on directory
/// order; none means the enumeration names a language nothing renders. Both
/// are a manifest mistake, and `check-entry-pack-conformance` reports them on
/// the fast line before this runs.
pub(crate) fn pack_for_language(
    all: &BTreeMap<&'static str, PackManifest>,
    language: Language,
) -> Result<&'static str, String> {
    let mut found = all
        .iter()
        .filter(|(_, m)| m.language == Some(language))
        .map(|(k, _)| *k);
    let first = found
        .next()
        .ok_or_else(|| format!("no entry pack declares language `{language}`"))?;
    if let Some(second) = found.next() {
        return Err(format!(
            "entry packs `{first}` and `{second}` both declare language `{language}`"
        ));
    }
    Ok(first)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every bundled manifest parses, and declares the fields its KIND
    /// requires. A language pack without an extension is a pack CMake cannot
    /// name an output for; a shared pack WITH one is a pack that looks
    /// renderable and is not.
    #[test]
    fn every_manifest_declares_its_kind_completely() {
        let all = manifests().expect("bundled manifests must parse");
        assert_eq!(all.len(), MANIFESTS.len());
        for (surface, m) in &all {
            if m.shared {
                assert!(
                    m.language.is_none()
                        && m.extension.is_none()
                        && m.c_family.is_none()
                        && m.entry_template.is_none(),
                    "shared pack `{surface}` declares a language pack's fields — the \
                     two kinds must not be confusable"
                );
                assert!(
                    !m.templates.is_empty(),
                    "shared pack `{surface}` declares no templates, so it is nothing"
                );
            } else {
                assert!(
                    m.language.is_some()
                        && m.extension.is_some()
                        && m.c_family.is_some()
                        && m.entry_template.is_some()
                        && m.context.is_some(),
                    "language pack `{surface}` is missing a required field — CMake \
                     cannot name its output or pick its compiler, or the renderer \
                     cannot tell what its template reads"
                );
                // A pack the generic renderer drives must say what it can
                // construct; one that constructs nothing renders nothing.
                if m.context == Some(PackContext::LoweredEntry) {
                    assert!(
                        !m.components.is_empty(),
                        "pack `{surface}` renders a LoweredEntry and accepts no component kind"
                    );
                }
            }
        }
        // Every language has exactly one pack.
        for language in Language::ALL {
            pack_for_language(&all, language).unwrap_or_else(|e| panic!("{e}"));
        }
    }

    /// A language pack's `entry_template` is one of ITS OWN templates.
    ///
    /// phase-474 W1 — the registry is now generated from the manifests'
    /// `templates` rows, so "the manifests and the registry describe the same
    /// templates" holds by construction and its test went with the authored
    /// list. What can still disagree is the one key a manifest names OUTSIDE
    /// its rows: an `entry_template` naming another pack's template, or none,
    /// would render the wrong TU or fail at some user's build.
    #[test]
    fn every_entry_template_is_its_own_packs_template() {
        let all = manifests().expect("manifests parse");
        let registered = super::super::render::template_keys();
        for (surface, m) in &all {
            let Some(t) = &m.entry_template else { continue };
            assert!(
                m.templates.iter().any(|row| &row.key == t),
                "pack `{surface}` names `{t}` as its entry template, which is not one of its own `templates`"
            );
            assert!(
                registered.contains(&t.as_str()),
                "`{t}` is not in the registry"
            );
        }
    }

    /// The routing rule, stated as cases rather than re-derived.
    ///
    /// phase-432 W3.1 — the rule is now "does this board have a C-ABI
    /// `run_components`", not "is it embedded". The two agreed while `native`
    /// was the only family with one; FreeRTOS has one now, so the cases split
    /// by CAPABILITY rather than by host-vs-embedded. The board list is taken
    /// from the predicate rather than written out, so a board gaining a runner
    /// moves between the two arms here automatically instead of leaving a
    /// stale literal asserting the old answer.
    #[test]
    fn a_c_entry_renders_as_c_exactly_where_the_board_has_a_c_runner() {
        for board in ["native", "zephyr", "nuttx", "freertos", "threadx"] {
            let has_runner = nros_entry_lower::board_family(board)
                .unwrap()
                .has_c_run_components();
            let got = entry_pack_for(Language::C, board).unwrap();
            if has_runner {
                assert_eq!(
                    (got.pack.as_str(), got.extension.as_str()),
                    ("c", "c"),
                    "`{board}` has a C-ABI run_components, so a C entry must render as C"
                );
                assert!(
                    !got.routed,
                    "`{board}` must not report routing — none fired"
                );
            } else {
                assert_eq!(
                    (got.pack.as_str(), got.extension.as_str()),
                    ("cpp", "cpp"),
                    "`{board}` has no C-ABI run_components, so a C entry must render as \
                     C++ — that pack drives the C++ board runner and reaches each C node \
                     through its `extern \"C\"` seam"
                );
                assert!(got.routed, "`{board}` must report that routing fired");
            }
        }
    }

    /// The state of the surface, pinned so a board's runner landing is a
    /// DELIBERATE edit here rather than a silent change of what ships.
    ///
    /// Issue 1286 moved ThreadX, the last family routed to C++. It has the
    /// shared `run_components` and no `run_tiers`, so a tiered ThreadX C plan
    /// takes the sched-context path rather than being refused.
    #[test]
    fn the_c_board_surface_is_where_this_phase_left_it() {
        use nros_entry_lower::BoardFamily;
        assert!(BoardFamily::Native.has_c_run_components());
        assert!(BoardFamily::Freertos.has_c_run_components());
        assert!(BoardFamily::Zephyr.has_c_run_components());
        assert!(BoardFamily::Nuttx.has_c_run_components());
        assert!(BoardFamily::Threadx.has_c_run_components());
    }

    /// Issue 1286 — both ThreadX board keys CMake passes answer `pack=c`, with
    /// no routing reported. CMake asks this before any plan exists, which is
    /// why the answer cannot depend on whether the plan declares tiers.
    #[test]
    fn a_threadx_c_entry_renders_as_c() {
        for board in ["threadx", "threadx-linux", "rv-virt-threadx"] {
            let got = entry_pack_for(Language::C, board).unwrap();
            assert_eq!(
                (got.pack.as_str(), got.extension.as_str(), got.routed),
                ("c", "c", false),
                "{board}"
            );
        }
    }

    /// A Rust entry is not a C-family TU, so CMake must not link it as one.
    #[test]
    fn a_rust_entry_is_not_c_family() {
        let got = entry_pack_for(Language::Rust, "native").unwrap();
        assert!(!got.c_family);
        assert_eq!(got.extension, "rs");
    }
}
