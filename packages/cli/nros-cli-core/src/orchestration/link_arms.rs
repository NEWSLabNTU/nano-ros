//! phase-560 W9.d -- an image's session endpoints against its RMW's port row.
//!
//! zenoh-pico's CAN and ISO-TP links are compiled per platform: the arm that
//! opens one (`_z_open_can`, `_z_open_isotp`) exists only where a platform
//! macro selects it (`ZENOH_LINUX`, `ZENOH_ZEPHYR`). The link's generic half
//! compiles everywhere, so an image that dials `can/vcan0` on a platform with
//! no arm configured, built every object, and failed at the final link with
//! `undefined reference to _z_open_can` (measured on `freertos-posix`).
//!
//! The RMW's port row states which of these links its platform has an arm
//! for (`[port.<platform>] links`, `nros_platform_config::manifest::PortRow`),
//! and [`check`] refuses an endpoint on a link the row does not list, at
//! configure, naming the image, the endpoint, the platform and the row.

use std::path::Path;

use nros_board_common::manifest::{PORT_LINKS, PortRows};

/// The `connect` / `listen` endpoints of a lowered RMW file
/// (`nros_orchestration_ir::session_properties` text).
pub fn session_endpoints(text: Option<&str>) -> Result<Vec<String>, String> {
    let Some(text) = text else {
        return Ok(Vec::new());
    };
    Ok(nros_orchestration_ir::session_properties::decode(text)?
        .into_iter()
        .filter(|(k, _)| k == "connect" || k == "listen")
        .map(|(_, v)| v)
        .collect())
}

/// The [`PORT_LINKS`] scheme `endpoint` dials, if it is one.
fn port_link(endpoint: &str) -> Option<&'static str> {
    let scheme = endpoint.split('/').next()?;
    PORT_LINKS.iter().copied().find(|l| *l == scheme)
}

/// What [`check`] needs to know about the image.
pub struct ImageLink<'a> {
    /// The image id.
    pub image: &'a str,
    /// The board, as the image names it.
    pub board: &'a str,
    /// The board's platform (`freertos`, `threadx-linux`).
    pub platform: &'a str,
    /// The image's RMW.
    pub rmw: &'a str,
    /// Its locator and its RMW file's endpoints.
    pub endpoints: &'a [String],
}

/// Refuse an endpoint on a link the RMW's port row for this platform lists no
/// arm for. `same_platform(row key, platform)` answers whether a row's key
/// names the image's platform (an alias resolves to its platform). An RMW
/// with no port rows builds no ported C, and a platform with no row is not
/// this check's to judge: both pass.
pub fn check(
    img: &ImageLink<'_>,
    ports: &PortRows,
    same_platform: impl Fn(&str, &str) -> bool,
) -> Result<(), String> {
    let Some((key, row)) = ports.iter().find(|(k, _)| same_platform(k, img.platform)) else {
        return Ok(());
    };
    let links = row.checked_links(key)?;
    for e in img.endpoints {
        let Some(link) = port_link(e) else { continue };
        if links.iter().any(|l| l == link) {
            continue;
        }
        let with: Vec<&str> = ports
            .iter()
            .filter(|(_, r)| r.links.iter().any(|l| l == link))
            .map(|(k, _)| k.as_str())
            .collect();
        return Err(format!(
            "image `{}` dials `{e}`, but rmw `{}` has no `{link}` link on platform `{}` \
             (board `{}`): its port row `[port.{key}]` lists no arm for it (`links = [{}]`), \
             so the image would fail at the final link (`undefined reference to \
             _z_open_{link}`). The link has an arm on: {}. Dial over another link, or send \
             raw frames through the CAN frame driver (`<nros/platform_can.h>`)",
            img.image,
            img.rmw,
            img.platform,
            img.board,
            links
                .iter()
                .map(|l| format!("{l:?}"))
                .collect::<Vec<_>>()
                .join(", "),
            if with.is_empty() {
                "no platform".to_string()
            } else {
                with.join(", ")
            }
        ));
    }
    Ok(())
}

