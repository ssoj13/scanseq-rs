//! Core sequence detection engine.
//!
//! This module provides fast file sequence scanning for VFX/animation pipelines.
//!
//! # Architecture
//! - `file`: Parses paths, extracts digit groups, creates masks for grouping
//! - `seq`: Groups files into sequences using mask-based hashing
//! - `scan`: Parallel directory traversal and orchestration
//!
//! # Algorithm
//! Files are grouped by mask (e.g., `render_@_frame_@.exr`), then sub-grouped
//! by anchor values (non-frame digit groups). This handles:
//! - Padded sequences: `img_0001.exr` - `img_0100.exr`
//! - Unpadded sequences: `img_1.exr` - `img_100.exr`
//! - Multi-group names: `shot_01_frame_0001.exr` (anchor=01, frame=0001)

mod file;
mod seq;
mod scan;

pub use seq::Seq;
pub use scan::get_seqs;

use rayon::prelude::*;
use std::path::Path;
use std::time::Instant;

/// Result of a scan operation
#[derive(Debug, Clone, Default)]
#[allow(dead_code)] // Public API for library users
pub struct ScanResult {
    /// Detected sequences
    pub seqs: Vec<Seq>,
    /// Scan duration in milliseconds
    pub elapsed_ms: f64,
    /// Errors encountered during scan
    pub errors: Vec<String>,
}

/// Stateful scanner with configuration and results.
///
/// # Example
/// ```ignore
/// use scanseq::core::Scanner;
///
/// let scanner = Scanner::new(
///     vec!["/renders".into()],
///     true,           // recursive
///     Some("*.exr"),  // mask
///     2               // min_len
/// );
///
/// println!("Found {} sequences in {:.1}ms", scanner.result.seqs.len(), scanner.result.elapsed_ms);
/// for seq in &scanner.result.seqs {
///     println!("{}", seq);
/// }
/// ```
#[derive(Debug, Clone)]
#[allow(dead_code)] // Public API for library users
pub struct Scanner {
    /// Root paths to scan
    pub roots: Vec<String>,
    /// Recursive scanning enabled
    pub recursive: bool,
    /// File mask filter (glob pattern)
    pub mask: Option<String>,
    /// Minimum sequence length
    pub min_len: usize,
    /// Scan results
    pub result: ScanResult,
}

impl Scanner {
    /// Create scanner and run initial scan.
    ///
    /// # Arguments
    /// * `roots` - List of directory paths to scan
    /// * `recursive` - Scan subdirectories
    /// * `mask` - File mask/glob pattern (e.g., "*.exr")
    /// * `min_len` - Minimum sequence length
    #[allow(dead_code)] // Public library API
    pub fn new<S: Into<String>>(
        roots: Vec<S>,
        recursive: bool,
        mask: Option<&str>,
        min_len: usize,
    ) -> Self {
        let roots: Vec<String> = roots.into_iter().map(|s| s.into()).collect();
        let mask = mask.map(|s| s.to_string());

        let mut scanner = Scanner {
            roots,
            recursive,
            mask,
            min_len,
            result: ScanResult::default(),
        };
        scanner.rescan();
        scanner
    }

    /// Scan a single path (static method).
    ///
    /// # Arguments
    /// * `root` - Directory path to scan
    /// * `recursive` - Scan subdirectories
    /// * `mask` - File mask/glob pattern
    /// * `min_len` - Minimum sequence length
    ///
    /// # Returns
    /// `ScanResult` with sequences, timing, and errors
    #[allow(dead_code)] // Public library API
    pub fn get_seq<P: AsRef<Path>>(
        root: P,
        recursive: bool,
        mask: Option<&str>,
        min_len: usize,
    ) -> ScanResult {
        let start = Instant::now();
        let mut result = ScanResult::default();

        match get_seqs(root.as_ref(), recursive, mask, min_len) {
            Ok(seqs) => result.seqs = seqs,
            Err(e) => result.errors.push(e),
        }

        result.elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        result
    }

    /// Scan multiple paths in parallel (static method).
    ///
    /// # Arguments
    /// * `roots` - Directory paths to scan
    /// * `recursive` - Scan subdirectories
    /// * `mask` - File mask/glob pattern
    /// * `min_len` - Minimum sequence length
    ///
    /// # Returns
    /// `ScanResult` with sequences, timing, and errors
    pub fn get_seqs<P: AsRef<Path> + Sync>(
        roots: &[P],
        recursive: bool,
        mask: Option<&str>,
        min_len: usize,
    ) -> ScanResult {
        let start = Instant::now();

        // Scan roots in parallel
        let results: Vec<_> = roots.par_iter().map(|root| {
            match get_seqs(root.as_ref(), recursive, mask, min_len) {
                Ok(seqs) => (seqs, None),
                Err(e) => (Vec::new(), Some(format!("{}: {}", root.as_ref().display(), e))),
            }
        }).collect();

        let mut result = ScanResult::default();
        for (seqs, err) in results {
            result.seqs.extend(seqs);
            if let Some(e) = err {
                result.errors.push(e);
            }
        }

        result.elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;
        result
    }

    /// Re-scan all roots in parallel with current settings.
    /// Updates `result` with new sequences, timing, and errors.
    #[allow(dead_code)] // Public library API
    pub fn rescan(&mut self) {
        let start = Instant::now();

        // Scan roots in parallel
        let results: Vec<_> = self.roots.par_iter().map(|root| {
            match get_seqs(root, self.recursive, self.mask.as_deref(), self.min_len) {
                Ok(seqs) => (seqs, None),
                Err(e) => (Vec::new(), Some(format!("{}: {}", root, e))),
            }
        }).collect();

        let mut all_seqs = Vec::new();
        let mut errors = Vec::new();
        for (seqs, err) in results {
            all_seqs.extend(seqs);
            if let Some(e) = err {
                errors.push(e);
            }
        }

        self.result = ScanResult {
            seqs: all_seqs,
            errors,
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
        };
    }

    /// Number of sequences found
    #[must_use]
    #[allow(dead_code)] // Public library API
    pub fn len(&self) -> usize {
        self.result.seqs.len()
    }

    /// Check if no sequences found
    #[must_use]
    #[allow(dead_code)] // Public library API
    pub fn is_empty(&self) -> bool {
        self.result.seqs.is_empty()
    }

    /// Iterate over sequences
    #[allow(dead_code)] // Public library API
    pub fn iter(&self) -> impl Iterator<Item = &Seq> {
        self.result.seqs.iter()
    }
}

impl std::fmt::Display for Scanner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Scanner(roots={}, seqs={}, elapsed={:.2}ms)",
            self.roots.len(),
            self.result.seqs.len(),
            self.result.elapsed_ms
        )
    }
}
