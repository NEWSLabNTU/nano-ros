//! The names `<Tab>` offers (RFC-0133 D7, phase-569 W4).
//!
//! Each completer answers one name argument from what the CLI already reads:
//! compiled-in tables and the files a `list` verb reads. A completer runs on
//! every `<Tab>`, so it never builds, scans a workspace or reaches the
//! network, and it never fails: a source it cannot read offers no names.

use clap_complete::engine::CompletionCandidate;
use eyre::{Result, eyre};

use crate::orchestration::{board_descriptor::BoardCatalog, rmw_index};

/// Board keys and their aliases, as `board = "…"` and `nros board show` take
/// them, from the board catalog of the SDK root `nros board list` reads.
pub fn boards() -> Vec<CompletionCandidate> {
    let Ok(root) = crate::cmd::board::catalog_root(None) else {
        return Vec::new();
    };
    let Ok(catalog) = BoardCatalog::load(&root) else {
        return Vec::new();
    };
    let names = catalog
        .descriptors()
        .iter()
        .flat_map(|d| d.names.iter().cloned());
    sorted(names.map(CompletionCandidate::new))
}

/// RMW names and their aliases, as `rmw = "…"` takes them.
pub fn rmws() -> Vec<CompletionCandidate> {
    let Ok(cwd) = std::env::current_dir() else {
        return Vec::new();
    };
    let Ok(index) = rmw_index::for_dir(&cwd) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for name in index.known() {
        match index.resolve(&name) {
            Ok(r) => out.extend(r.names.iter().cloned().map(CompletionCandidate::new)),
            Err(_) => out.push(CompletionCandidate::new(name)),
        }
    }
    sorted(out)
}

/// Public knob names, as `[image.<id>.knobs]` keys them, each with its prompt.
/// Read from the registry directly rather than through `cmd::knob`, which
/// would pull that verb into the code `nros build` runs.
pub fn knobs() -> Vec<CompletionCandidate> {
    sorted(nros_zephyr_build::public_knobs().map(|r| {
        CompletionCandidate::new(r.image_key).help(r.prompt.map(|p| p.to_string().into()))
    }))
}

// The registration of dynamic completion. Here rather than in the
// `completions` verb so `nros setup` can print it without reaching that verb.

/// The shells `nros` completes in.
pub const SHELLS: [&str; 4] = ["bash", "zsh", "fish", "powershell"];

pub fn shell_from_env(shell_var: Option<&str>) -> Option<String> {
    let base = shell_var?.rsplit('/').next()?;
    SHELLS.iter().find(|s| **s == base).map(|s| s.to_string())
}

/// The line that registers dynamic completion for `shell`, to add to the
/// shell's startup file.
pub fn registration_line(shell: &str) -> Option<&'static str> {
    Some(match shell {
        "bash" => "source <(COMPLETE=bash nros)",
        "zsh" => "source <(COMPLETE=zsh nros)",
        "fish" => "COMPLETE=fish nros | source",
        "powershell" => {
            "$env:COMPLETE = \"powershell\"; nros | Out-String | Invoke-Expression; \
             Remove-Item Env:\\COMPLETE"
        }
        _ => return None,
    })
}

/// The startup file each shell reads, for the message around the line.
fn startup_file(shell: &str) -> &'static str {
    match shell {
        "bash" => "~/.bashrc",
        "zsh" => "~/.zshrc",
        "fish" => "~/.config/fish/config.fish",
        _ => "your PowerShell profile ($PROFILE)",
    }
}

/// The text `nros completions` prints: the line, and where it goes.
pub fn render(shell: &str) -> Result<String> {
    let line =
        registration_line(shell).ok_or_else(|| eyre!("`nros` does not complete in `{shell}`"))?;
    Ok(format!(
        "# Turn on `nros` completion: add this line to {}\n{line}\n",
        startup_file(shell)
    ))
}

/// Sorted by name, one candidate per name.
fn sorted(items: impl IntoIterator<Item = CompletionCandidate>) -> Vec<CompletionCandidate> {
    let mut v: Vec<CompletionCandidate> = items.into_iter().collect();
    v.sort_by(|a, b| a.get_value().cmp(b.get_value()));
    v.dedup_by(|a, b| a.get_value() == b.get_value());
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(c: &[CompletionCandidate]) -> Vec<String> {
        c.iter()
            .map(|c| c.get_value().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn knobs_offers_every_public_knob_with_a_summary() {
        let k = knobs();
        assert!(values(&k).iter().any(|n| n == "net.max_sockets"));
        assert_eq!(k.len(), nros_zephyr_build::public_knobs().count());
    }

    #[test]
    fn sorted_keeps_one_candidate_per_name() {
        let v = sorted(["b", "a", "b"].map(CompletionCandidate::new));
        assert_eq!(values(&v), ["a", "b"]);
    }

    #[test]
    fn every_shell_has_a_registration_line_naming_complete() {
        for s in SHELLS {
            let line = registration_line(s).expect(s);
            assert!(
                line.contains("COMPLETE") && line.contains("nros"),
                "{s}: {line}"
            );
        }
        assert!(registration_line("tcsh").is_none());
    }

    #[test]
    fn the_shell_defaults_to_the_one_shell_names() {
        assert_eq!(shell_from_env(Some("/usr/bin/zsh")).as_deref(), Some("zsh"));
        assert_eq!(shell_from_env(Some("fish")).as_deref(), Some("fish"));
        assert_eq!(shell_from_env(Some("/bin/tcsh")), None);
        assert_eq!(shell_from_env(None), None);
    }

    #[test]
    fn render_says_where_the_line_goes() {
        assert_eq!(
            render("bash").unwrap(),
            "# Turn on `nros` completion: add this line to ~/.bashrc\n\
             source <(COMPLETE=bash nros)\n"
        );
    }
}
