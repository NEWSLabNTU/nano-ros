//! `nros search <word>` (RFC-0133 D6, phase-569 W9).
//!
//! `toolchain` is the one D2 noun left out: its `list` loader
//! (`store::scan`) measures each store entry's size by walking the whole SDK
//! store, which is not the cheap read D6 searches.
//!
//! Search keeps no index of its own. It calls each noun's own `list` loader
//! (the one `nros <noun> list` prints from) and matches the word,
//! case-insensitively, against each item's name, its other names and its
//! one-line summary. An item a source gains (a board descriptor, a knob in a
//! package, a runner a workspace announces) is therefore found with no change
//! here; only a new NOUN needs a row in `SOURCES`.
//!
//! Two kinds of row are not matched on their own text:
//! - an RMW's boards: when an RMW matches, the boards its row names (Runtime
//!   tested, then build-only) are listed under `board`, so `nros search
//!   zenoh` answers which boards support it. That is the RMW loader's own
//!   fact, not a second table;
//! - an image's planned nodes and topics, when the image has a plan (a prior
//!   `nros configure`), listed under `node` and `topic`.

use std::path::PathBuf;

use clap::Args as ClapArgs;
use eyre::Result;
use serde::Serialize;

use crate::cmd::frame;

/// The `--json` schema.
pub const SCHEMA: &str = "nros.search.v1";

#[derive(Debug, ClapArgs)]
pub struct Args {
    /// The word to look for, in names, other names and descriptions
    pub word: String,

    /// Print one JSON object instead of a table
    #[arg(long)]
    pub json: bool,

    /// Workspace root. Defaults to the one the current directory is in
    #[arg(long)]
    pub workspace: Option<PathBuf>,
}

/// One result.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct Hit {
    /// The noun it belongs to (`board`, `rmw`, `knob`, …).
    pub noun: &'static str,
    pub name: String,
    pub summary: String,
    /// The command that tells more about it.
    pub show: String,
}

/// One item a loader offers to the match.
struct Candidate {
    noun: &'static str,
    name: String,
    aliases: Vec<String>,
    summary: String,
    show: String,
    /// Rows listed when this one matches (an RMW's boards).
    related: Vec<Hit>,
}

impl Candidate {
    fn new(
        noun: &'static str,
        name: &str,
        aliases: Vec<String>,
        summary: &str,
        show: String,
    ) -> Self {
        Candidate {
            noun,
            name: name.to_string(),
            aliases,
            summary: summary.to_string(),
            show,
            related: Vec::new(),
        }
    }

    fn hit(&self) -> Hit {
        Hit {
            noun: self.noun,
            name: self.name.clone(),
            summary: self.summary.clone(),
            show: self.show.clone(),
        }
    }

    fn matches(&self, word: &str) -> bool {
        let w = word.to_lowercase();
        std::iter::once(&self.name)
            .chain(self.aliases.iter())
            .chain(std::iter::once(&self.summary))
            .any(|t| t.to_lowercase().contains(&w))
    }
}

/// What a loader is handed.
struct Ctx {
    workspace: Option<PathBuf>,
}

type Loader = fn(&Ctx) -> Result<Vec<Candidate>>;

/// One row per noun, in the order results print. Each loader is the noun's
/// own `list` loader.
const SOURCES: &[(&str, Loader)] = &[
    ("rmw", rmws),
    ("board", boards),
    ("platform", platforms),
    ("knob", knobs),
    ("profile", profiles),
    ("template", templates),
    ("runner", runners),
    ("interface", interfaces),
    ("package", packages),
    ("code", codes),
    ("image", images),
];

fn rmws(c: &Ctx) -> Result<Vec<Candidate>> {
    Ok(crate::cmd::rmw::collect(c.workspace.clone())?
        .into_iter()
        .map(|r| {
            let mut cand = Candidate::new(
                "rmw",
                &r.name,
                r.aliases.clone(),
                &r.summary,
                format!("nros rmw show {}", r.name),
            );
            let runs = r.boards.iter().map(|b| (b, "runs"));
            let builds = r.build_only_boards.iter().map(|b| (b, "builds with"));
            cand.related = runs
                .chain(builds)
                .map(|(b, how)| Hit {
                    noun: "board",
                    name: b.board.clone(),
                    summary: format!("{how} `{}`", r.name),
                    show: format!("nros board show {}", b.board),
                })
                .collect();
            cand
        })
        .collect())
}

