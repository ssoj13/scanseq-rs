//! # ScanSeq - High-Performance File Sequence Detection
//!
//! Fast, Rust-powered library and Python extension for detecting numbered file sequences.
//! Designed for VFX, animation, and media production pipelines.
//!
//! # Features
//!
//! - **Parallel Scanning**: Uses jwalk for fast directory traversal
//! - **Memory Efficient**: Pre-computed digit groups, mask-based grouping
//! - **Smart Detection**: Automatically picks longest sequence when files have multiple number groups
//! - **Missing Frame Tracking**: Identifies gaps in sequences automatically
//! - **Builder Pattern**: Fluent API for scanner configuration
//! - **Frame Path Resolution**: Get file paths for any frame number
//!
//! # Quick Start
//!
//! ```ignore
//! use scanseq::core::Scanner;
//!
//! // Builder pattern (recommended)
//! let scanner = Scanner::path("/renders")
//!     .recursive(true)
//!     .extensions(&["exr", "png", "jpg"])
//!     .min_len(2)
//!     .scan();
//!
//! // Or use VFX presets
//! let scanner = Scanner::path("/renders")
//!     .vfx_images()  // exr, dpx, tif, png, jpg, tga, hdr
//!     .scan();
//!
//! println!("Found {} sequences in {:.1}ms",
//!     scanner.len(), scanner.result.elapsed_ms);
//!
//! for seq in scanner.iter() {
//!     println!("{} [{}-{}]", seq.pattern(), seq.start, seq.end);
//!
//!     // Get specific frame path
//!     if let Some(path) = seq.get_file(seq.start) {
//!         println!("  First: {}", path);
//!     }
//!
//!     // Check for gaps
//!     if !seq.is_complete() {
//!         println!("  Missing {} frames", seq.missed.len());
//!     }
//! }
//! ```
//!
//! # Classic Constructor
//!
//! ```ignore
//! use scanseq::core::Scanner;
//!
//! let scanner = Scanner::new(
//!     vec!["/renders", "/comp"],
//!     true,           // recursive
//!     Some("*.exr"),  // mask
//!     2               // min_len
//! );
//!
//! for seq in scanner.iter() {
//!     println!("{}", seq);
//! }
//! ```
//!
//! # Static Methods
//!
//! ```ignore
//! use scanseq::core::Scanner;
//!
//! // Scan single path
//! let result = Scanner::get_seq("/renders", true, Some("*.exr"), 2);
//!
//! // Scan multiple paths in parallel
//! let result = Scanner::get_seqs(&["/renders", "/comp"], true, Some("*.exr"), 2);
//!
//! // Find sequence from a single file
//! if let Some(seq) = Scanner::from_file("/renders/shot_0001.exr") {
//!     println!("Found: {} [{}-{}]", seq.pattern(), seq.start, seq.end);
//! }
//! ```
//!
//! # Seq Methods
//!
//! ```ignore
//! // Frame operations
//! seq.get_file(42)       // Get path for frame 42
//! seq.first_file()       // First frame path
//! seq.last_file()        // Last frame path
//! seq.is_complete()      // No missing frames?
//! seq.frame_count()      // Number of existing frames
//! seq.range_count()      // Total range size
//!
//! // Expansion
//! seq.expand()           // All paths in range (Result<Vec<String>>)
//! seq.expand_existing()  // Only existing frame paths
//!
//! // Serialization
//! seq.to_json()          // JSON string
//! seq.to_json_pretty()   // Pretty JSON
//! ```
//!
//! # Pattern Notation
//!
//! - `####` - Padded sequences (e.g., `0001`, `0002`)
//! - `@` - Unpadded sequences (e.g., `1`, `2`, `100`)
//!
//! Examples:
//! - `render_####.exr` -> `render_0001.exr`, `render_0002.exr`
//! - `shot_@.png` -> `shot_1.png`, `shot_2.png`
//!
//! # Python API
//!
//! Enable with `--features python`:
//!
//! ```python
//! import scanseq
//!
//! scanner = scanseq.Scanner(["/renders"], recursive=True, mask="*.exr")
//! for seq in scanner.result.seqs:
//!     print(f"{seq.pattern} [{seq.start}-{seq.end}]")
//!     path = seq.get_file(seq.start)  # Get first frame path
//!     if not seq.is_complete():
//!         print(f"  Missing: {seq.missed}")
//! ```
//!
//! # Algorithm Overview
//!
//! 1. **Scan**: Discover directories using jwalk (parallel)
//! 2. **Parse**: Extract digit groups from filenames, create masks
//! 3. **Group**: Hash by mask (e.g., `render_@.exr`), sub-group by anchors
//! 4. **Detect**: Find frame numbers, compute padding, identify gaps
//!
//! See [`core`] module for detailed API documentation.

