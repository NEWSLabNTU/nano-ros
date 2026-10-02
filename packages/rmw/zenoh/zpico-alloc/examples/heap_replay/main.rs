//! Issue 1370 — how small an arena still serves a recorded image's traffic,
//! and how large one a stress pattern in the same size range needs.
//!
//! ```text
//! cargo run -p zpico-alloc --features stats --example heap_replay -- <trace>...
//! ```
//!
//! `<trace>` is what `scripts/heap-trace/gdb_heap_trace.py` records off a
//! running image. For each one this prints the traffic's shape (request range,
//! peak requested bytes), the arena's own counters from a replay, the SMALLEST
//! arena from which every larger candidate also serves the whole trace, the
//! verdict of the refusal just below it, and the arena the Robson-style
//! adversary needs for the same size range at the same live budget.

#[cfg(feature = "stats")]
mod engine;

#[cfg(not(feature = "stats"))]
fn main() {
    eprintln!("heap_replay needs the arena's counters: re-run with `--features stats`");
    std::process::exit(2);
}

#[cfg(feature = "stats")]
fn main() {
    use engine::*;

    let paths: Vec<String> = std::env::args().skip(1).collect();
    if paths.is_empty() {
        eprintln!(
            "usage: heap_replay <trace>...   (see scripts/heap-trace/gdb_heap_trace.py)\n       \
             heap_replay --adversary <min> <max> <live-budget>"
        );
        std::process::exit(2);
    }
    if paths[0] == "--adversary" {
        let n: Vec<usize> = paths[1..].iter().filter_map(|a| a.parse().ok()).collect();
        let [lo, hi, budget] = n[..] else {
            eprintln!("--adversary takes three byte counts: <min> <max> <live-budget>");
            std::process::exit(2);
        };
        match smallest_safe(|c| (c.2)(lo, hi, budget)) {
            Some(adv) => println!(
                "adversary {lo}..{hi} B at {budget} B live needs {} bytes ({:.2}x the live budget)",
                adv.arena,
                adv.arena as f64 / budget as f64
            ),
            None => println!(
                "adversary {lo}..{hi} B at {budget} B live is refused even at the largest candidate"
            ),
        }
        return;
    }
    let mut failed = false;
    for path in &paths {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{path}: {e}");
                failed = true;
                continue;
            }
        };
        let trace = match parse(&text) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("{path}: {e}");
                failed = true;
                continue;
            }
        };
        let s = shape(&trace);
        println!("== {path}");
        println!(
            "   traffic   {} requests, {}..{} bytes, peak requested live {} bytes",
            s.requests, s.min_request, s.max_request, s.peak_requested
        );
        if trace.refused_in_image + trace.unmatched_frees > 0 {
            println!(
                "   (skipped: {} refused in the image, {} frees of untraced blocks)",
                trace.refused_in_image, trace.unmatched_frees
            );
        }
        let Some(safe) = smallest_safe(|c| (c.1)(&trace)) else {
            println!("   replay    refused even at the largest candidate arena");
            failed = true;
            continue;
        };
        let (lo, hi) = safe.spread;
        println!(
            "   replay    smallest safe arena {} bytes; arena peak {} bytes; TLSF requests {}..{} (ratio {:.1})",
            safe.arena,
            safe.peak,
            lo,
            hi,
            if lo > 0 { hi as f64 / lo as f64 } else { 0.0 }
        );
        println!(
            "             safe arena / arena peak = {:.2}",
            safe.arena as f64 / safe.peak.max(1) as f64
        );
        if let Some(below) = largest_refusing(|c| (c.1)(&trace)) {
            let r = below.refusal.expect("largest_refusing returns a refusal");
            println!(
                "   below it  {} bytes refuses {} B at event {} as {}: free {} B, largest hole {} B, {} holes",
                below.arena,
                r.size,
                r.at,
                r.verdict.as_str(),
                r.shape.free_total,
                r.shape.largest_free,
                r.shape.free_blocks
            );
        }
        if lo > 0 {
            let budget = safe.peak;
            match smallest_safe(|c| (c.2)(lo, hi, budget)) {
                Some(adv) => println!(
                    "   adversary {}..{} B at {} B live needs {} bytes ({:.2}x the live budget)",
                    lo,
                    hi,
                    budget,
                    adv.arena,
                    adv.arena as f64 / budget as f64
                ),
                None => println!(
                    "   adversary {lo}..{hi} B at {budget} B live is refused even at the largest candidate"
                ),
            }
        }
    }
    if failed {
        std::process::exit(1);
    }
}
