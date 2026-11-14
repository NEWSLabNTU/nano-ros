use std::{format, string::String, vec::Vec};

use crate::{ALL, Diag, Entry, Surface, by_number, lookup, nearest, parse_code, views};

/// `check-user-surface-ids`' pattern (scripts/check/check-user-surface-ids.py
/// `ID`), spelled without a regex engine: a phase, an issue, a `#NNN`, or an
/// RFC id. The gate's own self-test asserts that `NROS-NNNN` is none of them.
fn ids(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    let lower = text.to_ascii_lowercase();
    let b = lower.as_bytes();
    let digits_at = |i: usize| b[i..].iter().take_while(|c| c.is_ascii_digit()).count();
    let word_start = |i: usize| i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_');
    for (i, _) in lower.char_indices() {
        for (word, lo) in [("phase", 2), ("issue", 3), ("issues", 3)] {
            if lower[i..].starts_with(word) && word_start(i) {
                let j = i + word.len();
                if j < b.len() && (b[j] == b'-' || b[j] == b' ') {
                    let k = j + 1 + usize::from(word != "phase" && b.get(j + 1) == Some(&b'#'));
                    let n = digits_at(k);
                    if (lo..=4).contains(&n) {
                        found.push(String::from(&text[i..k + n]));
                    }
                }
            }
        }
        if lower[i..].starts_with("rfc") && word_start(i) {
            let k = i + 3 + usize::from(b.get(i + 3) == Some(&b'-'));
            if digits_at(k) == 4 {
                found.push(String::from(&text[i..k + 4]));
            }
        }
        if b[i] == b'#'
            && (i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b"_&/#".contains(&b[i - 1])))
            && (3..=4).contains(&digits_at(i + 1))
        {
            found.push(String::from(&text[i..i + 1 + digits_at(i + 1)]));
        }
    }
    found
}

#[test]
fn the_id_pattern_matches_what_the_gate_matches() {
    assert_eq!(ids("see phase-552 W2"), ["phase-552"]);
    assert_eq!(ids("issue 2305"), ["issue 2305"]);
    assert_eq!(ids("RFC-0129 D2"), ["RFC-0129"]);
    assert_eq!(ids("PR #472"), ["#472"]);
    assert!(ids("error[NROS-0412]: x\n  explain: nros explain NROS-0412").is_empty());
}

#[test]
fn rows_are_in_number_order_and_unique() {
    for w in ALL.windows(2) {
        assert!(
            w[0].number < w[1].number,
            "{} then {}",
            w[0].code,
            w[1].code
        );
    }
    let mut names: Vec<&str> = ALL.iter().map(|e| e.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), ALL.len(), "two rows share a name");
}

#[test]
fn every_code_is_spelled_from_its_number() {
    for e in ALL {
        assert_eq!(e.code, format!("NROS-{:04}", e.number));
        assert!(e.number >= 1);
    }
}

#[test]
fn every_live_row_is_complete_and_id_free() {
    for e in ALL.iter().filter(|e| e.is_live()) {
        assert!(!e.title.is_empty(), "{}", e.code);
        assert!(!e.remedy.is_empty(), "{}", e.code);
        assert!(!e.surfaces.is_empty(), "{} names no surface", e.code);
        assert!(!e.explanation.trim().is_empty(), "{}", e.code);
        assert_eq!(ids(e.title), Vec::<String>::new(), "{}", e.code);
        assert_eq!(ids(e.remedy), Vec::<String>::new(), "{}", e.code);
        // Ids live only under the closing `## Design background`.
        assert_eq!(
            ids(views::user_explanation(e)),
            Vec::<String>::new(),
            "{}",
            e.code
        );
        assert_eq!(
            ids(&views::explain_text(e)),
            Vec::<String>::new(),
            "{}",
            e.code
        );
    }
}

#[test]
fn every_variant_has_its_row() {
    for e in ALL.iter().filter(|e| e.is_live()) {
        assert!(by_number(e.number).is_some());
    }
    assert_eq!(Diag::AmentPrefixUnset.entry().name, "AmentPrefixUnset");
    assert_eq!(Diag::AmentPrefixUnset.code(), "NROS-0001");
    assert_eq!(Diag::PlatformNoFamily.number(), 5);
}

#[test]
fn a_retired_row_points_at_a_live_successor() {
    for e in ALL.iter().filter(|e| !e.is_live()) {
        let s = e
            .successor()
            .unwrap_or_else(|| panic!("{}: no successor row", e.code));
        assert!(
            s.is_live(),
            "{} retires into {}, itself retired",
            e.code,
            s.code
        );
    }
}

#[test]
fn a_code_is_ns_ros_and_exactly_four_digits() {
    assert_eq!(parse_code("NROS-0412"), Some(412));
    assert_eq!(parse_code("NROS-0001"), Some(1));
    for not in [
        "NROS-412",
        "NROS-04120",
        "nros-0412",
        "./NROS-0412",
        "NROS-04a2",
        "NROS-",
    ] {
        assert_eq!(parse_code(not), None, "{not}");
    }
    assert_eq!(
        lookup("NROS-0001").map(|e| e.name),
        Some("AmentPrefixUnset")
    );
    assert!(lookup("NROS-9999").is_none());
}

