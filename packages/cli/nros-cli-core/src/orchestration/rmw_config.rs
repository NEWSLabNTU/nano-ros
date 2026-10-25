//! RFC-0109 D8 / D9 (phase-514 W3) — a bringup's own RMW file, found by
//! convention and checked at sync.
//!
//! The file is the middleware's own language (layer 2) — for CycloneDDS the
//! XML `CYCLONEDDS_URI` names, for zenoh the json5 `ZENOH_SESSION_CONFIG_URI`
//! names. It lives beside the `system.toml` (a bringup's, or a single-package
//! leaf's) and applies to every image of it whose backend reads that language.
//! There is no key naming it: the backend is declared, and its descriptor
//! (`[rmw.config]` in `nros-rmw.toml`) names the file and how it is carried,
//! so the name follows (RFC-0087's "derived from convention"). Core never
//! learns either (RFC-0071): what reaches an image is the backend-neutral
//! session-property text `nros_orchestration_ir::session_properties` defines.
//!
//! The checks are RFC-0109 D9's, run here so every road refuses the same file
//! the same way:
//!
//! - a backend's retired file (`[rmw.config] retired`) is refused, naming the
//!   one that replaced it;
//! - an image that states a `locator` AND has a file is refused: the file is
//!   the session's wiring, and two homes for one endpoint disagree silently;
//! - a `verbatim-xml` `<Domain id>` other than `any` or the image's domain is
//!   refused;
//! - a `session-json5` key the target cannot take is refused (lowering it is
//!   the check).

use std::path::{Path, PathBuf};

use cargo_nano_ros::rmw_resolver::{RmwConfigFile, resolve_rmw};

/// An image's RMW file, found and checked.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RmwFile {
    /// The file.
    pub path: PathBuf,
    /// For a `session-json5` file, the session properties it lowers to, which
    /// the image bakes. Empty for a file the backend parses itself.
    pub properties: Vec<(String, String)>,
}

/// What backend `rmw`'s descriptor says about its configuration file, or
/// `None` for an undeclared or unknown backend, or one with no file format.
fn declared(rmw: Option<&str>) -> Option<RmwConfigFile> {
    let r = resolve_rmw(rmw?).ok()?;
    (!r.config.file.is_empty()).then_some(r.config)
}

/// The file backend `rmw` reads beside the `system.toml` in `dir`, if the
/// bringup ships one. No check runs; [`resolve`] is the checked form.
pub fn find(dir: &Path, rmw: Option<&str>) -> Option<PathBuf> {
    let p = dir.join(declared(rmw)?.file);
    p.is_file().then_some(p)
}

/// The RMW file of an image with backend `rmw` declared by the `system.toml`
/// in `dir`, checked against what else the image states. `None` when the
/// bringup ships none for that backend. `at` names the image (`image.<id>`).
///
/// `image_locator` is a locator stated by the image itself (or its
/// `[image_defaults]`), never `[system]`'s project-wide default, which an
/// image with a file simply does not use.
///
/// # Errors
///
/// One of the module's refusals.
pub fn resolve(
    dir: &Path,
    rmw: Option<&str>,
    image_locator: Option<&str>,
    domain_id: u32,
    at: &str,
) -> Result<Option<RmwFile>, String> {
    let Some(cfg) = declared(rmw) else {
        return Ok(None);
    };
    if !cfg.retired.is_empty() {
        let retired = dir.join(cfg.retired);
        if retired.is_file() {
            return Err(format!(
                "{}: this format is retired (RFC-0109 D8) — write the same settings in the \
                 middleware's own configuration language as `{}`, the document its ROS 2 \
                 implementation reads",
                retired.display(),
                cfg.file
            ));
        }
    }
    let path = dir.join(cfg.file);
    if !path.is_file() {
        return Ok(None);
    }
    if let Some(loc) = image_locator {
        return Err(format!(
            "`[{at}]` states `locator = \"{loc}\"` and its bringup ships `{}`: the file is \
             this session's wiring, so the endpoint has two homes (RFC-0109 D9) — state it in \
             the file, or drop the file",
            cfg.file
        ));
    }
    let properties = match cfg.format {
        "session-json5" => session_properties(&path)?,
        _ => {
            check_xml_domain(&path, domain_id, at)?;
            Vec::new()
        }
    };
    Ok(Some(RmwFile { path, properties }))
}

