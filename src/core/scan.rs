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

/// Scan folder(s) for files matching extensions.
/// Uses jwalk for parallel recursive scanning.
///
/// # Arguments
/// * `roots` - Directory or directories to scan
/// * `recursive` - Scan subdirectories
/// * `exts` - File extensions or glob patterns (e.g., &["mp4", "jp*", "tif?"])
///
/// # Example
/// ```ignore
/// let videos = scan_files(&["/media"], true, &["mp4", "mov", "avi"])?;
/// let images = scan_files(&["/renders"], false, &["exr", "jp*"])?; // jpg, jpeg, jp2...
/// let all = scan_files(&["/data"], true, &[])?; // all files
/// ```
pub fn scan_files<P: AsRef<Path> + Sync>(roots: &[P], recursive: bool, exts: &[&str]) -> Result<Vec<PathBuf>, String> {
    // Pre-compile glob patterns (only for entries with wildcards)
    let patterns: Vec<Option<glob::Pattern>> = exts
        .iter()
        .map(|e| {
            if e.contains('*') || e.contains('?') {
                glob::Pattern::new(&e.to_lowercase()).ok()
            } else {
                None
            }
        })
        .collect();

    let files: Vec<PathBuf> = roots
        .par_iter()
        .flat_map(|root| {
            let walker = if recursive {
                WalkDir::new(root.as_ref()).follow_links(false)
            } else {
                WalkDir::new(root.as_ref()).max_depth(1).follow_links(false)
            };

            walker
                .into_iter()
                .filter_map(|e| match e {
                    Ok(entry) => Some(entry),
                    Err(err) => {
                        warn!("Skipping inaccessible path: {}", err);
                        None
                    }
                })
                .filter(|e| e.file_type().is_file())
                .filter_map(|e| {
                    let path = e.path();

                    // Filter by extension if provided
                    if !exts.is_empty() {
                        let ext = path.extension()?.to_str()?.to_lowercase();
                        let matched = exts.iter().zip(patterns.iter()).any(|(e, pat)| {
                            match pat {
                                Some(p) => p.matches(&ext),  // glob match
                                None => e.eq_ignore_ascii_case(&ext),  // exact match
                            }
                        });
                        if !matched {
                            return None;
                        }
                    }

                    Some(path.to_path_buf())
                })
                .collect::<Vec<_>>()
        })
        .collect();

    Ok(files)
}

/// Scan single folder for files with glob mask (internal, used by get_seqs)
fn scan_files_glob<P: AsRef<Path>>(folder: P, mask: Option<&str>) -> Result<Vec<PathBuf>, String> {
    let folder = folder.as_ref();
    let entries = std::fs::read_dir(folder)
        .map_err(|e| format!("Failed to read dir {}: {}", folder.display(), e))?;

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
        if let Some(ref pattern) = glob_pattern {
            if let Some(name) = path.file_name() {
                if !pattern.matches(&name.to_string_lossy()) {
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
                let files = match scan_files_glob(folder, mask) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_scan_files_flat() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        // Create test files
        fs::write(root.join("video.mp4"), "").unwrap();
        fs::write(root.join("video.mov"), "").unwrap();
        fs::write(root.join("image.exr"), "").unwrap();

        // Scan all (empty extensions = all files)
        let all = scan_files(&[root], false, &[]).unwrap();
        assert_eq!(all.len(), 3);

        // Scan specific extension
        let videos = scan_files(&[root], false, &["mp4"]).unwrap();
        assert_eq!(videos.len(), 1);
        assert!(videos[0].to_string_lossy().contains("video.mp4"));
    }

    #[test]
    fn test_scan_files_recursive() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let sub = root.join("subdir");
        fs::create_dir(&sub).unwrap();

        // Files in root and subdir
        fs::write(root.join("a.exr"), "").unwrap();
        fs::write(sub.join("b.exr"), "").unwrap();
        fs::write(sub.join("c.mp4"), "").unwrap();

        // Non-recursive - only root
        let flat = scan_files(&[root], false, &["exr"]).unwrap();
        assert_eq!(flat.len(), 1);

        // Recursive - both
        let deep = scan_files(&[root], true, &["exr"]).unwrap();
        assert_eq!(deep.len(), 2);
    }

    #[test]
    fn test_scan_files_multi_ext() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::write(root.join("a.mp4"), "").unwrap();
        fs::write(root.join("b.mov"), "").unwrap();
        fs::write(root.join("c.avi"), "").unwrap();
        fs::write(root.join("d.exr"), "").unwrap();

        // Multiple extensions
        let videos = scan_files(&[root], false, &["mp4", "mov", "avi"]).unwrap();
        assert_eq!(videos.len(), 3);
    }

    #[test]
    fn test_scan_files_multiple_roots() {
        let dir1 = tempdir().unwrap();
        let dir2 = tempdir().unwrap();

        fs::write(dir1.path().join("a.exr"), "").unwrap();
        fs::write(dir2.path().join("b.exr"), "").unwrap();

        let files = scan_files(&[dir1.path(), dir2.path()], false, &["exr"]).unwrap();
        assert_eq!(files.len(), 2);
    }

    #[test]
    fn test_scan_files_glob_patterns() {
        let dir = tempdir().unwrap();
        let root = dir.path();

        fs::write(root.join("a.jpg"), "").unwrap();
        fs::write(root.join("b.jpeg"), "").unwrap();
        fs::write(root.join("c.jp2"), "").unwrap();
        fs::write(root.join("d.tif"), "").unwrap();
        fs::write(root.join("e.tiff"), "").unwrap();
        fs::write(root.join("f.png"), "").unwrap();

        // jp* matches jpg, jpeg, jp2
        let jp = scan_files(&[root], false, &["jp*"]).unwrap();
        assert_eq!(jp.len(), 3);

        // tif? matches tiff but not tif (single char)
        let tif = scan_files(&[root], false, &["tif?"]).unwrap();
        assert_eq!(tif.len(), 1);

        // Mix exact and glob
        let mixed = scan_files(&[root], false, &["png", "jp*"]).unwrap();
        assert_eq!(mixed.len(), 4);
    }
}