pub mod core;

#[cfg(feature = "python")]
use pyo3::prelude::*;
#[cfg(feature = "python")]
use pyo3::types::PyDict;
#[cfg(feature = "python")]
use std::sync::Arc;
#[cfg(feature = "python")]
use std::time::Instant;

#[cfg(feature = "python")]
use core::Seq as CoreSeq;
#[cfg(feature = "python")]
use rayon::prelude::*;

/// Python-facing Seq class wrapping core::Seq
#[cfg(feature = "python")]
#[pyclass(name = "Seq")]
#[derive(Clone)]
pub struct PySeq {
    #[pyo3(get)]
    pattern: String,
    #[pyo3(get)]
    start: i64,
    #[pyo3(get)]
    end: i64,
    #[pyo3(get)]
    padding: usize,
    #[pyo3(get)]
    indices: Vec<i64>,
    #[pyo3(get)]
    missed: Vec<i64>,
}

#[cfg(feature = "python")]
impl From<CoreSeq> for PySeq {
    fn from(s: CoreSeq) -> Self {
        PySeq {
            pattern: s.pattern().to_string(),
            start: s.start,
            end: s.end,
            padding: s.padding,
            indices: s.indices.clone(),
            missed: s.missed.clone(),
        }
    }
}

#[cfg(feature = "python")]
impl PySeq {
    /// Format frame number into path using pattern
    fn format_frame(&self, frame: i64) -> String {
        if self.padding >= 2 {
            let placeholder = "#".repeat(self.padding);
            let frame_str = format!("{:0width$}", frame, width = self.padding);
            self.pattern.replace(&placeholder, &frame_str)
        } else {
            self.pattern.replace('@', &frame.to_string())
        }
    }
}

#[cfg(feature = "python")]
#[pymethods]
impl PySeq {
    fn __repr__(&self) -> String {
        if self.missed.is_empty() {
            format!(
                "Seq(\"{}\", start={}, end={}, frames={})",
                self.pattern, self.start, self.end, self.indices.len()
            )
        } else {
            format!(
                "Seq(\"{}\", start={}, end={}, frames={}, missed={})",
                self.pattern, self.start, self.end, self.indices.len(), self.missed.len()
            )
        }
    }

    /// Support dict(seq) by implementing Mapping protocol
    fn keys(&self) -> Vec<&str> {
        vec!["pattern", "start", "end", "padding", "indices", "missed", "count"]
    }

    fn __getitem__(&self, key: &str) -> PyResult<PyObject> {
        Python::with_gil(|py| {
            match key {
                "pattern" => Ok(self.pattern.clone().into_pyobject(py)?.into_any().unbind()),
                "start" => Ok(self.start.into_pyobject(py)?.into_any().unbind()),
                "end" => Ok(self.end.into_pyobject(py)?.into_any().unbind()),
                "padding" => Ok(self.padding.into_pyobject(py)?.into_any().unbind()),
                "indices" => Ok(self.indices.clone().into_pyobject(py)?.into_any().unbind()),
                "missed" => Ok(self.missed.clone().into_pyobject(py)?.into_any().unbind()),
                "count" => Ok(self.indices.len().into_pyobject(py)?.into_any().unbind()),
                _ => Err(pyo3::exceptions::PyKeyError::new_err(format!("Unknown key: {}", key))),
            }
        })
    }

    fn __str__(&self) -> String {
        self.__repr__()
    }

    /// Number of files in sequence
    fn __len__(&self) -> usize {
        self.indices.len()
    }

    /// Get file path for specific frame number.
    /// Returns None if frame doesn't exist (not in indices).
    #[pyo3(signature = (frame))]
    fn get_file(&self, frame: i64) -> Option<String> {
        // O(log n) lookup in sorted indices - handles large gaps correctly
        if self.indices.binary_search(&frame).is_ok() {
            Some(self.format_frame(frame))
        } else {
            None
        }
    }

    /// Check if sequence is complete (no missing frames)
    fn is_complete(&self) -> bool {
        self.missed.is_empty()
    }

    /// Expand to all frame paths in range (including missing).
    /// Limited to 1M frames to prevent OOM.
    fn expand(&self) -> PyResult<Vec<String>> {
        const MAX_EXPAND: i64 = 1_000_000;
        let count = self.end.saturating_sub(self.start).saturating_add(1);
        if count > MAX_EXPAND {
            return Err(pyo3::exceptions::PyValueError::new_err(
                format!("Range too large: {} frames (max {})", count, MAX_EXPAND)
            ));
        }
        Ok((self.start..=self.end).map(|f| self.format_frame(f)).collect())
    }

