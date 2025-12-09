# ScanSeq - High-Performance File Sequence Detection

Fast, Rust-powered library and Python extension for detecting numbered file sequences in VFX, animation, and media production pipelines.

## Features

- **Parallel Scanning**: Uses jwalk for fast directory traversal
- **Memory Efficient**: Pre-computed digit groups, mask-based grouping
- **Smart Detection**: Automatically picks longest sequence when files have multiple number groups
- **Missing Frame Tracking**: Identifies gaps in sequences automatically
- **Single File Lookup**: Find sequence from any file path in O(n) time
- **Builder Pattern**: Fluent API for scanner configuration
- **Frame Path Resolution**: Get file paths for any frame number
- **File Scanner**: Scan files by extensions with glob patterns (`jp*`, `tif?`)

## Quick Start

### Rust API

```rust
use scanseq::core::{Scanner, Seq, get_seqs, scan_files};

fn main() {
    // Builder pattern (recommended)
    let scanner = Scanner::path("/renders")
        .recursive(true)
        .extensions(&["exr", "png", "jpg"])
        .min_len(2)
        .scan();

    // Or use VFX presets
    let scanner = Scanner::path("/renders")
        .vfx_images()  // exr, dpx, tif, png, jpg, tga, hdr
        .scan();

    println!("Found {} sequences in {:.1}ms",
        scanner.len(), scanner.result.elapsed_ms);

    for seq in scanner.iter() {
        println!("{} [{}-{}]", seq.pattern(), seq.start, seq.end);

        // Get specific frame path
        if let Some(path) = seq.get_file(seq.start) {
            println!("  First: {}", path);
        }

        // Check for gaps
        if !seq.is_complete() {
            println!("  Missing {} frames", seq.missed.len());
        }
    }

    // Classic constructor (also valid)
    let scanner = Scanner::new(
        vec!["/renders", "/comp"],
        true,           // recursive
        Some("*.exr"),  // mask
        2               // min_len
    );

    // Static methods (return ScanResult)
    let result = Scanner::get_seqs(&["/renders"], true, Some("*.exr"), 2);
    let result = Scanner::get_seq("/renders", true, Some("*.exr"), 2);

    // Find sequence from a single file
    if let Some(seq) = Scanner::from_file("/renders/shot_0001.exr") {
        println!("Found: {} [{}-{}]", seq.pattern(), seq.start, seq.end);
    }

    // Low-level function (returns Result<Vec<Seq>>)
    let seqs = get_seqs("/renders", true, Some("*.exr"), 2).unwrap();

    // Scan files by extensions (not sequences, just file list)
    let videos = scan_files(&["/media"], true, &["mp4", "mov", "avi"]).unwrap();
    let images = scan_files(&["/renders"], true, &["exr", "jp*", "tif*"]).unwrap();  // glob patterns
}
```

Add to `Cargo.toml`:
```toml
[dependencies]
scanseq = "0.1"
```

### Python API

```python
import scanseq

# Create scanner (runs automatically on construction)
scanner = scanseq.Scanner(
    roots=["/renders", "/comp"],
    recursive=True,
    mask="*.exr",
    min_len=2
)

# Access results via scanner.result
print(f"Found {len(scanner.result.seqs)} in {scanner.result.elapsed_ms:.1f}ms")

for seq in scanner.result.seqs:
    print(f"{seq.pattern} [{seq.start}-{seq.end}]")

    # Get specific frame path
    path = seq.get_file(seq.start)
    if path:
        print(f"  First: {path}")

    # Check completeness
    if not seq.is_complete():
        print(f"  Missing: {seq.missed}")

# Static methods
result = scanseq.Scanner.get_seqs(["/renders"], recursive=True)
result = scanseq.Scanner.get_seq("/renders", mask="*.exr")

# Find sequence from a single file
seq = scanseq.Scanner.from_file("/renders/shot_0001.exr")
if seq:
    print(f"{seq.pattern} [{seq.start}-{seq.end}]")

# Convert Seq to dict
data = dict(seq)  # or seq.to_dict()

# Expand to all frame paths
all_paths = seq.expand()  # ["/renders/shot_0001.exr", ...]

# Rescan with same settings
scanner.rescan()
```

### CLI

```bash
# Show help
scanseq-cli

# Scan paths and print results
scanseq-cli -p /renders -p /comp -o

# Recursive scan
scanseq-cli -p /renders -r -o

# With mask filter
scanseq-cli -p /renders -m "*.exr" -o

# JSON output
scanseq-cli -p /renders -oj

# Scan files by extensions (not sequences)
scanseq-cli -p /media -s mp4 mov avi -r -o

# With glob patterns
scanseq-cli -p /renders -s exr jp* tif* -r -o

# JSON file list
scanseq-cli -p /media -s mp4 -r -oj
```

## API Reference

### Rust

#### `Scanner`

Stateful scanner with configuration and results:

```rust
pub struct Scanner {
    pub roots: Vec<String>,
    pub recursive: bool,
    pub mask: Option<String>,
    pub min_len: usize,
    pub result: ScanResult,
}

impl Scanner {
    // Builder pattern (recommended)
    pub fn path(root: P) -> ScannerBuilder      // Single path
    pub fn paths(roots: &[P]) -> ScannerBuilder // Multiple paths

    // Classic constructor - scans immediately
    pub fn new(roots: Vec<S>, recursive: bool, mask: Option<&str>, min_len: usize) -> Self

    // Static methods - return ScanResult
    pub fn get_seq(root: P, recursive: bool, mask: Option<&str>, min_len: usize) -> ScanResult
    pub fn get_seqs(roots: &[P], recursive: bool, mask: Option<&str>, min_len: usize) -> ScanResult

    // Find sequence containing a file (scans parent directory)
    pub fn from_file(path: P) -> Option<Seq>

    // Instance methods
    pub fn rescan(&mut self)
    pub fn len(&self) -> usize
    pub fn is_empty(&self) -> bool
    pub fn iter(&self) -> impl Iterator<Item = &Seq>
}
```

#### `ScannerBuilder`

Fluent builder for scanner configuration:

```rust
pub struct ScannerBuilder { ... }

impl ScannerBuilder {
    pub fn recursive(self, recursive: bool) -> Self
    pub fn mask(self, mask: &str) -> Self
    pub fn extensions(self, exts: &[&str]) -> Self  // ["exr", "png"] -> "*.{exr,png}"
    pub fn vfx_images(self) -> Self                  // Preset: exr, dpx, tif, png, jpg, tga, hdr
    pub fn min_len(self, min_len: usize) -> Self
    pub fn scan(self) -> Scanner                     // Execute scan
    pub fn into_seqs(self) -> Vec<Seq>               // Scan and return sequences only
}
```

#### `ScanResult`

```rust
pub struct ScanResult {
    pub seqs: Vec<Seq>,
    pub elapsed_ms: f64,
    pub errors: Vec<String>,
}
```

#### `get_seqs`

Low-level sequence scanning function:

```rust
pub fn get_seqs<P: AsRef<Path>>(
    root: P,                    // Directory to scan
    recursive: bool,            // Scan subdirectories
    mask: Option<&str>,         // Glob pattern filter
    min_len: usize              // Minimum sequence length
) -> Result<Vec<Seq>, String>
```

#### `scan_files`

Scan files by extensions (returns file paths, not sequences):

```rust
pub fn scan_files<P: AsRef<Path>>(
    roots: &[P],                // Directories to scan
    recursive: bool,            // Scan subdirectories
    exts: &[&str]               // Extensions or glob patterns
) -> Result<Vec<PathBuf>, String>
```

Examples:
```rust
// Exact extensions
let videos = scan_files(&["/media"], true, &["mp4", "mov", "avi"])?;

// Glob patterns
let images = scan_files(&["/renders"], true, &["jp*", "tif?"])?;  // jpg, jpeg, jp2, tiff

// All files (empty extensions)
let all = scan_files(&["/data"], true, &[])?;
```

#### `Seq`

Sequence struct with frame operations:

```rust
pub struct Seq {
    pub indices: Vec<i64>,      // Frame numbers present
    pub missed: Vec<i64>,       // Missing frame numbers
    pub start: i64,             // First frame
    pub end: i64,               // Last frame
    pub padding: usize,         // 0 = variable, >=2 = fixed width
}

impl Seq {
    // Basic info
    pub fn pattern(&self) -> &str       // Pattern string ("img_####.exr")
    pub fn len(&self) -> usize          // Number of files
    pub fn is_empty(&self) -> bool      // Check if empty

    // Frame operations
    pub fn get_file(&self, frame: i64) -> Option<String>  // Get path for frame
    pub fn first_file(&self) -> String                     // First frame path
    pub fn last_file(&self) -> String                      // Last frame path
    pub fn is_complete(&self) -> bool                      // No missing frames?
    pub fn frame_count(&self) -> usize                     // Number of existing frames
    pub fn range_count(&self) -> i64                       // Total range size

    // Expansion
    pub fn expand(&self) -> Result<Vec<String>, String>    // All paths in range
    pub fn expand_existing(&self) -> Vec<String>           // Only existing frame paths

    // Serialization
    pub fn to_json(&self) -> String                        // JSON string
    pub fn to_json_pretty(&self) -> String                 // Pretty JSON
    pub fn to_map(&self) -> HashMap<&str, serde_json::Value>
}

// Implements Display: "Seq("img_####.exr", range: 1-100)"
// Implements Serialize (serde)
```

#### Constants

```rust
pub const VFX_IMAGE_EXTS: &[&str];  // ["exr", "dpx", "tif", "tiff", "png", "jpg", "jpeg", "tga", "hdr"]
pub const VIDEO_EXTS: &[&str];      // ["mp4", "mov", "avi", "mkv", "webm", "m4v", "mxf"]
```