#[test]
fn nearest_orders_by_distance_then_number() {
    let near: Vec<&str> = nearest(4).iter().flatten().map(|e| e.code).collect();
    assert_eq!(near, ["NROS-0004", "NROS-0003", "NROS-0005"]);
    let near: Vec<&str> = nearest(9999).iter().flatten().map(|e| e.code).collect();
    assert_eq!(near.len(), 3);
    assert_eq!(near[0], ALL.last().unwrap().code);
}

// --- rendering (RFC-0129 D2) ---------------------------------------------------

#[test]
fn a_refusal_renders_detail_remedy_and_explain_line() {
    let board = "acme";
    let text = format!(
        "{}",
        Diag::BoardUnknown.refuse(&format_args!(
            "board `{board}` is claimed by no board descriptor"
        ))
    );
    assert_eq!(
        text,
        "error[NROS-0002]: board `acme` is claimed by no board descriptor\n  \
         help: check the spelling of the image's `board`; `nros board list` names every board \
         this SDK has\n  \
         explain: nros explain NROS-0002"
    );
    assert_eq!(ids(&text), Vec::<String>::new());
}

#[test]
fn a_site_may_pass_a_more_specific_remedy() {
    let text = format!(
        "{}",
        Diag::PlatformNoFamily
            .refuse(&"platform `acmeos` reaches no family")
            .help(&format_args!(
                "add `inherits = \"<family>\"` to the nros-platform.toml of `{}`",
                "acme-base"
            ))
    );
    assert_eq!(
        text,
        "error[NROS-0005]: platform `acmeos` reaches no family\n  \
         help: add `inherits = \"<family>\"` to the nros-platform.toml of `acme-base`\n  \
         explain: nros explain NROS-0005"
    );
}

#[test]
fn a_note_prints_only_when_verbose() {
    let r = Diag::ImageNoBoard
        .refuse(&"x: names no board")
        .note(&"see issue 1234");
    let quiet = format!("{r}");
    assert!(!quiet.contains("issue 1234"), "{quiet}");
    assert_eq!(ids(&quiet), Vec::<String>::new());
    let loud = format!("{}", r.verbose(true));
    assert_eq!(
        loud,
        "error[NROS-0004]: x: names no board\n  \
         help: add `board = \"<board>\"` to the `[image.<id>]` table in system.toml\n  \
         note: see issue 1234\n  \
         explain: nros explain NROS-0004"
    );
}

#[test]
fn the_build_script_constructor_panics_with_the_rendered_text() {
    let err = std::panic::catch_unwind(|| {
        Diag::ImageNoBoard.panic(format_args!("{}: names no board", "s.toml"))
    })
    .expect_err("panics");
    let msg = err
        .downcast_ref::<String>()
        .expect("a formatted panic message");
    assert_eq!(
        msg,
        "\nerror[NROS-0004]: s.toml: names no board\n  \
         help: add `board = \"<board>\"` to the `[image.<id>]` table in system.toml\n  \
         explain: nros explain NROS-0004"
    );
    assert_eq!(ids(msg), Vec::<String>::new());
}

// --- retirement, on a table of its own ---------------------------------------

mod retired_table {
    diagnostics! {
        0001 Kept {
            title: "kept",
            surfaces: [Cli, Cmake],
            remedy: "do the thing",
        }
        0002 Retired(Gone) => 0001,
    }

    #[test]
    fn a_retired_row_keeps_its_number_and_has_no_variant() {
        assert_eq!(ALL.len(), 2);
        assert_eq!(ALL[1].code, "NROS-0002");
        assert_eq!(ALL[1].name, "Gone");
        assert_eq!(ALL[1].retired, Some(1));
        assert!(ALL[1].explanation.is_empty());
        // `Diag::Gone` does not exist; the only variant is the live one.
        assert!(matches!(Diag::Kept, Diag::Kept));
        assert_eq!(Diag::Kept as u16, 1);
        assert_eq!(
            ALL[0].surfaces,
            &[crate::Surface::Cli, crate::Surface::Cmake]
        );
    }

    #[test]
    fn explain_names_a_retired_codes_successor() {
        assert_eq!(
            crate::views::explain_text_in(&ALL[1], ALL),
            "NROS-0002: retired.\n\nThis code is no longer raised. It was replaced by \
             NROS-0001: kept\nRun `nros explain NROS-0001` for its explanation.\n"
        );
    }
}

// --- views -------------------------------------------------------------------------

#[test]
fn the_lock_lists_every_row_and_round_trips() {
    let lock = views::lock();
    let rows = views::parse_lock(&lock).expect("the generated lock parses");
    assert_eq!(rows.len(), ALL.len());
    for (r, e) in rows.iter().zip(ALL) {
        assert_eq!(
            (r.code.as_str(), r.name.as_str(), r.live),
            (e.code, e.name, e.is_live())
        );
    }
    let committed = include_str!("../codes.lock");
    assert_eq!(
        committed, lock,
        "codes.lock is stale: run nros-diagnostics-gen"
    );
}

#[test]
fn explain_text_leaves_out_the_design_background() {
    let e: &Entry = Diag::AmentPrefixUnset.entry();
    assert!(e.explanation.contains(views::DESIGN_BACKGROUND));
    let t = views::explain_text(e);
    assert!(!t.contains(views::DESIGN_BACKGROUND), "{t}");
    assert!(t.starts_with("NROS-0001: ROS 2 is not sourced"), "{t}");
    assert!(t.contains("The refusal's help line: source ROS 2"), "{t}");
}

#[test]
fn surfaces_spell_as_the_macro_does() {
    assert_eq!(Surface::BuildScript.name(), "BuildScript");
}