fn boards(c: &Ctx) -> Result<Vec<Candidate>> {
    let (items, _notes) = crate::cmd::board::collect(c.workspace.clone())?;
    Ok(items
        .into_iter()
        .map(|b| {
            let summary = format!("{} board ({})", b.platform, b.krate);
            Candidate::new(
                "board",
                &b.key,
                b.aliases.clone(),
                &summary,
                format!("nros board show {}", b.key),
            )
        })
        .collect())
}

fn platforms(_c: &Ctx) -> Result<Vec<Candidate>> {
    let src = crate::cmd::platform_verb::load()?;
    Ok(crate::cmd::platform_verb::items(&src)?
        .into_iter()
        .map(|p| {
            let summary = match &p.inherits {
                Some(base) => format!("platform, builds on {base}"),
                None => "platform".to_string(),
            };
            Candidate::new(
                "platform",
                &p.name,
                p.aliases.clone(),
                &summary,
                format!("nros platform show {}", p.name),
            )
        })
        .collect())
}

fn knobs(c: &Ctx) -> Result<Vec<Candidate>> {
    Ok(crate::cmd::knob::collect(None, c.workspace.as_deref())?
        .into_iter()
        .map(|k| {
            Candidate::new(
                "knob",
                &k.name,
                Vec::new(),
                &k.summary,
                format!("nros knob show {}", k.name),
            )
        })
        .collect())
}

fn profiles(_c: &Ctx) -> Result<Vec<Candidate>> {
    Ok(crate::cmd::profile::items()
        .into_iter()
        .map(|p| {
            let aliases = p.cmake_build_type.map(str::to_string).into_iter().collect();
            Candidate::new(
                "profile",
                p.name,
                aliases,
                p.summary,
                format!("nros profile show {}", p.name),
            )
        })
        .collect())
}

fn templates(_c: &Ctx) -> Result<Vec<Candidate>> {
    Ok(crate::cmd::template_verb::items()
        .into_iter()
        .map(|t| {
            Candidate::new(
                "template",
                t.name,
                Vec::new(),
                t.summary,
                format!("nros template show {}", t.name),
            )
        })
        .collect())
}

fn runners(c: &Ctx) -> Result<Vec<Candidate>> {
    let ws = match &c.workspace {
        Some(w) => w.clone(),
        None => crate::cmd::extension::workspace(),
    };
    let (items, _refused) = crate::cmd::runner_verb::items(&ws);
    Ok(items
        .into_iter()
        .map(|r| {
            let summary = format!("{} from {}", r.kind.as_str(), r.package);
            Candidate::new(
                "runner",
                &r.name,
                Vec::new(),
                &summary,
                format!("nros runner show {}", r.name),
            )
        })
        .collect())
}

