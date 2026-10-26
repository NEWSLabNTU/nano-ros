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
    /// How the backend's descriptor says the file is carried
    /// (`[rmw.config] format`: `session-json5`, `verbatim-xml`).
    pub format: String,
}

impl RmwFile {
    /// What a session opened on this file hands its backend, for a session
    /// that is not the image's default one (a `[[domain]]` a bridge opens):
    /// the lowered properties of a `session-json5` file, or — for a file the
    /// backend parses itself — its whole text as one
    /// [`nros_orchestration_ir::session_properties::DOCUMENT`] property. The
    /// default session's document is baked by its road instead
    /// (`nros_bake_rmw_user_config`).
    ///
    /// # Errors
    ///
    /// The file cannot be read.
    pub fn session_properties(&self) -> Result<Vec<(String, String)>, String> {
        if !self.properties.is_empty() {
            return Ok(self.properties.clone());
        }
        let text = std::fs::read_to_string(&self.path)
            .map_err(|e| format!("{}: {e}", self.path.display()))?;
        Ok(vec![(
            nros_orchestration_ir::session_properties::DOCUMENT.to_string(),
            text,
        )])
    }
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

/// The RMW file of a session on backend `rmw` declared by the `system.toml`
/// in `dir`, checked against what else the session states. `explicit` is a
/// file the bringup NAMES (`[image] rmw_config`, `[system] rmw_config`,
/// `[[domain]] config`), relative to `dir`; without one the backend's
/// conventional file is used when the bringup ships it. `None` when there is
/// no file. `at` names the table (`image.<id>`, `domain.<name>`).
///
/// `image_locator` is a locator stated by the session itself (the image or its
/// `[image_defaults]`, or the domain), never `[system]`'s project-wide
/// default, which an image with a file simply does not use.
///
/// # Errors
///
/// One of the module's refusals.
pub fn resolve(
    dir: &Path,
    rmw: Option<&str>,
    explicit: Option<&str>,
    image_locator: Option<&str>,
    domain_id: u32,
    at: &str,
) -> Result<Option<RmwFile>, String> {
    // RFC-0109 D10 — whatever the backend, a locator is wiring: a CAN rate in
    // one is a device fact with a second home. The file's own endpoints are
    // refused by the same rule as they lower.
    if let Some(loc) = image_locator {
        nros_zenoh_session_config::refuse_device_facts(loc)
            .map_err(|e| format!("`[{at}]`: {e}"))?;
    }
    let Some(cfg) = declared(rmw) else {
        if let Some(f) = explicit {
            return Err(format!(
                "`[{at}]` names the RMW file `{f}`, but backend `{}` reads no configuration \
                 file (RFC-0109 D8) — state its endpoint as `locator` instead",
                rmw.unwrap_or("?")
            ));
        }
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
    let name = explicit.unwrap_or(cfg.file);
    let path = dir.join(name);
    if !path.is_file() {
        if explicit.is_some() {
            return Err(format!(
                "`[{at}]` names the RMW file `{name}`, which does not exist beside {}",
                dir.join("system.toml").display()
            ));
        }
        return Ok(None);
    }
    if let Some(loc) = image_locator {
        return Err(format!(
            "`[{at}]` states `locator = \"{loc}\"` and has the RMW file `{name}`: the file is \
             this session's wiring, so the endpoint has two homes (RFC-0109 D9) — state it in \
             the file, or drop the file"
        ));
    }
    let properties = match cfg.format {
        "session-json5" => session_properties(&path)?,
        _ => {
            check_xml_domain(&path, domain_id, at)?;
            Vec::new()
        }
    };
    Ok(Some(RmwFile {
        path,
        properties,
        format: cfg.format.to_string(),
    }))
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

/// The default session's RMW file for backend `rmw` in the bringup at `dir`
/// — the question a CMake road asks (`nros ws rmw-config`), which knows the
/// backend it links but not always the image. With `image`, that image's file
/// ([`SystemToml::rmw_file_for`]'s rule, `rmw` standing for the backend the
/// road resolved). Without one, the file every image on `rmw` agrees on, or
/// the bringup's own (`[system] rmw_config`, else the conventional file) when
/// no image is on `rmw`; images that disagree are refused, naming
/// `-DNROS_IMAGE`, rather than one of them winning.
///
/// [`SystemToml::rmw_file_for`]: crate::orchestration::cargo_metadata_schema::SystemToml::rmw_file_for
///
/// # Errors
///
/// The `system.toml` does not parse, the file is refused ([`resolve`]), or
/// the images disagree.
pub fn default_session_file(
    dir: &Path,
    rmw: &str,
    image: Option<&str>,
) -> Result<Option<RmwFile>, String> {
    let path = dir.join("system.toml");
    let raw = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let sys: crate::orchestration::cargo_metadata_schema::SystemToml =
        toml::from_str(&raw).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut chosen: Option<(String, Option<RmwFile>)> = None;
    for (id, img) in sys.resolved_images() {
        if image.is_some_and(|want| want != id) {
            continue;
        }
        let img_rmw = img.rmw.as_deref().unwrap_or(sys.system.rmw.as_str());
        if image.is_none() && img_rmw != rmw {
            continue;
        }
        let explicit = img
            .rmw_config
            .as_ref()
            .and_then(crate::orchestration::image::RmwConfigSel::default_file)
            .or(sys.system.rmw_config.as_deref());
        let file = resolve(
            dir,
            Some(rmw),
            explicit,
            img.locator.as_deref(),
            img.domain_id.unwrap_or(sys.system.domain_id),
            &format!("image.{id}"),
        )?;
        let shown = |f: &Option<RmwFile>| {
            f.as_ref()
                .map_or_else(|| "none".to_string(), |f| f.path.display().to_string())
        };
        if let Some((other, other_file)) = &chosen
            && other_file.as_ref().map(|f| &f.path) != file.as_ref().map(|f| &f.path)
        {
            return Err(format!(
                "{}: images `{other}` and `{id}` have different RMW files for `{rmw}` ({} vs \
                 {}) — configure with `-DNROS_IMAGE=<id>` to say which one this build is",
                path.display(),
                shown(other_file),
                shown(&file),
            ));
        }
        chosen = Some((id, file));
    }
    match (chosen, image) {
        (Some((_, file)), _) => Ok(file),
        (None, Some(want)) => Err(format!("{}: no `[image.{want}]`", path.display())),
        (None, None) => resolve(
            dir,
            Some(rmw),
            sys.system.rmw_config.as_deref(),
            None,
            sys.system.domain_id,
            "system",
        ),
    }
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
        let f = resolve(&d, Some("zenoh"), None, None, 0, "image.a")
            .unwrap()
            .unwrap();
        assert_eq!(
            f.properties,
            [("connect".to_string(), "tcp/127.0.0.1:7500".to_string())]
        );
        // Another backend's image does not read it.
        assert_eq!(
            resolve(&d, Some("xrce"), None, None, 0, "image.a").unwrap(),
            None
        );
        let e = resolve(&d, Some("zenoh"), None, Some("tcp/x:1"), 0, "image.a").unwrap_err();
        assert!(e.contains("two homes") && e.contains("[image.a]"), "{e}");
        std::fs::write(d.join(ZENOH_FILE), "{ adminspace: { enabled: true } }").unwrap();
        let e = resolve(&d, Some("zenoh"), None, None, 0, "image.a").unwrap_err();
        assert!(
            e.contains("adminspace.enabled") && e.contains("zenoh.json5"),
            "{e}"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_flat_zenoh_conf_is_refused_naming_the_json5() {
        let d = dir("conf", &[(RETIRED_ZENOH_CONF, "connect = tcp/x:1\n")]);
        let e = resolve(&d, Some("zenoh"), None, None, 0, "image.a").unwrap_err();
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
            let r = resolve(&d, Some("cyclonedds"), None, None, 3, "image.a");
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

    fn sys(d: &Path) -> crate::orchestration::cargo_metadata_schema::SystemToml {
        toml::from_str(&std::fs::read_to_string(d.join("system.toml")).unwrap()).unwrap()
    }

    const ZEN_LAB: &str = "{ connect: { endpoints: ['tcp/10.0.0.1:7447'] } }";
    const ZEN_QEMU: &str = "{ connect: { endpoints: ['tcp/10.0.2.2:7447'] } }";
    const CYC_ANY: &str = "<CycloneDDS><Domain id=\"any\"><General/></Domain></CycloneDDS>";

    /// RFC-0109 D6 (phase-514 W3b) — the default session's file: the image's
    /// `rmw_config`, else `[system] rmw_config`, else the convention; a named
    /// file that is absent is refused rather than read as "no file".
    #[test]
    fn the_default_sessions_file_is_the_images_then_the_bringups_then_the_convention() {
        let d = dir(
            "default-file",
            &[
                (ZENOH_FILE, ZEN_QEMU),
                ("rmw/zenoh-lab.json5", ZEN_LAB),
                (
                    "system.toml",
                    "[system]\nname = \"s\"\nrmw = \"zenoh\"\ndomain_id = 0\n\
                     [image.qemu]\nboard = \"linux\"\n\
                     [image.lab]\nboard = \"linux\"\nrmw_config = \"rmw/zenoh-lab.json5\"\n",
                ),
            ],
        );
        let s = sys(&d);
        let ep = |id: &str| s.rmw_file_for(&d, id).unwrap().unwrap().properties;
        assert_eq!(ep("qemu")[0].1, "tcp/10.0.2.2:7447", "the convention");
        assert_eq!(ep("lab")[0].1, "tcp/10.0.0.1:7447", "the image's own file");

        std::fs::write(
            d.join("system.toml"),
            "[system]\nname = \"s\"\nrmw = \"zenoh\"\ndomain_id = 0\n\
             rmw_config = \"rmw/zenoh-lab.json5\"\n[image.qemu]\nboard = \"linux\"\n",
        )
        .unwrap();
        let s = sys(&d);
        assert_eq!(
            s.rmw_file_for(&d, "qemu").unwrap().unwrap().properties[0].1,
            "tcp/10.0.0.1:7447",
            "the bringup's `[system] rmw_config` outranks the convention"
        );

        std::fs::write(
            d.join("system.toml"),
            "[system]\nname = \"s\"\nrmw = \"zenoh\"\ndomain_id = 0\n\
             [image.qemu]\nboard = \"linux\"\nrmw_config = \"rmw/nope.json5\"\n",
        )
        .unwrap();
        let e = sys(&d).rmw_file_for(&d, "qemu").unwrap_err();
        assert!(
            e.contains("rmw/nope.json5") && e.contains("does not exist"),
            "{e}"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// RFC-0109 D10 (phase-514 W5) — a CAN rate is the controller's, so a
    /// locator stating one is refused wherever it is wiring: the image's own,
    /// `[system]`'s default, and an endpoint in the RMW file. The wiring keys
    /// beside it are not.
    #[test]
    fn a_can_rate_in_any_locator_is_refused_naming_where_it_was() {
        let base = "[system]\nname = \"s\"\nrmw = \"zenoh\"\ndomain_id = 0\n";
        for (body, file, at) in [
            (
                format!(
                    "{base}[image.a]\nboard = \"linux\"\n\
                     locator = \"isotp/vcan0#tx_id=0x200;rx_id=0x201;bitrate=500000\"\n"
                ),
                None,
                "`[image.a]`",
            ),
            (
                format!(
                    "{base}locator = \"can/can0#dbitrate=2000000;id=0x100\"\n\
                     [image.a]\nboard = \"linux\"\n"
                ),
                None,
                "`[system]`",
            ),
            (
                format!("{base}[image.a]\nboard = \"linux\"\n"),
                Some("{ connect: { endpoints: ['can/can0#bitrate=500000'] } }"),
                "connect.endpoints",
            ),
        ] {
            let mut files = vec![("system.toml", body.as_str())];
            if let Some(f) = file {
                files.push((ZENOH_FILE, f));
            }
            let d = dir("can-rate", &files);
            let e = sys(&d).rmw_file_for(&d, "a").unwrap_err();
            assert!(e.contains(at) && e.contains("RFC-0109 D10"), "{at}\n-> {e}");
            let _ = std::fs::remove_dir_all(&d);
        }
        let d = dir(
            "can-wiring",
            &[(
                "system.toml",
                "[system]\nname = \"s\"\nrmw = \"zenoh\"\ndomain_id = 0\n\
                 [image.a]\nboard = \"linux\"\nlocator = \"isotp/vcan0#tx_id=0x200;rx_id=0x201\"\n",
            )],
        );
        assert_eq!(sys(&d).rmw_file_for(&d, "a").unwrap(), None);
        let _ = std::fs::remove_dir_all(&d);
    }

    /// A `[[domain]]`'s `config` is checked against ITS backend and id, may not
    /// sit beside a `locator`, and is refused for a backend with no file
    /// format; an image's per-domain override names a declared domain.
    #[test]
    fn a_domain_file_is_checked_against_its_own_backend_and_id() {
        let base = "[system]\nname = \"s\"\nrmw = \"zenoh\"\ndomain_id = 0\n\
                    [[bridge]]\nname = \"gw\"\nfrom = \"zenoh:ecu\"\nto = \"ctl\"\n\
                    [[bridge]]\nname = \"x\"\nfrom = \"a\"\nto = \"main\"\n\
                    [image.gw]\nboard = \"linux\"\n";
        let d = dir(
            "domain-file",
            &[
                ("rmw/zenoh-can.json5", ZEN_LAB),
                ("rmw/cyclonedds-eth.xml", CYC_ANY),
                (
                    "rmw/cyclonedds-4.xml",
                    "<CycloneDDS><Domain id=\"4\"/></CycloneDDS>",
                ),
            ],
        );
        let write = |extra: &str| {
            std::fs::write(d.join("system.toml"), format!("{base}{extra}")).unwrap();
            sys(&d)
        };
        let s = write(
            "[[domain]]\nname = \"ecu\"\nrmw = \"zenoh\"\nconfig = \"rmw/zenoh-can.json5\"\n\
             [[domain]]\nname = \"ctl\"\nrmw = \"cyclonedds\"\nid = 5\n\
             config = \"rmw/cyclonedds-eth.xml\"\n",
        );
        s.check_rmw_files(&d).unwrap();
        let ecu = s.domain_file(&d, "ecu", None).unwrap().unwrap();
        assert_eq!(ecu.session_properties().unwrap()[0].1, "tcp/10.0.0.1:7447");
        let ctl = s.domain_file(&d, "ctl", None).unwrap().unwrap();
        assert_eq!(
            ctl.session_properties().unwrap(),
            [(
                nros_orchestration_ir::session_properties::DOCUMENT.to_string(),
                CYC_ANY.to_string()
            )],
            "a document the backend parses itself travels whole"
        );

        let e = write(
            "[[domain]]\nname = \"ctl\"\nrmw = \"cyclonedds\"\nid = 5\n\
             config = \"rmw/cyclonedds-4.xml\"\n",
        )
        .check_rmw_files(&d)
        .unwrap_err();
        assert!(
            e.contains("never joins") && e.contains("[domain.ctl]"),
            "{e}"
        );

        let e = write(
            "[[domain]]\nname = \"ecu\"\nrmw = \"zenoh\"\nlocator = \"tcp/x:1\"\n\
             config = \"rmw/zenoh-can.json5\"\n",
        )
        .check_rmw_files(&d)
        .unwrap_err();
        assert!(e.contains("two homes") && e.contains("[domain.ecu]"), "{e}");

        let e =
            write("[[domain]]\nname = \"a\"\nrmw = \"xrce\"\nconfig = \"rmw/zenoh-can.json5\"\n")
                .check_rmw_files(&d)
                .unwrap_err();
        assert!(
            e.contains("reads no configuration file") && e.contains("locator"),
            "{e}"
        );

        let e = write("[image.gw.rmw_config]\nmain = \"rmw/zenoh-can.json5\"\n")
            .check_rmw_files(&d)
            .unwrap_err();
        assert!(e.contains("names domain `main`"), "{e}");

        let e = write(
            "[[domain]]\nname = \"lone\"\nrmw = \"zenoh\"\nconfig = \"rmw/zenoh-can.json5\"\n",
        )
        .check_rmw_files(&d)
        .unwrap_err();
        assert!(
            e.contains("no `[[bridge]]` opens") && e.contains("[system] rmw_config"),
            "a file nothing reads is refused: {e}"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    /// U9/U11 — an image overrides one domain's file; another image keeps the
    /// domain's own. Refused at sync until a bridge config is per image
    /// (issue 2062).
    #[test]
    fn an_image_overrides_one_domains_file() {
        let d = dir(
            "domain-override",
            &[
                ("rmw/zenoh-can.json5", ZEN_QEMU),
                ("rmw/zenoh-lab.json5", ZEN_LAB),
                (
                    "system.toml",
                    "[system]\nname = \"s\"\nrmw = \"zenoh\"\ndomain_id = 0\n\
                     [[domain]]\nname = \"main\"\nrmw = \"zenoh\"\n\
                     config = \"rmw/zenoh-can.json5\"\n\
                     [[domain]]\nname = \"dds\"\nrmw = \"cyclonedds\"\nid = 5\n\
                     [[bridge]]\nname = \"gw\"\nfrom = \"main\"\nto = \"dds\"\n\
                     [image.qemu]\nboard = \"linux\"\n\
                     [image.lab]\nboard = \"linux\"\n\
                     [image.lab.rmw_config]\nmain = \"rmw/zenoh-lab.json5\"\n",
                ),
            ],
        );
        let s = sys(&d);
        let e = s.check_rmw_files(&d).unwrap_err();
        assert!(
            e.contains("not delivered yet") && e.contains("issue 2062"),
            "checked, then refused while nothing carries it: {e}"
        );
        // The resolver itself already answers per image, for when it is.
        let ep = |img: Option<&str>| {
            s.domain_file(&d, "main", img).unwrap().unwrap().properties[0]
                .1
                .clone()
        };
        assert_eq!(ep(Some("lab")), "tcp/10.0.0.1:7447");
        assert_eq!(ep(Some("qemu")), "tcp/10.0.2.2:7447");
        assert_eq!(ep(None), "tcp/10.0.2.2:7447");
        let _ = std::fs::remove_dir_all(&d);
    }

    /// The CMake road's question: the file for a backend, per image or agreed
    /// across the images on it, and refused — naming `-DNROS_IMAGE` — when
    /// they disagree.
    #[test]
    fn the_cmake_road_asks_per_image_or_for_the_agreed_file() {
        let d = dir(
            "cmake-query",
            &[
                (CYCLONEDDS_FILE, CYC_ANY),
                ("rmw/cyclonedds-lab.xml", CYC_ANY),
                (
                    "system.toml",
                    "[system]\nname = \"s\"\nrmw = \"cyclonedds\"\ndomain_id = 0\n\
                     [image.a]\nboard = \"linux\"\n\
                     [image.b]\nboard = \"linux\"\nrmw_config = \"rmw/cyclonedds.xml\"\n\
                     [image.z]\nboard = \"linux\"\nrmw = \"zenoh\"\n",
                ),
            ],
        );
        let f = default_session_file(&d, "cyclonedds", None)
            .unwrap()
            .unwrap();
        assert_eq!(f.path, d.join(CYCLONEDDS_FILE), "a and b agree by path");
        assert_eq!(f.format, "verbatim-xml");
        assert_eq!(default_session_file(&d, "zenoh", None).unwrap(), None);

        std::fs::write(
            d.join("system.toml"),
            "[system]\nname = \"s\"\nrmw = \"cyclonedds\"\ndomain_id = 0\n\
             [image.a]\nboard = \"linux\"\n\
             [image.b]\nboard = \"linux\"\nrmw_config = \"rmw/cyclonedds-lab.xml\"\n",
        )
        .unwrap();
        let e = default_session_file(&d, "cyclonedds", None).unwrap_err();
        assert!(
            e.contains("-DNROS_IMAGE") && e.contains("`a`") && e.contains("`b`"),
            "{e}"
        );
        let b = default_session_file(&d, "cyclonedds", Some("b"))
            .unwrap()
            .unwrap();
        assert_eq!(b.path, d.join("rmw/cyclonedds-lab.xml"));
        assert!(default_session_file(&d, "cyclonedds", Some("q")).is_err());
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
