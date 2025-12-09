//! # ScanSeq CLI - Command Line Interface
//!
//! Fast file sequence scanner for VFX, animation, and media production pipelines.
//!
//! # Usage
//!
//! ```bash
//! # Show help
//! scanseq-cli
//!
//! # Scan paths and print results
//! scanseq-cli -p /renders -p /projects -o
//!
//! # Recursive scan
//! scanseq-cli -p /renders -r -o
//!
//! # Filter by mask
//! scanseq-cli -p /renders -m "*.exr" -o
//!
//! # JSON output
//! scanseq-cli -p /renders -oj
//! ```
//!
//! # Architecture
//!
//! This binary is the CLI entry point. It:
//! 1. Parses arguments via [`clap`] - supports multiple paths via `-p/--path`
//! 2. Calls [`core::get_seqs()`] for each path to scan and group files
//! 3. Outputs results as human-readable text or JSON
//!
//! # Dependencies
//!
//! - [`core`]: Sequence detection engine (file parsing, grouping, scanning)
//! - [`clap`]: Command-line argument parsing
//! - [`serde_json`]: JSON serialization for `--json` output
//! - [`log`]/[`env_logger`]: Logging infrastructure
//!
//! # See Also
//!
//! - `core`: Core sequence detection algorithm
//! - Python bindings available when built with `--features python`

mod core;

use clap::Parser;
use core::{format_frame, scan_files, Scanner, Seq};
use std::path::PathBuf;

use log::{debug, info};

#[derive(Parser)]
#[command(name = "scanseq-cli")]
#[command(about = "Fast file sequence scanner for VFX/animation pipelines", long_about = None)]
struct Args {
    /// Paths to scan (can specify multiple: -p /path1 -p /path2)
    #[arg(short = 'p', long = "path")]
    paths: Vec<PathBuf>,

    /// Scan subdirectories recursively
    #[arg(short = 'r', long = "recursive")]
    recursive: bool,

    /// File mask/pattern (e.g., "*.exr") for sequence detection
    #[arg(short, long)]
    mask: Option<String>,

    /// Scan files by extensions (e.g., -s exr mp4 mov). Supports glob: jp* tif?
    #[arg(short = 's', long = "scan-files", num_args = 1..)]
    scan_exts: Option<Vec<String>>,

    /// Minimum sequence length
    #[arg(short = 'n', long = "min", default_value = "2")]
    min_len: usize,

    /// Print sequences to stdout (default: off)
    #[arg(short = 'o', long = "out")]
    out: bool,

    /// Use JSON format (with -o)
    #[arg(short = 'j', long)]
    json: bool,
}

fn main() {
    // Initialize logger - respect RUST_LOG, default to Info
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let args = Args::parse();

    // Show help if no paths specified
    if args.paths.is_empty() {
        use clap::CommandFactory;
        if let Err(e) = Args::command().print_help() {
            eprintln!("Failed to print help: {}", e);
        }
        println!();
        return;
    }

    info!("ScanSeq - Fast file sequence scanner");

    // Log paths being scanned
    for path in &args.paths {
        debug!("Scanning: {}", path.display());
    }

    // Mode: scan files by extension OR detect sequences
    if let Some(exts) = &args.scan_exts {
        // File scanning mode
        let ext_refs: Vec<&str> = exts.iter().map(|s| s.as_str()).collect();
        match scan_files(&args.paths, args.recursive, &ext_refs) {
            Ok(files) => {
                if args.out {
                    if args.json {
                        #[derive(serde::Serialize)]
                        struct FilesOutput {
                            files: Vec<String>,
                            total: usize,
                        }
                        let output = FilesOutput {
                            total: files.len(),
                            files: files.iter().map(|p| p.display().to_string()).collect(),
                        };
                        match serde_json::to_string_pretty(&output) {
                            Ok(json) => println!("{}", json),
                            Err(e) => {
                                eprintln!("JSON error: {}", e);
                                std::process::exit(1);
                            }
                        }
                    } else {
                        for f in &files {
                            println!("{}", f.display());
                        }
                        eprintln!("\nTotal: {} files", files.len());
                    }
                }
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                std::process::exit(1);
            }
        }
        return;
    }

    // Sequence detection mode
    let result = Scanner::get_seqs(&args.paths, args.recursive, args.mask.as_deref(), args.min_len);

    // Report errors
    for err in &result.errors {
        eprintln!("Error: {}", err);
    }

    // Sort sequences by pattern
    let mut all_seqs: Vec<Seq> = result.seqs;
    all_seqs.sort_by(|a, b| a.pattern().cmp(b.pattern()));

    let total_files: usize = all_seqs.iter().map(|s| s.len()).sum();

    let has_errors = !result.errors.is_empty();

    // Output only if --out is specified
    if args.out {
        if args.json {
            // JSON output
            #[derive(serde::Serialize)]
            struct Output {
                sequences: Vec<Seq>,
                total_sequences: usize,
                total_files: usize,
                errors: Vec<String>,
            }

            let output = Output {
                total_sequences: all_seqs.len(),
                sequences: all_seqs,
                total_files,
                errors: result.errors.clone(),
            };

            match serde_json::to_string_pretty(&output) {
                Ok(json) => println!("{}", json),
                Err(e) => {
                    eprintln!("JSON error: {}", e);
                    std::process::exit(1);
                }
            }
        } else {
            // Human-readable output
            if all_seqs.is_empty() {
                println!("No sequences found.");
            } else {
                println!("Sequences:");
                for seq in &all_seqs {
                    let pattern = seq.pattern();
                    let first_file = format_frame(pattern, seq.padding, seq.start);
                    if seq.missed.is_empty() {
                        println!("  {} [{}-{}] ({} files)", pattern, seq.start, seq.end, seq.len());
                    } else {
                        println!("  {} [{}-{}] ({} files, {} missed)", pattern, seq.start, seq.end, seq.len(), seq.missed.len());
                    }
                    debug!("    First: {}", first_file);
                }

                println!("\nSummary: {} sequences, {} files", all_seqs.len(), total_files);
            }
        }
    }

    // Exit with error if any path failed
    if has_errors {
        std::process::exit(1);
    }
}