    /// Convert to dict
    fn to_dict(&self, py: Python) -> PyResult<Py<PyAny>> {
        let dict = PyDict::new(py);
        dict.set_item("pattern", &self.pattern)?;
        dict.set_item("start", self.start)?;
        dict.set_item("end", self.end)?;
        dict.set_item("padding", self.padding)?;
        dict.set_item("indices", &self.indices)?;
        dict.set_item("missed", &self.missed)?;
        dict.set_item("count", self.indices.len())?;
        Ok(dict.into_any().unbind())
    }
}

/// Python-facing ScanResult class wrapping core::ScanResult
#[cfg(feature = "python")]
#[pyclass(name = "ScanResult")]
#[derive(Clone)]
pub struct PyScanResult {
    /// Detected sequences (Arc for cheap iterator cloning)
    seqs: Arc<Vec<PySeq>>,
    /// Scan duration in milliseconds
    #[pyo3(get)]
    elapsed_ms: f64,
    /// Errors encountered during scan
    #[pyo3(get)]
    errors: Vec<String>,
}

#[cfg(feature = "python")]
#[pymethods]
impl PyScanResult {
    /// Get sequences list (clones Vec for Python ownership)
    #[getter]
    fn seqs(&self) -> Vec<PySeq> {
        (*self.seqs).clone()
    }

    fn __repr__(&self) -> String {
        format!(
            "ScanResult(seqs={}, elapsed={:.2}ms, errors={})",
            self.seqs.len(), self.elapsed_ms, self.errors.len()
        )
    }

    fn __len__(&self) -> usize {
        self.seqs.len()
    }

    /// Iterate over sequences (cheap Arc clone, no Vec copy)
    fn __iter__(slf: PyRef<'_, Self>) -> PyResult<Py<SeqIter>> {
        Py::new(slf.py(), SeqIter {
            seqs: Arc::clone(&slf.seqs),
            index: 0,
        })
    }
}

/// Stateful scanner - stores configuration and results (1:1 with Rust API)
#[cfg(feature = "python")]
#[pyclass]
pub struct Scanner {
    /// Root paths to scan
    #[pyo3(get)]
    roots: Vec<String>,
    /// Recursive scanning enabled
    #[pyo3(get)]
    recursive: bool,
    /// File mask filter
    #[pyo3(get)]
    mask: Option<String>,
    /// Minimum sequence length
    #[pyo3(get)]
    min_len: usize,
    /// Scan results (sequences, elapsed_ms, errors)
    #[pyo3(get)]
    result: PyScanResult,
}

#[cfg(feature = "python")]
#[pymethods]
impl Scanner {
    /// Create scanner and run initial scan.
    ///
    /// Args:
    ///     roots: List of directory paths to scan
    ///     recursive: Scan subdirectories (default: True)
    ///     mask: File mask/glob pattern (e.g., "*.exr")
    ///     min_len: Minimum sequence length (default: 2)
    #[new]
    #[pyo3(signature = (roots, recursive=true, mask=None, min_len=2))]
    fn new(py: Python, roots: Vec<String>, recursive: bool, mask: Option<String>, min_len: usize) -> PyResult<Self> {
        let mut scanner = Scanner {
            roots,
            recursive,
            mask,
            min_len,
            result: PyScanResult {
                seqs: Arc::new(Vec::new()),
                elapsed_ms: 0.0,
                errors: Vec::new(),
            },
        };
        scanner.rescan_impl(py)?;
        Ok(scanner)
    }

    /// Scan a single path (static method).
    ///
    /// Args:
    ///     root: Directory path to scan
    ///     recursive: Scan subdirectories (default: True)
    ///     mask: File mask/glob pattern
    ///     min_len: Minimum sequence length (default: 2)
    ///
    /// Returns:
    ///     ScanResult with sequences, elapsed_ms, and errors
    #[staticmethod]
    #[pyo3(signature = (root, recursive=true, mask=None, min_len=2))]
    fn get_seq(py: Python, root: String, recursive: bool, mask: Option<String>, min_len: usize) -> PyResult<PyScanResult> {
        let start = Instant::now();

        let (seqs, errors) = py.allow_threads(|| {
            match core::get_seqs(&root, recursive, mask.as_deref(), min_len) {
                Ok(s) => (s, Vec::new()),
                Err(e) => (Vec::new(), vec![e]),
            }
        });

        Ok(PyScanResult {
            seqs: Arc::new(seqs.into_iter().map(PySeq::from).collect()),
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
            errors,
        })
    }