/// The session properties the `session-json5` file at `path` lowers to — the
/// one lowering every road runs (`nros codegen entry --rmw-config`, sync's
/// [`resolve`], and a hosted image's `ZENOH_SESSION_CONFIG_URI`).
///
/// # Errors
///
/// The file cannot be read, is not json5, or names a key the target cannot
/// take.
pub fn session_properties(path: &Path) -> Result<Vec<(String, String)>, String> {
    nros_zenoh_session_config::lower_file(path)
}

/// The `nros_orchestration_ir::session_properties` text image `image_id` of
/// the `system.toml` in `dir` bakes, or `None` when its bringup ships no
/// lowered RMW file — the value a cargo road writes into the image's build
/// settings, so `nros::main!` receives the properties and no backend name.
///
/// # Errors
///
/// The `system.toml` does not parse, or the file is refused ([`resolve`]).
pub fn session_text(dir: &Path, image_id: &str) -> Result<Option<String>, String> {
    let path = dir.join("system.toml");
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Ok(None);
    };
    let sys: crate::orchestration::cargo_metadata_schema::SystemToml =
        toml::from_str(&raw).map_err(|e| format!("{}: {e}", path.display()))?;
    let file = sys
        .rmw_file_for(dir, image_id)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(file
        .filter(|f| !f.properties.is_empty())
        .map(|f| nros_orchestration_ir::session_properties::encode(&f.properties)))
}

