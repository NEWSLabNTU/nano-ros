//! `nros template list` / `nros template show <role>` (RFC-0133, phase-569
//! W6).
//!
//! The node roles `nros new --template <role>` scaffolds, read from the
//! scaffold's own role table, so a role added there is listed here with no
//! change to this verb.

use clap::{Args as ClapArgs, Subcommand};
use eyre::Result;
use serde::Serialize;

use cargo_nano_ros::node_role::Role;

use crate::cmd::frame::{self, Fields, Table};

/// The `--json` schema names (RFC-0133 D1). Adding a key keeps the version;
/// removing or renaming one is a new version.
pub const LIST_SCHEMA: &str = "nros.template.list.v1";
pub const SHOW_SCHEMA: &str = "nros.template.show.v1";
/// The Tutorial chapter that scaffolds the server/client and action roles.
pub const BOOK_PAGE: &str = "tutorial/services-and-actions.html";

/// The languages every role is scaffolded in.
pub const LANGUAGES: [&str; 3] = ["rust", "c", "cpp"];

#[derive(Debug, Subcommand)]
pub enum Args {
    /// List the node roles `nros new --template` scaffolds.
    List(JsonArgs),
    /// Show one role: what it does, its interface and its languages.
    Show(ShowArgs),
}

#[derive(Debug, ClapArgs)]
pub struct JsonArgs {
    /// Print one JSON object instead of text.
    #[arg(long)]
    pub json: bool,
}

#[derive(Debug, ClapArgs)]
pub struct ShowArgs {
    /// The role, as `nros new --template` takes it (`talker`).
    pub role: String,
    /// Print one JSON object instead of text.
    #[arg(long)]
    pub json: bool,
}

/// One node role.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TemplateItem {
    pub name: &'static str,
    pub summary: &'static str,
    /// The interface package the role uses (the ROS demo's own).
    pub interface_package: &'static str,
    /// The node name a component of this role gets by default.
    pub demo_node_name: &'static str,
    pub languages: [&'static str; 3],
}

pub fn run(args: Args) -> Result<()> {
    match args {
        Args::List(a) => list(a),
        Args::Show(a) => show(a),
    }
}

pub fn item(r: Role) -> TemplateItem {
    TemplateItem {
        name: r.name(),
        summary: r.summary(),
        interface_package: r.interface_package(),
        demo_node_name: r.demo_node_name(),
        languages: LANGUAGES,
    }
}

pub fn items() -> Vec<TemplateItem> {
    Role::ALL.into_iter().map(item).collect()
}

fn list(args: JsonArgs) -> Result<()> {
    let items = items();
    if args.json {
        return frame::print_json(LIST_SCHEMA, "items", &items, &[]);
    }
    print!("{}", render_list(&items));
    Ok(())
}

/// The role `name` names, or the refusal with the nearest roles.
pub fn find(name: &str) -> Result<Role> {
    Role::parse(name).ok_or_else(|| {
        frame::unknown(
            "template",
            name,
            Role::ALL.iter().map(|r| r.name()),
            "nros template list",
        )
    })
}

fn show(args: ShowArgs) -> Result<()> {
    let item = item(find(&args.role)?);
    if args.json {
        return frame::print_json(SHOW_SCHEMA, "item", &item, &[]);
    }
    print!("{}", render_show(&item));
    Ok(())
}

pub fn render_list(items: &[TemplateItem]) -> String {
    let mut t = Table::new(&["TEMPLATE", "INTERFACES", "WHAT IT IS"], "No templates.");
    for i in items {
        t.row(vec![
            i.name.to_string(),
            i.interface_package.to_string(),
            i.summary.to_string(),
        ]);
    }
    t.footer("`nros new <name> --template <template>` scaffolds one.");
    t.render()
}

pub fn render_show(i: &TemplateItem) -> String {
    let mut out = frame::show_head(i.name, i.summary);
    let mut f = Fields::new();
    f.field("interfaces", i.interface_package)
        .field("node name", i.demo_node_name)
        .field("languages", i.languages.join(", "))
        .field("scaffold", format!("nros new <name> --template {}", i.name));
    out.push_str(&f.render());
    out.push_str(&frame::book_line(BOOK_PAGE));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_role_the_scaffold_knows_is_listed() {
        let names: Vec<&str> = items().iter().map(|i| i.name).collect();
        let roles: Vec<&str> = Role::ALL.iter().map(|r| r.name()).collect();
        assert_eq!(names, roles);
        for r in Role::ALL {
            assert!(render_list(&items()).contains(r.name()));
        }
    }

    #[test]
    fn an_unknown_role_is_refused_with_the_nearest() {
        let e = find("talkr").unwrap_err().to_string();
        assert!(e.contains("did you mean `talker`?"), "{e}");
    }

    #[test]
    fn json_schema_names_are_pinned() {
        assert_eq!(LIST_SCHEMA, "nros.template.list.v1");
        assert_eq!(SHOW_SCHEMA, "nros.template.show.v1");
    }
}
