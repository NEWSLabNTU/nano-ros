//! The user tier (RFC-0117 D2/D5): the verbs `nros --help` shows, grouped by
//! task, and the older user spellings they absorbed.
//!
//! An absorbed spelling stays a hidden variant of [`Cmd`], and its new
//! spelling is [`lower`]ed into it, so one handler serves both. Unlike a
//! `nros dev` alias, an old user spelling prints one note on stderr naming the
//! new spelling, and only when stderr is a terminal: a script that captures
//! output sees no change (RFC-0117 D5).

use std::io::Write;

use crate::cmd::{Cmd, setup, store, toolchain, ws};

/// Every old user spelling, as `(old, new)` command paths.
///
/// The tests below hold this table to the parser in both directions: each
/// row's two spellings lower to the same command, [`old_spelling`] names the
/// row for the old one and nothing for the new one.
pub const ALIASES: &[(&str, &str)] = &[
    ("init", "setup presets"),
    ("store list", "toolchain list"),
    ("store gc", "toolchain gc"),
    ("toolchain uninstall", "toolchain remove"),
    ("generate", "interface generate"),
    ("ws list", "list"),
    ("ws clean", "clean"),
    ("sync", "configure"),
];

/// The groups `nros --help` shows, in RFC-0117 D2's order. Every visible
/// top-level verb is in exactly one group; a test holds that.
pub const GROUPS: &[(&str, &[&str])] = &[
    ("Start", &["new", "setup"]),
    ("Build", &["build", "configure", "clean", "materialize"]),
    (
        "Run on a target",
        &["run", "flash", "monitor", "bench", "debug"],
    ),
    ("Workspace", &["list", "graph"]),
    ("Interfaces", &["interface"]),
    (
        "Inspect",
        &["doctor", "board", "config", "plan", "check", "explain"],
    ),
    ("Toolchain", &["toolchain", "pin", "update", "locate"]),
    ("Other", &["version", "completions", "help"]),
];

/// Lower a user-tier spelling into the command the older spelling parses to.
/// Commands with no older spelling pass through unchanged.
pub fn lower(cmd: Cmd) -> Cmd {
    match cmd {
        Cmd::Setup(setup::Args {
            command: Some(setup::SetupCommand::Presets(a)),
            ..
        }) => Cmd::Init(a),
        Cmd::Toolchain(toolchain::Args { command }) => match command {
            toolchain::Sub::List(a) => Cmd::Store(store::Args {
                command: store::Sub::List(a),
            }),
            toolchain::Sub::Gc(a) => Cmd::Store(store::Args {
                command: store::Sub::Gc(a),
            }),
            toolchain::Sub::Remove(a) => Cmd::Toolchain(toolchain::Args {
                command: toolchain::Sub::Uninstall(a),
            }),
            other => Cmd::Toolchain(toolchain::Args { command: other }),
        },
        Cmd::Interface(crate::cmd::interface::Args::Generate(a)) => Cmd::Generate(a),
        Cmd::List(a) => Cmd::Ws(ws::Args {
            command: ws::Sub::List(a),
        }),
        Cmd::Clean(a) => Cmd::Ws(ws::Args {
            command: ws::Sub::Clean(a),
        }),
        // phase-530 W1 — the OLD spelling lowers to the new one here, the
        // reverse of the rows above: `configure` is the verb with a handler.
        Cmd::Sync(a) => Cmd::Configure(a),
        other => other,
    }
}

/// The [`ALIASES`] row `cmd` was spelled with, when it is an old user
/// spelling. Read before [`lower`], which erases the difference.
pub fn old_spelling(cmd: &Cmd) -> Option<(&'static str, &'static str)> {
    let i = match cmd {
        Cmd::Init(_) => 0,
        Cmd::Store(store::Args {
            command: store::Sub::List(_),
        }) => 1,
        Cmd::Store(store::Args {
            command: store::Sub::Gc(_),
        }) => 2,
        Cmd::Toolchain(toolchain::Args {
            command: toolchain::Sub::Uninstall(_),
        }) => 3,
        Cmd::Generate(_) => 4,
        Cmd::Ws(ws::Args {
            command: ws::Sub::List(_),
        }) => 5,
        Cmd::Ws(ws::Args {
            command: ws::Sub::Clean(_),
        }) => 6,
        Cmd::Sync(_) => 7,
        _ => return None,
    };
    Some(ALIASES[i])
}

/// The one line an old user spelling prints.
pub fn note(old: &str, new: &str) -> String {
    format!("note: `nros {old}` is now `nros {new}`; the old spelling keeps working")
}

/// Old spellings that are TEMPORARY and will be retired, so they warn on
/// every run, terminal or not: a script still calling one must hear about it
/// before it stops working. Every other row keeps RFC-0117 D5's terminal-only
/// note. `sync` (phase-530 W1, maintainer ruling 2026-11-03).
pub const RETIRING: &[&str] = &["sync"];

