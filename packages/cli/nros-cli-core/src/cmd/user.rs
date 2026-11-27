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

/// Old user spellings that move an ARGUMENT, not only a command path
/// (RFC-0133 D3), as sample command lines. `<code>` stands for any
/// diagnostic code. Listed with [`ALIASES`] in `nros help --aliases`, and
/// noted the same way; the tests hold each to the parser.
pub const ARG_ALIASES: &[(&str, &str)] = &[
    ("setup --list", "toolchain list --available"),
    ("explain <code>", "code show <code>"),
    ("config explain --platform", "knob list --platform"),
];

/// Flags that keep an [`ARG_ALIASES`] old spelling on its own handler: with
/// one of them, the old command is no old spelling (and `nros help --aliases
/// --json` says so, so `check-cli-old-spellings` never rewrites it).
pub const ARG_ALIAS_UNLESS: &[(&str, &[&str])] =
    &[("config explain --platform", &["--get", "--platforms-dir"])];

/// Whether `config explain` was asked only what `knob list --platform`
/// answers: no `--get` (one value, for a script) and no `--platforms-dir`
/// (a search path `knob list` does not take).
fn config_explain_is_knob_list(a: &crate::cmd::config::ExplainArgs) -> bool {
    a.get.is_none() && a.platforms_dir.is_none()
}