    /// Scan multiple paths in parallel (static method).
    ///
    /// Args:
    ///     roots: List of directory paths to scan
    ///     recursive: Scan subdirectories (default: True)
    ///     mask: File mask/glob pattern
    ///     min_len: Minimum sequence length (default: 2)
    ///
    /// Returns:
    ///     ScanResult with sequences, elapsed_ms, and errors
    #[staticmethod]
    #[pyo3(signature = (roots, recursive=true, mask=None, min_len=2))]
    fn get_seqs(py: Python, roots: Vec<String>, recursive: bool, mask: Option<String>, min_len: usize) -> PyResult<PyScanResult> {
        let start = Instant::now();

        // Scan roots in parallel
        let (seqs, errors) = py.allow_threads(|| {
            let results: Vec<_> = roots.par_iter().map(|root| {
                match core::get_seqs(root, recursive, mask.as_deref(), min_len) {
                    Ok(s) => (s, None),
                    Err(e) => (Vec::new(), Some(format!("{}: {}", root, e))),
                }
            }).collect();

            let mut all_seqs = Vec::new();
            let mut all_errors = Vec::new();
            for (seqs, err) in results {
                all_seqs.extend(seqs);
                if let Some(e) = err {
                    all_errors.push(e);
                }
            }
            (all_seqs, all_errors)
        });

        Ok(PyScanResult {
            seqs: Arc::new(seqs.into_iter().map(PySeq::from).collect()),
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
            errors,
        })
    }

    /// Find sequence containing the given file.
    /// Scans parent directory (non-recursive) to find matching files.
    ///
    /// Args:
    ///     path: Path to a file that may be part of a sequence
    ///
    /// Returns:
    ///     Seq if file is part of a sequence, None otherwise
    #[staticmethod]
    #[pyo3(signature = (path))]
    fn from_file(py: Python, path: String) -> Option<PySeq> {
        py.allow_threads(|| {
            core::Scanner::from_file(&path).map(PySeq::from)
        })
    }

    /// Re-scan all roots with current settings.
    /// Updates result with new sequences, elapsed_ms, and errors.
    fn rescan(&mut self, py: Python) -> PyResult<()> {
        self.rescan_impl(py)
    }

    /// Number of sequences found
    fn __len__(&self) -> usize {
        self.result.seqs.len()
    }

    fn __repr__(&self) -> String {
        format!(
            "Scanner(roots={}, seqs={}, elapsed={:.2}ms)",
            self.roots.len(),
            self.result.seqs.len(),
            self.result.elapsed_ms
        )
    }

    /// Iterate over sequences (convenience, same as iter(scanner.result))
    fn __iter__(slf: PyRef<'_, Self>) -> PyResult<Py<SeqIter>> {
        Py::new(slf.py(), SeqIter {
            seqs: Arc::clone(&slf.result.seqs),
            index: 0,
        })
    }
}

#[cfg(feature = "python")]
impl Scanner {
    fn rescan_impl(&mut self, py: Python) -> PyResult<()> {
        let start = Instant::now();

        // Clone config for GIL-free scanning
        let roots = self.roots.clone();
        let recursive = self.recursive;
        let mask = self.mask.clone();
        let min_len = self.min_len;

        // Release GIL during parallel Rust file scanning
        let (seqs, errors) = py.allow_threads(|| {
            let results: Vec<_> = roots.par_iter().map(|root| {
                match core::get_seqs(root, recursive, mask.as_deref(), min_len) {
                    Ok(s) => (s, None),
                    Err(e) => (Vec::new(), Some(format!("{}: {}", root, e))),
                }
            }).collect();

            let mut all_seqs = Vec::new();
            let mut all_errors = Vec::new();
            for (seqs, err) in results {
                all_seqs.extend(seqs);
                if let Some(e) = err {
                    all_errors.push(e);
                }
            }
            (all_seqs, all_errors)
        });

        // Update result (GIL held again)
        self.result = PyScanResult {
            seqs: Arc::new(seqs.into_iter().map(PySeq::from).collect()),
            elapsed_ms: start.elapsed().as_secs_f64() * 1000.0,
            errors,
        };

        Ok(())
    }
}

/// Iterator for sequences (uses Arc for cheap cloning)
#[cfg(feature = "python")]
#[pyclass]
pub struct SeqIter {
    seqs: Arc<Vec<PySeq>>,
    index: usize,
}

#[cfg(feature = "python")]
#[pymethods]
impl SeqIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self) -> Option<PySeq> {
        if self.index < self.seqs.len() {
            let seq = self.seqs[self.index].clone();
            self.index += 1;
            Some(seq)
        } else {
            None
        }
    }
}

#[cfg(feature = "python")]
#[pymodule]
fn scanseq(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Scanner>()?;
    m.add_class::<PyScanResult>()?;
    m.add_class::<PySeq>()?;
    Ok(())
}