/// Every message, service and action type on the interface search path, as
/// `nros interface list` reads it.
fn interfaces(c: &Ctx) -> Result<Vec<Candidate>> {
    let index = crate::cmd::interface::load_index(c.workspace.as_deref())?;
    let mut out = Vec::new();
    for p in index.packages().values() {
        let kinds = [
            ("msg", "message type", &p.interfaces.messages),
            ("srv", "service type", &p.interfaces.services),
            ("action", "action type", &p.interfaces.actions),
        ];
        for (kind, what, names) in kinds {
            for n in names {
                let full = format!("{}/{kind}/{n}", p.name);
                let summary = format!("{what} in {}", p.name);
                let show = format!("nros interface show {full}");
                out.push(Candidate::new(
                    "interface",
                    &full,
                    Vec::new(),
                    &summary,
                    show,
                ));
            }
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// The workspace's packages, as `nros package list` prints them. Outside a
/// workspace there are none, which is not an error.
fn packages(c: &Ctx) -> Result<Vec<Candidate>> {
    let Ok(rows) = crate::cmd::ws::package_rows(c.workspace.as_deref()) else {
        return Ok(Vec::new());
    };
    Ok(rows
        .into_iter()
        .map(|(name, kind, dir)| {
            let summary = format!("{kind} package in {}", dir.display());
            Candidate::new(
                "package",
                &name,
                Vec::new(),
                &summary,
                "nros package list".into(),
            )
        })
        .collect())
}

fn codes(_c: &Ctx) -> Result<Vec<Candidate>> {
    Ok(crate::cmd::code_verb::items(false)
        .into_iter()
        .map(|e| {
            Candidate::new(
                "code",
                e.code,
                vec![e.name.to_string()],
                e.title,
                format!("nros code show {}", e.code),
            )
        })
        .collect())
}

/// The workspace's images, and each one's planned nodes and topics when it
/// has a plan. Outside a workspace there are none, which is not an error.
fn images(c: &Ctx) -> Result<Vec<Candidate>> {
    let Ok(root) = crate::cmd::image_verb::root(c.workspace.clone()) else {
        return Ok(Vec::new());
    };
    let mut out = Vec::new();
    for i in crate::cmd::image_verb::items(&root)? {
        let summary = format!(
            "image, board {}, rmw {}",
            i.board.as_deref().unwrap_or("-"),
            i.rmw.as_deref().unwrap_or("-")
        );
        out.push(Candidate::new(
            "image",
            &i.name,
            vec![i.id.clone()],
            &summary,
            format!("nros image show {}", i.name),
        ));
        // A plan is there only after `nros configure`; without one the image
        // row stands alone.
        let Ok(model) = crate::cmd::image_verb::planned_model(&i) else {
            continue;
        };
        for n in crate::cmd::image_verb::planned_nodes(&model) {
            let summary = format!("node planned by image {}: runs {}", i.name, n.runs);
            out.push(Candidate::new(
                "node",
                &n.name,
                Vec::new(),
                &summary,
                format!("nros image show {} --nodes", i.name),
            ));
        }
        for e in crate::cmd::image_verb::planned_endpoints(&model) {
            let summary = format!("{} {} planned by image {}", e.kind, e.ty, i.name);
            out.push(Candidate::new(
                "topic",
                &e.name,
                Vec::new(),
                &summary,
                format!("nros image show {} --topics", i.name),
            ));
        }
    }
    Ok(out)
}

/// Every hit for `word` over `sources`, grouped by noun in the sources'
/// order, each (noun, name) once. A source that fails is skipped and named in
/// the returned notes.
fn search_in(sources: &[(&str, Loader)], ctx: &Ctx, word: &str) -> (Vec<Hit>, Vec<String>) {
    let mut by_noun: Vec<(&'static str, Vec<Hit>)> = Vec::new();
    let mut notes = Vec::new();
    let push = |by_noun: &mut Vec<(&'static str, Vec<Hit>)>, h: Hit| {
        let slot = match by_noun.iter().position(|(n, _)| *n == h.noun) {
            Some(i) => i,
            None => {
                by_noun.push((h.noun, Vec::new()));
                by_noun.len() - 1
            }
        };
        if !by_noun[slot].1.iter().any(|x| x.name == h.name) {
            by_noun[slot].1.push(h);
        }
    };
    for (noun, load) in sources {
        match load(ctx) {
            Ok(cands) => {
                for c in cands.iter().filter(|c| c.matches(word)) {
                    push(&mut by_noun, c.hit());
                    for r in &c.related {
                        push(&mut by_noun, r.clone());
                    }
                }
            }
            Err(e) => {
                let first = e.to_string();
                let first = first.lines().next().unwrap_or("");
                notes.push(format!("search: {noun} not searched: {first}"));
            }
        }
    }
    (by_noun.into_iter().flat_map(|(_, h)| h).collect(), notes)
}

pub fn run(args: Args) -> Result<()> {
    let ctx = Ctx {
        workspace: args.workspace.clone(),
    };
    let (hits, notes) = search_in(SOURCES, &ctx, &args.word);
    for n in &notes {
        eprintln!("{n}");
    }
    if args.json {
        return frame::print_json(
            SCHEMA,
            "items",
            &hits,
            &[("word", serde_json::json!(args.word))],
        );
    }
    let empty = format!("Nothing matches `{}`.", args.word);
    let mut t = frame::Table::new(&["NOUN", "NAME", "SUMMARY", "SHOW"], &empty);
    for h in &hits {
        t.row(vec![
            h.noun.to_string(),
            h.name.clone(),
            h.summary.clone(),
            h.show.clone(),
        ]);
    }
    print!("{}", t.render());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Ctx {
        Ctx { workspace: None }
    }

    fn fake_rmw(_c: &Ctx) -> Result<Vec<Candidate>> {
        let mut c = Candidate::new("rmw", "zenoh", vec!["zpico".into()], "Zenoh", "show".into());
        c.related = vec![Hit {
            noun: "board",
            name: "nucleo".into(),
            summary: "runs `zenoh`".into(),
            show: "nros board show nucleo".into(),
        }];
        Ok(vec![
            c,
            Candidate::new("rmw", "xrce", vec![], "XRCE-DDS", "s".into()),
        ])
    }

    fn fake_board(_c: &Ctx) -> Result<Vec<Candidate>> {
        Ok(vec![
            Candidate::new("board", "nucleo", vec![], "zephyr board", "s".into()),
            Candidate::new(
                "board",
                "pico",
                vec!["rp2040".into()],
                "zephyr board",
                "s".into(),
            ),
        ])
    }

    fn failing(_c: &Ctx) -> Result<Vec<Candidate>> {
        eyre::bail!("no catalog here\nsecond line")
    }

    /// The `--json` schema keeps its version while keys are only added.
    #[test]
    fn the_schema_is_pinned() {
        assert_eq!(SCHEMA, "nros.search.v1");
        let h = Hit {
            noun: "rmw",
            name: "zenoh".into(),
            summary: "s".into(),
            show: "nros rmw show zenoh".into(),
        };
        let keys: Vec<String> = serde_json::to_value(&h)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(keys, ["name", "noun", "show", "summary"]);
    }

    #[test]
    fn a_match_on_name_alias_or_summary_is_case_insensitive() {
        let src: &[(&str, Loader)] = &[("rmw", fake_rmw), ("board", fake_board)];
        let names = |w| {
            search_in(src, &ctx(), w)
                .0
                .into_iter()
                .map(|h| format!("{}:{}", h.noun, h.name))
                .collect::<Vec<_>>()
        };
        assert_eq!(names("XRCE"), ["rmw:xrce"]);
        assert_eq!(names("RP2040"), ["board:pico"]);
        assert_eq!(names("dds"), ["rmw:xrce"]);
    }

    #[test]
    fn an_rmw_lists_its_boards_once_under_board() {
        let src: &[(&str, Loader)] = &[("rmw", fake_rmw), ("board", fake_board)];
        let (hits, _) = search_in(src, &ctx(), "zenoh");
        let rows: Vec<_> = hits.iter().map(|h| (h.noun, h.name.as_str())).collect();
        assert_eq!(rows, [("rmw", "zenoh"), ("board", "nucleo")]);
    }

    #[test]
    fn a_failing_source_is_named_and_the_rest_still_answer() {
        let src: &[(&str, Loader)] = &[("board", failing), ("rmw", fake_rmw)];
        let (hits, notes) = search_in(src, &ctx(), "zenoh");
        assert_eq!(hits[0].name, "zenoh");
        assert_eq!(notes, ["search: board not searched: no catalog here"]);
    }

    /// The acceptance's second half: search reads each noun's own loader, so
    /// every item a compiled-in source lists is found by its own name, with
    /// no search code naming it.
    #[test]
    fn every_item_a_compiled_in_source_lists_is_found_by_name() {
        for (noun, load) in SOURCES {
            if !["knob", "profile", "template", "code"].contains(noun) {
                continue;
            }
            let src: &[(&str, Loader)] = &[(noun, *load)];
            for c in load(&ctx()).expect("compiled-in source loads") {
                let (hits, _) = search_in(src, &ctx(), &c.name);
                assert!(
                    hits.iter().any(|h| h.noun == c.noun && h.name == c.name),
                    "{noun} `{}` is not found by its own name",
                    c.name
                );
            }
        }
    }
}