### Python

#### Scanner

Stateful scanner class that runs on construction:

```python
scanner = scanseq.Scanner(
    roots: list[str],           # Directories to scan
    recursive: bool = True,     # Scan subdirectories
    mask: str | None = None,    # Glob pattern (e.g., "*.exr")
    min_len: int = 2            # Minimum sequence length
)
```

**Attributes:**
```python
scanner.roots        # list[str] - directories scanned
scanner.recursive    # bool
scanner.mask         # str | None
scanner.min_len      # int
scanner.result       # ScanResult - scan results
```

**Static Methods:**
```python
Scanner.get_seq(root, recursive=True, mask=None, min_len=2)   # Single path
Scanner.get_seqs(roots, recursive=True, mask=None, min_len=2) # Multiple paths
Scanner.from_file(path)                                       # Find seq from file
```

**Instance Methods:**
```python
scanner.rescan()     # Re-scan with current settings
len(scanner)         # Number of sequences
for seq in scanner:  # Iterate over sequences
    ...
```

#### ScanResult

```python
result.seqs          # list[Seq] - detected sequences
result.elapsed_ms    # float - scan duration in ms
result.errors        # list[str] - errors encountered
len(result)          # Number of sequences
for seq in result:   # Iterate over sequences
    ...
```

#### Seq

Sequence object with frame information:

```python
# Attributes
seq.pattern      # "shot_####.exr" (#### = padded, @ = unpadded)
seq.start        # First frame number
seq.end          # Last frame number
seq.padding      # Padding width (4 for 0001)
seq.indices      # list[int] - actual frames present
seq.missed       # list[int] - missing frames

# Frame operations
seq.get_file(frame)   # Get path for specific frame (None if missing)
seq.is_complete()     # True if no missing frames
seq.expand()          # All frame paths in range (including missing)

# Conversion
seq.to_dict()         # Convert to dictionary
dict(seq)             # Also works via Mapping protocol
seq["pattern"]        # Item access via Mapping protocol

# Magic methods
len(seq)              # Number of files
str(seq)              # String representation
repr(seq)             # Detailed representation
```

### CLI

```bash
scanseq-cli [OPTIONS]

Options:
  -p, --path <PATH>           Directory to scan (can specify multiple)
  -r, --recursive             Scan subdirectories recursively
  -m, --mask <MASK>           File mask/glob pattern for sequences
  -s, --scan-files <EXT>...   Scan files by extensions (e.g., -s mp4 mov jp*)
  -n, --min <N>               Minimum sequence length (default: 2)
  -o, --out                   Print results to stdout (default: off)
  -j, --json                  Use JSON format (with -o)
  -h, --help                  Print help
```

## Installation

### From crates.io

```toml
[dependencies]
scanseq = "0.1"
```

### From Source

```bash
# Build CLI
cargo build --release

# Install Python module
pip install maturin
maturin develop --features python
```

## Architecture

### Algorithm

1. **Scan**: Parallel directory traversal with jwalk
2. **Parse**: Extract digit groups from filenames, create masks
3. **Group**: Hash by mask (e.g., `render_@.exr`), sub-group by anchors
4. **Detect**: Find frame numbers, compute padding, identify gaps

### Pattern Notation

- `####` - Padded sequences (e.g., `0001`, `0002`)
- `@` - Unpadded sequences (e.g., `1`, `2`, `100`)

Examples:
- `render_####.exr` -> `render_0001.exr`, `render_0002.exr`
- `shot_@.png` -> `shot_1.png`, `shot_2.png`

## Examples

### Find Missing Frames

```rust
let scanner = Scanner::path("/renders").vfx_images().scan();

for seq in scanner.iter() {
    if !seq.is_complete() {
        println!("{}: missing frames {:?}", seq.pattern(), seq.missed);
    }
}
```

### Generate Contact Sheet

```rust
let seq = Scanner::from_file("/renders/shot_0001.exr").unwrap();

// Get evenly spaced frames for thumbnail generation
let step = seq.frame_count() / 10;
for (i, frame) in seq.indices.iter().step_by(step.max(1)).enumerate() {
    if let Some(path) = seq.get_file(*frame) {
        println!("Thumbnail {}: {}", i, path);
    }
}
```

### Validate Sequence Completeness

```python
import scanseq

scanner = scanseq.Scanner(["/renders"], mask="*.exr")

incomplete = [s for s in scanner.result.seqs if not s.is_complete()]
for seq in incomplete:
    print(f"INCOMPLETE: {seq.pattern}")
    print(f"  Range: {seq.start}-{seq.end}")
    print(f"  Missing: {len(seq.missed)} frames")
    print(f"  First missing: {seq.missed[:5]}")
```

## Development

```bash
# Run tests
cargo test

# Build with Python
cargo build --features python

# Python module dev install
maturin develop --features python
```

## License

MIT