/// The groups `nros --help` shows, in RFC-0117 D2's order. Every visible
/// top-level verb is in exactly one group; a test holds that.
pub const GROUPS: &[(&str, &[&str])] = &[
    ("Start", &["new", "setup"]),
    (
        "Build",
        &["build", "configure", "fix", "clean", "materialize"],
    ),
    (
        "Run on a target",
        &["run", "flash", "monitor", "bench", "debug"],
    ),
    ("Workspace", &["list", "package", "image", "graph"]),
    // RFC-0133 D3: the discovery verbs, one group.
    (
        "Find what nano-ros offers",
        &[
            "board",
            "platform",
            "rmw",
            "runner",
            "knob",
            "profile",
            "interface",
            "template",
            "code",
        ],
    ),
    ("Inspect", &["doctor", "config", "plan", "check", "explain"]),
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
        Cmd::Toolchain(toolchain::Args {
            command: toolchain::Sub::List(a),
        })
        | Cmd::Store(store::Args {
            command: store::Sub::List(a),
        }) if a.available => setup_list(),
        // RFC-0133 D3: the old spelling lowers to the new verb's handler.
        Cmd::Explain(a) if crate::cmd::explain::diagnostic_code(&a.plan).is_some() => Cmd::Code(
            crate::cmd::code_verb::Args::Show(crate::cmd::code_verb::ShowArgs {
                code: a.plan.to_string_lossy().into_owned(),
                json: false,
            }),
        ),
        Cmd::Config(crate::cmd::config::Args::Explain(a)) if config_explain_is_knob_list(&a) => {
            Cmd::Knob(crate::cmd::knob::Args::List(crate::cmd::knob::ListArgs {
                json: false,
                platform: Some(a.platform),
                board_toml: a.board_toml,
                workspace: a.workspace,
            }))
        }
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
        Cmd::List(a) | Cmd::Package(crate::cmd::PackageArgs::List(a)) => Cmd::Ws(ws::Args {
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

/// `nros setup --list`, as the parser builds it, so both spellings reach the
/// same arguments, defaults included.
fn setup_list() -> Cmd {
    use clap::{Args as _, FromArgMatches as _};
    let m = setup::Args::augment_args(clap::Command::new("setup"))
        .try_get_matches_from(["setup", "--list"])
        .expect("`setup --list` parses");
    Cmd::Setup(setup::Args::from_arg_matches(&m).expect("`setup --list` builds its arguments"))
}

/// The [`ALIASES`] or [`ARG_ALIASES`] row `cmd` was spelled with, when it is
/// an old user spelling. Read before [`lower`], which erases the difference.
pub fn old_spelling(cmd: &Cmd) -> Option<(&'static str, &'static str)> {
    match cmd {
        Cmd::Setup(a) if a.list => return Some(ARG_ALIASES[0]),
        Cmd::Explain(a) if crate::cmd::explain::diagnostic_code(&a.plan).is_some() => {
            return Some(ARG_ALIASES[1]);
        }
        Cmd::Config(crate::cmd::config::Args::Explain(a)) if config_explain_is_knob_list(a) => {
            return Some(ARG_ALIASES[2]);
        }
        _ => {}
    }
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
    // A sample row names the code it was given.
    let code = match cmd {
        Cmd::Explain(a) => a.plan.to_string_lossy().into_owned(),
        _ => String::new(),
    };
    let (old, new) = (old.replace("<code>", &code), new.replace("<code>", &code));
    let (old, new) = (old.as_str(), new.as_str());
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
                "Find what nano-ros offers:",
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
        assert_eq!(
            rows.len(),
            crate::cmd::dev::ALIASES.len() + ALIASES.len() + ARG_ALIASES.len()
        );
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

    /// RFC-0133 D3: each argument-level row, with a real code in place of
    /// `<code>`, lowers to the same command under both spellings; the old one
    /// is named as that row and notes on a terminal, the new one never.
    #[test]
    fn every_argument_alias_lowers_to_the_same_command() {
        use clap::Parser;
        let line = |s: &str| -> Vec<String> {
            std::iter::once("nros".to_string())
                .chain(
                    s.replace("<code>", "NROS-0001")
                        .split(' ')
                        .map(str::to_string),
                )
                // A flag that takes a value gets one.
                .chain(s.ends_with("--platform").then(|| "posix".to_string()))
                .collect()
        };
        for (old, new) in ARG_ALIASES {
            let o = Cli::try_parse_from(line(old)).unwrap().cmd;
            let n = Cli::try_parse_from(line(new)).unwrap().cmd;
            assert_eq!(old_spelling(&o), Some((*old, *new)), "`nros {old}`");
            assert_eq!(old_spelling(&n), None, "`nros {new}` is the new spelling");
            let mut tty = Vec::new();
            note_old_spelling(&o, true, &mut tty);
            assert_eq!(
                String::from_utf8(tty).unwrap(),
                format!(
                    "{}\n",
                    note(
                        &old.replace("<code>", "NROS-0001"),
                        &new.replace("<code>", "NROS-0001")
                    )
                )
            );
            assert_eq!(
                format!("{:?}", crate::cmd::lower(o)),
                format!("{:?}", crate::cmd::lower(n)),
                "`nros {old}` and `nros {new}` lower to different commands"
            );
        }
        // A plan path is not a code: `nros explain <plan>` is no old spelling.
        let plan = Cli::try_parse_from(["nros", "explain", "build/nros/nros-plan.json"])
            .unwrap()
            .cmd;
        assert_eq!(old_spelling(&plan), None);
        assert!(matches!(crate::cmd::lower(plan), Cmd::Explain(_)));
        // `config explain --get` asks for one value, which `knob list` does
        // not print: no old spelling, and it keeps its handler.
        let get = Cli::try_parse_from([
            "nros",
            "config",
            "explain",
            "--platform",
            "posix",
            "--get",
            "X",
        ])
        .unwrap()
        .cmd;
        assert_eq!(old_spelling(&get), None);
        // ... and every flag the table says keeps the old handler does.
        for (old, flags) in ARG_ALIAS_UNLESS {
            assert!(
                ARG_ALIASES.iter().any(|(o, _)| o == old),
                "`{old}` is no row"
            );
            for f in *flags {
                let mut l = line(old);
                l.extend([f.to_string(), "x".to_string()]);
                let c = Cli::try_parse_from(&l).unwrap().cmd;
                assert_eq!(old_spelling(&c), None, "`{}`", l.join(" "));
            }
        }
        assert!(matches!(
            crate::cmd::lower(get),
            Cmd::Config(crate::cmd::config::Args::Explain(_))
        ));
    }

    /// `nros package list` is `nros list` under its noun: the same command,
    /// and no note, because neither spelling is old.
    #[test]
    fn package_list_is_list() {
        use clap::Parser;
        for args in [&["list"][..], &["list", "-n", "--packages-select", "x"][..]] {
            let a: Vec<&str> = std::iter::once("nros")
                .chain(args.iter().copied())
                .collect();
            let b: Vec<&str> = ["nros", "package"]
                .into_iter()
                .chain(args.iter().copied())
                .collect();
            let (a, b) = (
                Cli::try_parse_from(&a).unwrap().cmd,
                Cli::try_parse_from(&b).unwrap().cmd,
            );
            assert_eq!(old_spelling(&b), None);
            assert_eq!(
                format!("{:?}", crate::cmd::lower(a)),
                format!("{:?}", crate::cmd::lower(b))
            );
        }
    }
}