/// [`check`] over the descriptors on disk: the RMW resolved over the index of
/// `dir` (the bringup or the leaf), its `nros-rmw.toml`'s port rows, and the
/// platform tree of `nano_ros_root`. Reads nothing when no endpoint is on a
/// [`PORT_LINKS`] link.
pub fn check_image(
    img: &ImageLink<'_>,
    dir: &Path,
    nano_ros_root: Option<&Path>,
) -> Result<(), String> {
    if !img.endpoints.iter().any(|e| port_link(e).is_some()) {
        return Ok(());
    }
    let index = super::rmw_index::for_dir(dir).map_err(|e| format!("{e}"))?;
    let Ok(rmw) = index.resolve(img.rmw) else {
        // An unresolved RMW is the build's own error to report.
        return Ok(());
    };
    let path = rmw.package_dir.join("nros-rmw.toml");
    let ports = nros_board_common::manifest::load_port_rows(&path)?;
    if ports.is_empty() {
        return Ok(());
    }
    let Some(root) = nano_ros_root
        .map(Path::to_path_buf)
        .or_else(|| super::nano_ros_root::resolve(None, dir))
    else {
        return Ok(());
    };
    use nros_board_common::platform_config::PlatformsTree;
    let tree = PlatformsTree::load_search_path(&PlatformsTree::default_search_path(
        &root,
        std::env::var("NROS_PLATFORMS_DIR").ok().as_deref(),
    ))
    .map_err(|e| format!("{e}"))?;
    check(img, &ports, |key, platform| {
        tree.resolve_alias(key) == tree.resolve_alias(platform)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..")
    }

    fn zenoh_rows() -> PortRows {
        nros_board_common::manifest::load_port_rows(
            &repo().join("packages/rmw/zenoh/nros-rmw-zenoh/nros-rmw.toml"),
        )
        .unwrap()
    }

    fn run(board: &str, platform: &str, endpoint: &str) -> Result<(), String> {
        use nros_board_common::platform_config::PlatformsTree;
        let tree =
            PlatformsTree::load_search_path(&PlatformsTree::default_search_path(&repo(), None))
                .unwrap();
        let endpoints = vec![endpoint.to_string()];
        check(
            &ImageLink {
                image: "a",
                board,
                platform,
                rmw: "zenoh",
                endpoints: &endpoints,
            },
            &zenoh_rows(),
            |key, p| tree.resolve_alias(key) == tree.resolve_alias(p),
        )
    }

    /// phase-560 W9.d -- a zenoh `can/` or `isotp/` endpoint on
    /// `freertos-posix` or `threadx-linux`, whose zenoh-pico port has no CAN
    /// arm (the build failed at its final link, `undefined reference to
    /// _z_open_can`), is refused at configure; on `linux` and Zephyr it
    /// passes.
    #[test]
    fn a_can_endpoint_on_a_port_with_no_can_arm_is_refused() {
        for (board, platform) in [
            ("freertos-posix", "freertos"),
            ("threadx-linux", "threadx-linux"),
        ] {
            for ep in ["can/vcan0#id=0x100", "isotp/vcan0#tx_id=0x101;rx_id=0x181"] {
                let e = run(board, platform, ep).expect_err(board);
                assert!(
                    e.contains(&format!("board `{board}`"))
                        && e.contains("lists no arm")
                        && e.contains("posix")
                        && e.contains("zephyr"),
                    "{e}"
                );
            }
            run(board, platform, "tcp/127.0.0.1:7447").unwrap();
        }
        for (board, platform) in [("linux", "posix"), ("zephyr", "zephyr")] {
            run(board, platform, "can/vcan0#id=0x100").unwrap();
            run(board, platform, "isotp/vcan0#tx_id=0x101;rx_id=0x181").unwrap();
        }
    }

    #[test]
    fn a_port_row_link_outside_the_vocabulary_is_refused() {
        let rows =
            nros_board_common::manifest::parse_port_rows("[port.posix]\nlinks = [\"tcp\"]\n", "t")
                .unwrap();
        let endpoints = vec!["can/vcan0".to_string()];
        let e = check(
            &ImageLink {
                image: "a",
                board: "linux",
                platform: "posix",
                rmw: "zenoh",
                endpoints: &endpoints,
            },
            &rows,
            |k, p| k == p,
        )
        .unwrap_err();
        assert!(e.contains("links names \"tcp\""), "{e}");
    }

    #[test]
    fn the_session_endpoints_are_its_connect_and_listen_rows() {
        assert!(session_endpoints(None).unwrap().is_empty());
    }
}