/// The one line a [`RETIRING`] spelling prints.
pub fn retiring_note(old: &str, new: &str) -> String {
    format!(
        "warning: `nros {old}` is a temporary alias of `nros {new}` and will be removed; \
         use `nros {new}`"
    )
}

/// Print the note for an old user spelling to `out`: a [`RETIRING`] one
/// always ([`retiring_note`]), any other only when stderr is a terminal
/// ([`note`]). `terminal` is a parameter so the test can drive both answers.
pub fn note_old_spelling(cmd: &Cmd, terminal: bool, out: &mut impl Write) {
    let Some((old, new)) = old_spelling(cmd) else {
        return;
    };
    if RETIRING.contains(&old) {
        let _ = writeln!(out, "{}", retiring_note(old, new));
    } else if terminal {
        let _ = writeln!(out, "{}", note(old, new));
    }
}

/// The verb list `nros --help` prints in place of clap's flat one: one
/// heading per [`GROUPS`] row, each verb with its one-line summary. With
/// `with_dev`, a last group lists `dev` (for `nros help --all`).
pub fn render_groups(root: &clap::Command, with_dev: bool) -> String {
    let summary = |name: &str| -> String {
        if name == "help" {
            return "Print this message or the help of the given subcommand(s)".to_string();
        }
        root.find_subcommand(name)
            .and_then(|c| c.get_about())
            .map(|a| a.to_string())
            .unwrap_or_default()
    };
    let mut groups: Vec<(&str, Vec<&str>)> = GROUPS.iter().map(|(h, v)| (*h, v.to_vec())).collect();
    if with_dev {
        groups.push(("Developer and build-system tier", vec!["dev"]));
    }
    let width = groups
        .iter()
        .flat_map(|(_, v)| v.iter().map(|n| n.len()))
        .max()
        .unwrap_or(0);
    let mut out = String::new();
    for (heading, verbs) in groups {
        out.push_str(heading);
        out.push_str(":\n");
        for v in verbs {
            out.push_str(&format!("  {v:<width$}  {}\n", summary(v)));
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cmd::dev::tests::{Cli, argv, find, parsed, shape};
    use clap::CommandFactory;

    /// `nros sync`'s output directory is `--generated-base`, with the old
    /// `--build-dir` hidden beside it; colcon's `--build-base` is not taken,
    /// because it would name a different directory (RFC-0117 D4).
    #[test]
    fn sync_names_its_generated_base() {
        use clap::Parser;
        // Through the lowering: `nros sync` is `nros configure` (phase-530 W1).
        let dir = |argv: &[&str]| match Cli::try_parse_from(argv).map(|c| crate::cmd::lower(c.cmd))
        {
            Ok(crate::cmd::Cmd::Configure(a)) => Ok(a.sync.build_dir),
            Ok(other) => panic!("{argv:?} parsed to {other:?}"),
            Err(e) => Err(e.kind()),
        };
        assert_eq!(dir(&["nros", "sync"]), Ok("generated".into()));
        assert_eq!(
            dir(&["nros", "sync", "--generated-base", "gen"]),
            Ok("gen".into())
        );
        assert_eq!(
            dir(&["nros", "sync", "--build-dir", "gen"]),
            Ok("gen".into())
        );
        assert_eq!(
            dir(&["nros", "sync", "--build-base", "gen"]),
            Err(clap::error::ErrorKind::UnknownArgument)
        );
        let help = Cli::command()
            .find_subcommand_mut("sync")
            .unwrap()
            .render_long_help()
            .to_string();
        assert!(help.contains("--generated-base"), "{help}");
        assert!(!help.contains("--build-dir"), "{help}");
    }

    #[test]
    fn every_user_alias_pair_parses_to_the_same_command() {
        let root = Cli::command();
        for (old, new) in ALIASES {
            assert_eq!(
                shape(find(&root, old)),
                shape(find(&root, new)),
                "`nros {old}` and `nros {new}` take different arguments"
            );
            let old_argv = argv(&root, old);
            let mut new_argv = argv(&root, new);
            let old_tail = &old_argv[1 + old.split(' ').count()..];
            new_argv.truncate(1 + new.split(' ').count());
            new_argv.extend(old_tail.iter().cloned());
            assert_eq!(
                format!("{:?}", parsed(&old_argv)),
                format!("{:?}", parsed(&new_argv)),
                "`{}` and `{}` lower to different commands",
                old_argv.join(" "),
                new_argv.join(" ")
            );
        }
    }

    /// The old spelling notes on a terminal and is silent when stderr is
    /// piped; the new spelling is silent either way.
    #[test]
    fn old_user_spellings_note_only_on_a_terminal() {
        use clap::Parser;
        let root = Cli::command();
        for (old, new) in ALIASES.iter().filter(|(o, _)| !RETIRING.contains(o)) {
            let old_cmd = Cli::try_parse_from(argv(&root, old)).unwrap().cmd;
            let new_cmd = Cli::try_parse_from(argv(&root, new)).unwrap().cmd;
            let mut tty = Vec::new();
            note_old_spelling(&old_cmd, true, &mut tty);
            assert_eq!(
                String::from_utf8(tty).unwrap(),
                format!("{}\n", note(old, new)),
                "`nros {old}` on a terminal"
            );
            for (cmd, terminal, what) in [
                (&old_cmd, false, "old, piped"),
                (&new_cmd, true, "new, terminal"),
                (&new_cmd, false, "new, piped"),
            ] {
                let mut buf = Vec::new();
                note_old_spelling(cmd, terminal, &mut buf);
                assert!(buf.is_empty(), "`nros {old}` ({what}) printed a note");
            }
        }
    }

    /// `nros sync` is a temporary alias (maintainer, 2026-11-03): it warns
    /// with stderr piped too, naming `nros configure`; the new spelling never
    /// warns.
    #[test]
    fn sync_alias_warns_without_a_tty() {
        use clap::Parser;
        let root = Cli::command();
        for terminal in [false, true] {
            let mut buf = Vec::new();
            let old = Cli::try_parse_from(argv(&root, "sync")).unwrap().cmd;
            note_old_spelling(&old, terminal, &mut buf);
            let text = String::from_utf8(buf).unwrap();
            assert!(
                text.contains("`nros configure`"),
                "terminal={terminal}: {text:?}"
            );
            assert_eq!(text, format!("{}\n", retiring_note("sync", "configure")));

            let mut buf = Vec::new();
            let new = Cli::try_parse_from(argv(&root, "configure")).unwrap().cmd;
            note_old_spelling(&new, terminal, &mut buf);
            assert!(buf.is_empty(), "`nros configure` warned");
        }
        // Every retiring spelling is a row of the table.
        for r in RETIRING {
            assert!(ALIASES.iter().any(|(o, _)| o == r), "`{r}` is no alias");
        }
    }

    #[test]
    fn old_user_spellings_are_hidden() {
        let root = Cli::command();
        let shown: Vec<&str> = ALIASES
            .iter()
            .map(|(old, _)| *old)
            .filter(|old| {
                // Hidden when any word of the path is: `store` hides
                // `store list` with it.
                let words: Vec<&str> = old.split(' ').collect();
                !(1..=words.len()).any(|n| find(&root, &words[..n].join(" ")).is_hide_set())
            })
            .collect();
        assert!(shown.is_empty(), "old spellings still in help: {shown:?}");
    }

    /// Every verb `nros --help` would list is in exactly one group, and every
    /// group names only verbs that exist and are shown.
    #[test]
    fn groups_cover_the_visible_tree_exactly_once() {
        let mut root = Cli::command();
        root.build();
        let visible: Vec<String> = root
            .get_subcommands()
            .filter(|c| !c.is_hide_set())
            .map(|c| c.get_name().to_string())
            .collect();
        let grouped: Vec<&str> = GROUPS.iter().flat_map(|(_, v)| v.iter().copied()).collect();
        for v in &visible {
            let n = grouped.iter().filter(|g| **g == v.as_str()).count();
            assert_eq!(n, 1, "`nros {v}` is in {n} help groups");
        }
        for g in &grouped {
            assert!(
                visible.iter().any(|v| v == g),
                "help group names `{g}`, which is not a shown verb"
            );
        }
        // No dev verb is shown.
        for (old, _) in crate::cmd::dev::ALIASES {
            assert!(
                !grouped.contains(old),
                "dev verb `{old}` is in a help group"
            );
        }
    }

    #[test]
    fn groups_render_in_rfc_order() {
        let mut root = Cli::command();
        root.build();
        let text = render_groups(&root, false);
        let headings: Vec<&str> = text
            .lines()
            .filter(|l| !l.starts_with(' ') && l.ends_with(':'))
            .collect();
        assert_eq!(
            headings,
            [
                "Start:",
                "Build:",
                "Run on a target:",
                "Workspace:",
                "Interfaces:",
                "Inspect:",
                "Toolchain:",
                "Other:"
            ]
        );
        assert!(!text.contains("  dev "));
        assert!(render_groups(&root, true).contains("  dev "));
    }

    #[test]
    fn aliases_json_lists_both_tiers() {
        let rows: Vec<serde_json::Value> =
            serde_json::from_str(&crate::cmd::aliases_json()).unwrap();
        assert_eq!(rows.len(), crate::cmd::dev::ALIASES.len() + ALIASES.len());
        let tagged: Vec<(&str, &str, &str)> = crate::cmd::all_aliases().collect();
        for ((old, new, tier), row) in tagged.iter().zip(&rows) {
            assert_eq!(row["old"], *old);
            assert_eq!(row["new"], *new);
            assert_eq!(row["tier"], *tier);
        }
        for (old, _) in ALIASES {
            assert!(rows.iter().any(|r| r["old"] == *old && r["tier"] == "user"));
        }
        for (old, _) in crate::cmd::dev::ALIASES {
            assert!(rows.iter().any(|r| r["old"] == *old && r["tier"] == "dev"));
        }
    }
}
