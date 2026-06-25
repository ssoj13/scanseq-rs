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

pub use seq::{Seq, format_frame};
pub use scan::{get_seqs, scan_files};

use file::File;

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

/// Common VFX image extensions for convenience
pub const VFX_IMAGE_EXTS: &[&str] = &["exr", "dpx", "tif", "tiff", "png", "jpg", "jpeg", "tga", "hdr"];
/// Common video extensions
#[allow(dead_code)] // Public API
pub const VIDEO_EXTS: &[&str] = &["mp4", "mov", "avi", "mkv", "webm", "m4v", "mxf"];

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

    // === Builder pattern for flexible configuration ===

    /// Create scanner builder for single root path.
    /// Call `.scan()` to execute.
    ///
    /// # Example
    /// ```ignore
    /// let scanner = Scanner::path("/renders")
    ///     .recursive(true)
    ///     .extensions(&["exr", "png"])
    ///     .min_len(2)
    ///     .scan();
    /// ```
    #[allow(dead_code)]
    pub fn path<P: AsRef<Path>>(root: P) -> ScannerBuilder {
        ScannerBuilder {
            roots: vec![root.as_ref().to_string_lossy().to_string()],
            recursive: true,
            mask: None,
            min_len: 2,
        }
    }

    /// Create scanner builder for multiple root paths.
    /// Call `.scan()` to execute.
    #[allow(dead_code)]
    pub fn paths<P: AsRef<Path>>(roots: &[P]) -> ScannerBuilder {
        ScannerBuilder {
            roots: roots.iter().map(|p| p.as_ref().to_string_lossy().to_string()).collect(),
            recursive: true,
            mask: None,
            min_len: 2,
        }
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

    /// Find sequence containing the given file.
    /// Scans parent directory (non-recursive) to find matching files.
    ///
    /// # Arguments
    /// * `path` - Path to a file that may be part of a sequence
    ///
    /// # Returns
    /// `Some(Seq)` if file is part of a sequence, `None` otherwise
    #[allow(dead_code)] // Public library API
    pub fn from_file<P: AsRef<Path>>(path: P) -> Option<Seq> {
        let path = path.as_ref();
        let dir = path.parent()?;

        // Read directory (non-recursive, no mask filter)
        let entries: Vec<std::path::PathBuf> = std::fs::read_dir(dir)
            .ok()?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();

        let target = File::new(path);
        if !target.has_nums() {
            return None;
        }

        let mut files: Vec<File> = entries.into_iter()
            .map(File::new)
            .filter(|f| f.has_nums())
            .collect();

        Seq::extract_seq(&target, &mut files)
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

// === Scanner Builder Pattern ===

/// Builder for configuring Scanner with fluent API.
/// Created via `Scanner::path()` or `Scanner::paths()`.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ScannerBuilder {
    roots: Vec<String>,
    recursive: bool,
    mask: Option<String>,
    min_len: usize,
}

impl ScannerBuilder {
    /// Set recursive scanning (default: true)
    #[allow(dead_code)]
    pub fn recursive(mut self, recursive: bool) -> Self {
        self.recursive = recursive;
        self
    }

    /// Set file mask/glob pattern (e.g., "*.exr")
    #[allow(dead_code)]
    pub fn mask(mut self, mask: &str) -> Self {
        self.mask = Some(mask.to_string());
        self
    }

    /// Set extensions to scan (convenience for common patterns).
    /// Converts `&["exr", "png"]` to mask `"*.exr;*.png"`.
    ///
    /// # Example
    /// ```ignore
    /// Scanner::path("/renders")
    ///     .extensions(&["exr", "png", "jpg"])
    ///     .scan();
    /// ```
    #[allow(dead_code)]
    pub fn extensions(mut self, exts: &[&str]) -> Self {
        if exts.is_empty() {
            self.mask = None;
        } else {
            // Build glob pattern: "*.exr" or "*.{exr,png,jpg}"
            let pattern = if exts.len() == 1 {
                format!("*.{}", exts[0])
            } else {
                format!("*.{{{}}}", exts.join(","))
            };
            self.mask = Some(pattern);
        }
        self
    }

    /// Use predefined VFX image extensions (exr, dpx, tif, png, jpg, tga, hdr).
    #[allow(dead_code)]
    pub fn vfx_images(self) -> Self {
        self.extensions(VFX_IMAGE_EXTS)
    }

    /// Set minimum sequence length (default: 2)
    #[allow(dead_code)]
    pub fn min_len(mut self, min_len: usize) -> Self {
        self.min_len = min_len;
        self
    }

    /// Execute scan and return configured Scanner with results.
    #[allow(dead_code)]
    pub fn scan(self) -> Scanner {
        Scanner::new(self.roots, self.recursive, self.mask.as_deref(), self.min_len)
    }

    /// Execute scan and return only the sequences (convenience).
    #[allow(dead_code)]
    pub fn into_seqs(self) -> Vec<Seq> {
        self.scan().result.seqs
    }
}

// === Unified detection entry point (file OR directory) ===

/// Error from [`detect`]. Intentionally minimal; a full typed error set is a later pass.
///
/// Lives here so callers can `use scanseq::core::DetectError` alongside [`detect`].
#[derive(Debug)]
pub enum DetectError {
    /// Directory holds more than one sequence — caller must disambiguate (e.g. pass a file).
    Ambiguous { count: usize },
    /// Filesystem / scan error.
    Scan(String),
}

impl std::fmt::Display for DetectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DetectError::Ambiguous { count } => write!(f, "directory holds {count} sequences; pass a file to disambiguate"),
            DetectError::Scan(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for DetectError {}

/// Detect the sequence for a FILE or a DIRECTORY.
/// - file -> the sequence containing it (via [`Scanner::from_file`])
/// - dir  -> the single sequence if EXACTLY one exists (scanned NON-recursively);
///           `Err(Ambiguous)` if 2+, `Ok(None)` if none.
///
/// No silent "pick longest" — ambiguity is a loud error by design, so callers
/// (e.g. codec-core's EXR scanner) never quietly load the wrong sequence.
///
/// `min_len = 2` is passed to [`get_seqs`] so a lone file in the directory is not
/// mistaken for a sequence, matching the rest of the crate's defaults.
#[allow(dead_code)] // Public API (unused by the bundled CLI bin)
pub fn detect<P: AsRef<Path>>(path: P) -> Result<Option<Seq>, DetectError> {
    let p = path.as_ref();
    if p.is_dir() {
        // Non-recursive scan of just this directory; min_len=2 ignores stray singletons.
        let mut seqs = get_seqs(p, false, None, 2).map_err(DetectError::Scan)?;
        match seqs.len() {
            0 => Ok(None),
            1 => Ok(Some(seqs.pop().expect("len checked == 1"))),
            n => Err(DetectError::Ambiguous { count: n }),
        }
    } else {
        // File path: AsRef<Path> flows straight into Scanner::from_file (no &str needed).
        Ok(Scanner::from_file(p))
    }
}

#[cfg(test)]
mod detect_tests {
    use super::*;
    use std::fs;

    /// Touch an empty file at `dir/name` (panics in tests only — acceptable).
    fn touch(dir: &Path, name: &str) {
        fs::write(dir.join(name), b"").expect("write temp file");
    }

    #[test]
    fn test_detect_dir_single_seq() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path();
        for n in 1..=4 {
            touch(dir, &format!("render_{n:04}.exr"));
        }
        let seq = detect(dir).expect("scan ok").expect("one sequence");
        assert_eq!(seq.start, 1);
        assert_eq!(seq.end, 4);
        assert_eq!(seq.len(), 4);
    }

    #[test]
    fn test_detect_dir_ambiguous() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path();
        // Two distinct-prefix sequences -> ambiguous, must be a loud error.
        for n in 1..=3 {
            touch(dir, &format!("alpha_{n:04}.exr"));
            touch(dir, &format!("beta_{n:04}.exr"));
        }
        match detect(dir) {
            Err(DetectError::Ambiguous { count }) => assert_eq!(count, 2),
            other => panic!("expected Ambiguous{{2}}, got {other:?}"),
        }
    }

    #[test]
    fn test_detect_dir_empty() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let res = detect(tmp.path()).expect("scan ok");
        assert!(res.is_none(), "empty dir must be Ok(None)");
    }

    #[test]
    fn test_detect_single_file() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let dir = tmp.path();
        for n in 1..=4 {
            touch(dir, &format!("shot_{n:04}.exr"));
        }
        // Pass a single file of the sequence -> resolves the whole sequence.
        let file = dir.join("shot_0002.exr");
        let seq = detect(&file).expect("scan ok").expect("sequence for file");
        assert_eq!(seq.start, 1);
        assert_eq!(seq.end, 4);
    }
}