/// RFC-0109 D9 check 3 — a `verbatim-xml` (CycloneDDS) `<Domain id="…">`
/// other than `any` or the image's own domain configures a domain the image
/// never joins.
fn check_xml_domain(path: &Path, domain_id: u32, at: &str) -> Result<(), String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut rest = text.as_str();
    while let Some(i) = rest.find("<Domain") {
        rest = &rest[i + "<Domain".len()..];
        // `<DomainTag>` and friends are other elements.
        if !rest.starts_with([' ', '\t', '\n', '\r', '>', '/']) {
            continue;
        }
        let tag = &rest[..rest.find('>').unwrap_or(rest.len())];
        let Some(j) = tag.find("id=") else {
            continue;
        };
        let v = &tag[j + 3..];
        let Some(q) = v.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            continue;
        };
        let id = v[1..].split(q).next().unwrap_or("");
        if id != "any" && id.parse::<u32>().ok() != Some(domain_id) {
            return Err(format!(
                "{}: `<Domain id=\"{id}\">` configures a domain `[{at}]` never joins (it is on \
                 domain {domain_id}) — write `id=\"any\"` or `id=\"{domain_id}\"` (RFC-0109 D9)",
                path.display()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const ZENOH_FILE: &str = "rmw/zenoh.json5";
    const CYCLONEDDS_FILE: &str = "rmw/cyclonedds.xml";
    const RETIRED_ZENOH_CONF: &str = "rmw/zenoh.conf";

    fn dir(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nros-rmwcfg-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("rmw")).unwrap();
        for (f, body) in files {
            std::fs::write(d.join(f), body).unwrap();
        }
        d
    }

    #[test]
    fn a_zenoh_file_lowers_and_is_the_images_only_endpoint() {
        let d = dir(
            "zenoh",
            &[(
                ZENOH_FILE,
                "{ connect: { endpoints: ['tcp/127.0.0.1:7500'] } }",
            )],
        );
        let f = resolve(&d, Some("zenoh"), None, 0, "image.a")
            .unwrap()
            .unwrap();
        assert_eq!(
            f.properties,
            [("connect".to_string(), "tcp/127.0.0.1:7500".to_string())]
        );
        // Another backend's image does not read it.
        assert_eq!(resolve(&d, Some("xrce"), None, 0, "image.a").unwrap(), None);
        let e = resolve(&d, Some("zenoh"), Some("tcp/x:1"), 0, "image.a").unwrap_err();
        assert!(e.contains("two homes") && e.contains("[image.a]"), "{e}");
        std::fs::write(d.join(ZENOH_FILE), "{ adminspace: { enabled: true } }").unwrap();
        let e = resolve(&d, Some("zenoh"), None, 0, "image.a").unwrap_err();
        assert!(
            e.contains("adminspace.enabled") && e.contains("zenoh.json5"),
            "{e}"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_flat_zenoh_conf_is_refused_naming_the_json5() {
        let d = dir("conf", &[(RETIRED_ZENOH_CONF, "connect = tcp/x:1\n")]);
        let e = resolve(&d, Some("zenoh"), None, 0, "image.a").unwrap_err();
        assert!(
            e.contains("retired") && e.contains("rmw/zenoh.json5"),
            "{e}"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_cyclone_domain_id_must_be_any_or_the_images() {
        let xml = |id: &str| {
            format!("<CycloneDDS><Domain id=\"{id}\"><General/></Domain><DomainTag/></CycloneDDS>")
        };
        for (id, ok) in [("any", true), ("3", true), ("4", false)] {
            let d = dir(&format!("cy{id}"), &[(CYCLONEDDS_FILE, &xml(id))]);
            let r = resolve(&d, Some("cyclonedds"), None, 3, "image.a");
            assert_eq!(r.is_ok(), ok, "{id}: {r:?}");
            if !ok {
                assert!(r.unwrap_err().contains("never joins"));
            }
            let _ = std::fs::remove_dir_all(&d);
        }
    }

    /// The text a cargo road hands `nros::main!`: the image's lowered file,
    /// for the image whose backend reads it, refusing a locator beside it.
    #[test]
    fn session_text_is_the_lowered_file_for_the_images_that_read_it() {
        let d = dir(
            "text",
            &[
                (
                    ZENOH_FILE,
                    "{ mode: 'client', connect: { endpoints: ['tcp/10.0.2.2:7500'] } }",
                ),
                (
                    "system.toml",
                    "[system]\nname = \"s\"\nrmw = \"zenoh\"\ndomain_id = 0\n\
                     [image.a]\nboard = \"linux\"\n\
                     [image.b]\nboard = \"linux\"\nrmw = \"xrce\"\n\
                     [image.c]\nboard = \"linux\"\nlocator = \"tcp/x:1\"\n",
                ),
            ],
        );
        let text = session_text(&d, "a")
            .unwrap()
            .expect("image a reads the file");
        assert_eq!(
            nros_orchestration_ir::session_properties::decode(&text).unwrap(),
            [
                ("connect".to_string(), "tcp/10.0.2.2:7500".to_string()),
                ("mode".to_string(), "client".to_string()),
            ]
        );
        assert_eq!(
            session_text(&d, "b").unwrap(),
            None,
            "an XRCE image reads no json5"
        );
        let e = session_text(&d, "c").unwrap_err();
        assert!(e.contains("[image.c]") && e.contains("two homes"), "{e}");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The tree's own CycloneDDS files pass the domain check for their images.
    #[test]
    fn the_tracked_cyclone_files_name_a_domain_their_images_join() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
        // The descriptor names the file this test reads.
        assert_eq!(declared(Some("cyclonedds")).unwrap().file, CYCLONEDDS_FILE);
        for bringup in [
            "examples/workspaces/cpp/src/demo_bringup",
            "examples/workspaces/cpp/src/slirp_bringup",
        ] {
            let text = std::fs::read_to_string(root.join(bringup).join(CYCLONEDDS_FILE)).unwrap();
            assert!(
                text.contains("id=\"any\"") || !text.contains("<Domain "),
                "{bringup}: its Domain id is not `any`; this test then needs the image's domain"
            );
        }
    }
}
