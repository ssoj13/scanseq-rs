# ScanSeq - High-Performance File Sequence Detection

Fast, Rust-powered library and Python extension for detecting numbered file sequences in VFX, animation, and media production pipelines.

## Features

- **Parallel Scanning**: Uses jwalk for fast directory traversal
- **Memory Efficient**: Pre-computed digit groups, mask-based grouping
- **Smart Detection**: Automatically picks longest sequence when files have multiple number groups
- **Missing Frame Tracking**: Identifies gaps in sequences automatically

## Quick Start

### Rust API

```rust
use scanseq::core::{Scanner, get_seqs};

fn main() {
    // Using Scanner (stateful, with timing)
    let scanner = Scanner::new(
        vec!["/renders", "/comp"],
        true,           // recursive
        Some("*.exr"),  // mask
        2               // min_len
    );

    println!("Found {} sequences in {:.1}ms",
        scanner.len(), scanner.result.elapsed_ms);

    for seq in scanner.iter() {
        println!("{} [{}-{}]", seq.pattern(), seq.start, seq.end);
    }

    // Static methods (return ScanResult)
    let result = Scanner::get_seqs(&["/renders"], true, Some("*.exr"), 2);
    let result = Scanner::get_seq("/renders", true, Some("*.exr"), 2);

    // Low-level function (returns Result<Vec<Seq>>)
    let seqs = get_seqs("/renders", true, Some("*.exr"), 2)?;
}
```

Add to `Cargo.toml`:
```toml
[dependencies]
scanseq = { path = "path/to/scanseq-rs" }
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
    if seq.missed:
        print(f"  Missing: {seq.missed}")

# Static methods
result = scanseq.Scanner.get_seqs(["/renders"], recursive=True)
result = scanseq.Scanner.get_seq("/renders", mask="*.exr")

# Convert Seq to dict
data = dict(seq)  # or seq.to_dict()

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
    // Constructor - scans immediately
    pub fn new(roots: Vec<S>, recursive: bool, mask: Option<&str>, min_len: usize) -> Self

    // Static methods - return ScanResult
    pub fn get_seq(root: P, recursive: bool, mask: Option<&str>, min_len: usize) -> ScanResult
    pub fn get_seqs(roots: &[P], recursive: bool, mask: Option<&str>, min_len: usize) -> ScanResult

    // Instance methods
    pub fn rescan(&mut self)
    pub fn len(&self) -> usize
    pub fn is_empty(&self) -> bool
    pub fn iter(&self) -> impl Iterator<Item = &Seq>
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

Low-level scanning function:

```rust
pub fn get_seqs<P: AsRef<Path>>(
    root: P,                    // Directory to scan
    recursive: bool,            // Scan subdirectories
    mask: Option<&str>,         // Glob pattern filter
    min_len: usize              // Minimum sequence length
) -> Result<Vec<Seq>, String>
```

#### `Seq`

Sequence struct:

```rust
pub struct Seq {
    pub indices: Vec<i64>,      // Frame numbers present
    pub missed: Vec<i64>,       // Missing frame numbers
    pub start: i64,             // First frame
    pub end: i64,               // Last frame
    pub padding: usize,         // 0 = variable, >=2 = fixed width
}

impl Seq {
    pub fn pattern(&self) -> &str   // Pattern string
    pub fn len(&self) -> usize      // Number of files
    pub fn is_empty(&self) -> bool  // Check if empty
}
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
seq.pattern      # "shot_####.exr" (#### = padded, @ = unpadded)
seq.start        # First frame number
seq.end          # Last frame number
seq.padding      # Padding width (4 for 0001)
seq.indices      # list[int] - actual frames present
seq.missed       # list[int] - missing frames

# Methods
seq.get_file(frame)   # Get path for specific frame
seq.is_complete()     # True if no missing frames
seq.expand()          # All frame paths in range
seq.to_dict()         # Convert to dictionary
dict(seq)             # Also works via Mapping protocol
len(seq)              # Number of files
```

### CLI

```bash
scanseq-cli [OPTIONS]

Options:
  -p, --path <PATH>   Directory to scan (can specify multiple)
  -r, --recursive     Scan subdirectories recursively
  -m, --mask <MASK>   File mask/glob pattern
  -n, --min <N>       Minimum sequence length (default: 2)
  -o, --out           Print sequences to stdout (default: off)
  -j, --json          Use JSON format (with -o)
  -h, --help          Print help
```

## Installation

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

## Development

```bash
# Run tests
cargo test

# Build with Python
cargo build --features python

# Python module dev install
maturin develop --features python
```
