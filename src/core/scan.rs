//! Directory scanning and parallel sequence detection.
//!
//! This module implements the two-phase scanning algorithm:
//! 1. **Phase 1**: Discover all subdirectories using jwalk (fast parallel walker)
//! 2. **Phase 2**: Process folders in parallel using rayon thread pool
//!
//! Each worker:
//! - Scans files in one folder
//! - Converts paths to [`File`] objects (extracts digit groups, creates masks)
//! - Groups files into [`Seq`] sequences via mask-based hashing
//!
//! The mask-based approach handles unpadded sequences correctly:
//! `img_1.exr` through `img_100.exr` all have mask `img_@` and group together.

use super::file::File;
use super::seq::Seq;
use indicatif::{ProgressBar, ProgressStyle};
use jwalk::WalkDir;
use log::{debug, info, warn};
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

/// Scan directory for all subdirectories
///
/// Returns unique sorted list of folder paths
pub fn scan_dirs<P: AsRef<Path>>(root: P, recursive: bool) -> Result<Vec<PathBuf>, String> {
    let root = root.as_ref();
    info!("Scanning folders in: {}", root.display());
    let walker = if recursive { WalkDir::new(root) } else { WalkDir::new(root).max_depth(1) };
    let mut folders = Vec::new();
    for entry in walker {
        match entry {
            Ok(e) if e.file_type().is_dir() => {
                let path = e.path();
                if path != root {
                    folders.push(path.to_path_buf());
                }
            }
            Ok(_) => {} // Not a directory
            Err(e) => warn!("Skipping inaccessible path: {}", e),
        }
    }
    // Always include root itself
    folders.push(root.to_path_buf());
    // Sort and deduplicate
    folders.sort();
    folders.dedup();
    info!("Found {} folders", folders.len());
    Ok(folders)
}

/// Scan single folder for files matching mask
///
/// Returns file list (non-recursive, just this folder)
pub fn scan_files<P: AsRef<Path>>(folder: P, mask: Option<&str>) -> Result<Vec<PathBuf>, String> {
    let folder = folder.as_ref();
    let entries = std::fs::read_dir(folder).map_err(|e| format!("Failed to read dir {}: {}", folder.display(), e))?;

    // Pre-compile glob pattern once (if mask contains wildcards)
    let glob_pattern = match mask {
        Some(m) if m.contains('*') => Some(glob::Pattern::new(m).map_err(|e| format!("Invalid mask: {}", e))?),
        _ => None,
    };

    let mut files = Vec::new();
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        // Apply mask if provided
        if let Some(mask) = mask {
            if let Some(name) = path.file_name() {
                let name_str = name.to_string_lossy();
                if let Some(ref pattern) = glob_pattern {
                    if !pattern.matches(&name_str) {
                        continue;
                    }
                } else if name_str != mask {
                    continue;
                }
            }
        }
        files.push(path);
    }
    Ok(files)
}

/// Main scan and group function
///
/// Returns all sequences found (flattened, not per-folder)
pub fn get_seqs<P: AsRef<Path>>(root: P, recursive: bool, mask: Option<&str>, min_len: usize) -> Result<Vec<Seq>, String> {
    let start = std::time::Instant::now();
    // Phase 1: Discover folders
    info!("Phase 1: Discovering folders...");
    let folders = scan_dirs(root, recursive)?;
    info!("Phase 1 complete: {} folders in {:.2}s", folders.len(), start.elapsed().as_secs_f64());

    // Phase 2: Process folders in parallel
    info!("Phase 2: Processing folders in parallel...");
    let phase2_start = std::time::Instant::now();

    // Use dynamic thread count based on available cores
    let num_threads = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8);
    info!("Using {} threads for parallel processing", num_threads);

    let pool = ThreadPoolBuilder::new()
        .num_threads(num_threads)
        .build()
        .map_err(|e| format!("Failed to create thread pool: {}", e))?;

    let found_seqs = AtomicUsize::new(0);

    // Progress bar
    let pb = Arc::new(ProgressBar::new(folders.len() as u64));
    pb.set_style(
        ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} folders ({msg})")
            .expect("Invalid progress bar template")
            .progress_chars("=>-"),
    );

    let all_seqs: Vec<Seq> = pool.install(|| {
        folders
            .par_iter()
            .flat_map(|folder| {
                // Scan files in this folder
                let files = match scan_files(folder, mask) {
                    Ok(f) => f,
                    Err(e) => {
                        warn!("Error scanning {}: {}", folder.display(), e);
                        pb.inc(1);
                        return Vec::new();
                    }
                };

                if files.is_empty() {
                    pb.inc(1);
                    return Vec::new();
                }

                debug!("Processing {} ({} files)", folder.display(), files.len());

                // Convert to File objects (move PathBuf instead of clone)
                let mut file_objs: Vec<File> = files.into_iter().map(File::new).collect();

                // Group into sequences
                let seqs = Seq::group_seqs(&mut file_objs);

                // Filter by min_len
                let filtered: Vec<Seq> = seqs.into_iter().filter(|s| s.len() >= min_len).collect();

                if !filtered.is_empty() {
                    let seq_count = filtered.len();
                    // Use fetch_add return value to avoid race condition in message
                    let prev = found_seqs.fetch_add(seq_count, std::sync::atomic::Ordering::Relaxed);
                    debug!("Found {} seqs in {}", seq_count, folder.display());
                    pb.set_message(format!("{} seqs found", prev + seq_count));
                }

                // Update progress bar
                pb.inc(1);

                filtered
            })
            .collect()
    });

    pb.finish_with_message("Complete");

    let total_seqs = all_seqs.len();
    info!("Phase 2 complete: {} sequences in {:.2}s", total_seqs, phase2_start.elapsed().as_secs_f64());
    info!("Total time: {:.2}s", start.elapsed().as_secs_f64());

    Ok(all_seqs)
}
