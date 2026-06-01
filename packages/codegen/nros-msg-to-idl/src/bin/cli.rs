//! `nros-msg-to-idl` — Cyclone-DDS-shaped IDL generator.
//!
//! Phase 212.K.3: drop-in replacement for
//! `scripts/cyclonedds/msg_to_cyclone_idl.py`'s `--interface` flow.
//! Reads a single `.msg` and writes the mangled IDL.

use std::{fs, path::PathBuf, process::ExitCode};

use clap::Parser;
use nros_msg_to_idl::Converter;

#[derive(Parser, Debug)]
#[command(
    name = "nros-msg-to-idl",
    version,
    about = "Convert a ROS 2 .msg file to Cyclone-DDS-shaped IDL.",
    long_about = "Pure-Rust port of scripts/cyclonedds/msg_to_cyclone_idl.py. \
                  Reads <INPUT> (.msg), writes the mangled IDL to --output or stdout."
)]
struct Cli {
    /// ROS package name (e.g. `std_msgs`).
    #[arg(long)]
    package: String,

    /// Message name (e.g. `Int32`). Used for the struct identifier
    /// inside the generated IDL.
    #[arg(long)]
    message: String,

    /// Inject the 16-byte `cdds_request_header_t` fields into every
    /// rewritten struct. Set this when running on a `.srv`-flavoured
    /// `.msg` half.
    #[arg(long, default_value_t = false)]
    service_header: bool,

    /// Output path. If omitted, writes to stdout.
    #[arg(short, long)]
    output: Option<PathBuf>,

    /// Path to the `.msg` file.
    input: PathBuf,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let src = match fs::read_to_string(&cli.input) {
        Ok(s) => s,
        Err(err) => {
            eprintln!("error: reading {}: {err}", cli.input.display());
            return ExitCode::FAILURE;
        }
    };

    let idl = match Converter::new(&cli.package, &cli.message)
        .with_service_header(cli.service_header)
        .convert(&src)
    {
        Ok(s) => s,
        Err(err) => {
            eprintln!("error: converting {}: {err}", cli.input.display());
            return ExitCode::FAILURE;
        }
    };

    match cli.output {
        Some(p) => {
            if let Err(err) = fs::write(&p, idl) {
                eprintln!("error: writing {}: {err}", p.display());
                return ExitCode::FAILURE;
            }
        }
        None => {
            print!("{idl}");
        }
    }
    ExitCode::SUCCESS
}
