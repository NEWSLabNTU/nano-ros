//! One test per reason class in RFC-0135 D8's table that this check
//! reports today, and the collection itself.

use super::*;
use crate::builder::preflight::{Missing, PreflightRefusal};

fn caps(heap: bool, atomics: bool, threads: bool) -> BoardCapabilities {
    toml::from_str(&format!(
        "heap = {heap}\natomics = {atomics}\nthreads = {threads}\n"
    ))
    .unwrap()
}

fn codes(i: &Incompatible) -> Vec<&'static str> {
    i.reasons().iter().map(|r| r.diag().code()).collect()
}

/// D8 row "RMW and its capability needs": each need the board lacks is
/// one reason, naming both sides; a need it has is none.
#[test]
fn an_rmw_need_the_board_lacks_is_a_reason() {
    let needs = ["threads".to_string(), "heap".to_string()];
    let r = rmw_needs("cyclonedds", &needs, "acme", caps(true, true, false));
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].diag(), Diag::RmwNeedsCapability);
    let text = r[0].to_string();
    assert!(text.contains("rmw `cyclonedds` needs threads"), "{text}");
    assert!(
        text.contains("board `acme` states threads = false"),
        "{text}"
    );
    assert!(rmw_needs("cyclonedds", &needs, "acme", caps(true, true, true)).is_empty());
    assert!(rmw_needs("xrce", &[], "acme", caps(false, false, false)).is_empty());
}

/// D8 row "transports and links": a link the board does not list is a
/// reason; a board that lists none, and a custom transport, are not
/// compared.
#[test]
fn a_link_the_board_does_not_carry_is_a_reason() {
    let links = ["tcp".to_string(), "udp".to_string()];
    let r = link("talker", Some("serial"), "acme", &links).expect("refused");
    assert_eq!(r.diag(), Diag::LinkNotOnBoard);
    assert!(
        r.to_string()
            .contains("over serial; board `acme` has links [tcp, udp]")
    );
    assert!(link("talker", Some("tcp"), "acme", &links).is_none());
    assert!(link("talker", Some("serial"), "acme", &[]).is_none());
    assert!(link("talker", Some("custom"), "acme", &links).is_none());
    assert!(link("talker", None, "acme", &links).is_none());
}

/// D8 row "the BSP root the package names is set and exists".
#[test]
fn a_vendor_sdk_root_that_is_unset_or_missing_is_a_reason() {
    let unset = sdk_root("acme", Some("ACME_SDK_ROOT"), None).expect("refused");
    assert_eq!(unset.diag(), Diag::BoardSdkRootUnset);
    assert!(unset.to_string().contains("ACME_SDK_ROOT is not set"));
    assert!(unset.to_string().contains("export ACME_SDK_ROOT="));
    let gone = sdk_root("acme", Some("ACME_SDK_ROOT"), Some("/no/such/sdk")).expect("refused");
    assert!(
        gone.to_string()
            .contains("`/no/such/sdk`, which is not a directory")
    );
    let here = std::env::temp_dir();
    assert!(sdk_root("acme", Some("ACME_SDK_ROOT"), here.to_str()).is_none());
    assert!(
        sdk_root("acme", None, None).is_none(),
        "the board names no SDK"
    );
}

/// D8 row "toolchain and tools present": each missing tool is a reason
/// whose remedy is the command that installs it.
#[test]
fn a_missing_tool_is_a_reason_with_its_command() {
    let r = tools(vec![Missing {
        what: "Rust target `thumbv7m-none-eabi`".into(),
        remedy: "nros setup acme".into(),
    }]);
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].diag(), Diag::BoardToolMissing);
    assert!(r[0].to_string().contains("run `nros setup acme`"));
}

/// D8 row "platform, framework, entry type": a coded refusal from the
/// framework check keeps its own code (NROS-0014); a check that answers in
/// text (the build road) gets its row's.
#[test]
fn a_coded_refusal_keeps_its_code_and_text_gets_the_rows() {
    let mut i = Incompatible::new("talker", "acme", None);
    let framework: eyre::Result<()> = Err(Diag::ImageFrameworkRefused
        .refusal("image `talker`: board `acme` does not boot `rtic`")
        .report());
    i.collect(Diag::BoardNoBuildRoad, framework);
    let road: eyre::Result<()> = Err(eyre::eyre!("no road builds C for this board"));
    assert_eq!(i.collect(Diag::BoardNoBuildRoad, road), None);
    assert_eq!(i.collect(Diag::BoardNoBuildRoad, Ok(7)), Some(7));
    assert_eq!(
        codes(&i),
        [
            Diag::ImageFrameworkRefused.code(),
            Diag::BoardNoBuildRoad.code()
        ]
    );
}

/// The acceptance: every reason is reported at once, under one header that
/// names the board's origin, and the missing tools are listed with them.
#[test]
fn every_reason_is_reported_at_once() {
    let mut i = Incompatible::new(
        "talker",
        "mps2-an385-baremetal",
        Some(Path::new("boards/acme/nros-board.toml")),
    );
    i.extend(rmw_needs(
        "cyclonedds",
        &["threads".to_string()],
        "mps2-an385-baremetal",
        caps(true, true, false),
    ));
    i.extend(link(
        "talker",
        Some("serial"),
        "mps2-an385-baremetal",
        &["tcp".to_string()],
    ));
    i.missing([Missing {
        what: "arm-none-eabi-gcc".into(),
        remedy: "nros setup mps2-an385-baremetal".into(),
    }]);
    let e = i.into_result().unwrap_err();
    let text = e.to_string();
    assert!(
        text.starts_with(
            "error[NROS-0015]: image `talker` cannot build for board `mps2-an385-baremetal` \
             (3 problems)\n  board: boards/acme/nros-board.toml\n"
        ),
        "{text}"
    );
    for code in ["NROS-0017", "NROS-0018", "NROS-0019"] {
        assert!(text.contains(&format!("error[{code}]")), "{code} in {text}");
    }
    assert!(
        crate::builder::preflight::host_refusal(&e).is_none(),
        "a refusal of the image, not of this host"
    );
}

/// Only tools missing: the preflight's own refusal, unchanged, which
/// `nros configure` reads as a refusal of this host.
#[test]
fn only_missing_tools_stay_the_preflight_refusal() {
    let mut i = Incompatible::new("talker", "acme", None);
    let m = Missing {
        what: "Rust target `x`".into(),
        remedy: "rustup target add x".into(),
    };
    i.missing([m.clone()]);
    let e = i.into_result().unwrap_err();
    assert_eq!(
        e.downcast_ref::<PreflightRefusal>().map(|r| r.0.clone()),
        Some(vec![m])
    );
    assert!(Incompatible::new("t", "a", None).into_result().is_ok());
}

/// D8 row "memory" (ruled 2026-11-26): the stated floor against the board's
/// RAM, worded as a lower bound; no budget, or a floor that fits, is none.
#[test]
fn a_stated_floor_over_the_boards_ram_is_a_reason() {
    let floor = nros_sizing_descriptor::RamFloor {
        parts: vec![("sched task stacks", 65536), ("heap", 131072)],
    };
    let r = memory("acme", &floor, Some(131072)).expect("refused");
    assert_eq!(r.diag(), Diag::ImageOverMemoryBudget);
    let text = r.to_string();
    assert!(
        text.contains(
            "needs at least 196608 bytes of RAM (sched task stacks 65536 + heap 131072); \
             board `acme` has 131072"
        ),
        "{text}"
    );
    assert!(memory("acme", &floor, Some(196608)).is_none());
    assert!(
        memory("acme", &floor, None).is_none(),
        "no budget, no comparison"
    );
}
