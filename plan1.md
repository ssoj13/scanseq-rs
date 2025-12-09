# ScanSeq Bug Hunt Report - Plan 1

## Summary

Comprehensive code review of scanseq-rs crate. Found **8 issues**: 2 high, 4 medium, 2 low priority.

**Test Status**: All 38 tests pass. No TODO/FIXME comments found.

---

## Issues Found

### HIGH Priority

#### 1. `extract_seq()` Logic Error
**File**: `src/core/seq/mod.rs:324`

**Problem**: Function analyzes only target file instead of all matching files to determine frame group.

```rust
// CURRENT (wrong):
let frame_grp_idx = find_frame_group(std::slice::from_ref(target));

// SHOULD BE:
let frame_grp_idx = find_frame_group(&matching);
```

**Impact**: May select wrong digit group as frame number for files with multiple number groups like `shot_01_frame_0001.exr`.

**Fix**: Change to analyze all files in matching group.

---

#### 2. Symlink Infinite Loop Risk
**File**: `src/core/scan.rs:85-89`

**Problem**: `scan_files` uses jwalk without `follow_links(false)`. Circular symlinks will hang.

```rust
// CURRENT:
let walker = WalkDir::new(root.as_ref());

// SHOULD BE:
let walker = WalkDir::new(root.as_ref()).follow_links(false);
```

**Impact**: Application hang on filesystems with circular symlinks.

**Fix**: Add `.follow_links(false)` to walker creation.

---

### MEDIUM Priority

#### 3. Silent Error Handling in `scan_files`
**File**: `src/core/scan.rs:93`

**Problem**: Errors silently ignored with `.filter_map(|e| e.ok())`.

**Comparison**: `scan_dirs` (line 43) properly logs with `warn!()`.

```rust
// CURRENT:
.filter_map(|e| e.ok())

// SHOULD BE:
.filter_map(|e| match e {
    Ok(entry) => Some(entry),
    Err(err) => { warn!("Skipping: {}", err); None }
})
```

**Impact**: Users get no feedback when files are inaccessible.

---

#### 4. No Parallelism in `scan_files`
**File**: `src/core/scan.rs:82-83`

**Problem**: Uses sequential `iter()` while `get_seqs` uses `par_iter()`.

```rust
// CURRENT:
let files: Vec<PathBuf> = roots.iter().flat_map(...)

// SHOULD BE:
let files: Vec<PathBuf> = roots.par_iter().flat_map(...)
```

**Impact**: Performance degradation for multiple roots.

---

#### 5. Python Bindings Missing `scan_files`
**File**: `src/lib.rs`

**Problem**: `scan_files` function not exposed to Python API.

**Fix**: Add static method to Scanner class:

```rust
#[staticmethod]
#[pyo3(signature = (roots, recursive=true, exts=vec![]))]
fn scan_files(py: Python, roots: Vec<String>, recursive: bool, exts: Vec<String>) -> PyResult<Vec<String>> {
    let ext_refs: Vec<&str> = exts.iter().map(|s| s.as_str()).collect();
    py.allow_threads(|| {
        core::scan_files(&roots, recursive, &ext_refs)
            .map(|files| files.iter().map(|p| p.display().to_string()).collect())
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e))
    })
}
```

---

#### 6. Python Bindings Missing PySeq Methods
**File**: `src/lib.rs`

**Missing methods** (exist in Rust Seq but not in PySeq):
- `first_file()` - returns first frame path
- `last_file()` - returns last frame path
- `expand_existing()` - returns only existing frame paths
- `range_count()` - returns total range size
- `to_json()` / `to_json_pretty()` - JSON serialization

**Fix**: Add these methods to `#[pymethods] impl PySeq`.

---

### LOW Priority

#### 7. Duplicated `format_frame()` Logic
**File**: `src/lib.rs:189-198`

**Problem**: `PySeq::format_frame()` duplicates `Seq::format_frame()` from `seq/mod.rs:127-137`.

**Risk**: Logic drift if Rust implementation changes.

**Fix**: Store `CoreSeq` inside `PySeq` and delegate, or extract to shared function.

---

#### 8. Missing Python Constants
**File**: `src/lib.rs`

**Problem**: `VFX_IMAGE_EXTS` and `VIDEO_EXTS` not exposed to Python.

**Fix**: Add module-level constants:

```rust
#[pymodule]
fn scanseq(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("VFX_IMAGE_EXTS", core::VFX_IMAGE_EXTS)?;
    m.add("VIDEO_EXTS", core::VIDEO_EXTS)?;
    // ... existing classes
}
```

---

## Dataflow Diagram

```
User Request
     |
     v
+--------------------+
|   CLI (main.rs)    |
|  -p path -s exts   |
+--------------------+
     |
     +---> scan_files() -----> File list (Vec<PathBuf>)
     |          |
     |          +---> jwalk::WalkDir (parallel dir walk)
     |          +---> glob::Pattern (extension matching)
     |
     +---> Scanner::get_seqs()
               |
               v
     +--------------------+
     |  scan_dirs()       |  Phase 1: Discover folders
     +--------------------+
               |
               v
     +--------------------+
     |  rayon par_iter    |  Phase 2: Process folders
     |  scan_files_glob() |
     +--------------------+
               |
               v
     +--------------------+
     |  File::new()       |  Parse filenames, extract digit groups
     +--------------------+
               |
               v
     +--------------------+
     |  Seq::group_seqs() |  Group by mask, sub-group by anchors
     +--------------------+
               |
               v
     +--------------------+
     |  Seq (result)      |  pattern, start, end, indices, missed
     +--------------------+
```

---

## Fix Plan

### Phase 1: Critical Fixes (High Priority)
- [x] 1.1 Fix `extract_seq()` to use `&matching` instead of `&[target]`
- [x] 1.2 Add `.follow_links(false)` to `scan_files` walker

### Phase 2: Quality Improvements (Medium Priority)
- [x] 2.1 Add error logging to `scan_files` (warn! macro)
- [x] 2.2 Add rayon `par_iter` to `scan_files` for parallelism
- [x] 2.3 Add `scan_files` to Python bindings
- [x] 2.4 Add missing PySeq methods (first_file, last_file, expand_existing, range_count, frame_count, to_json, to_json_pretty)

### Phase 3: Polish (Low Priority)
- [x] 3.1 Refactor PySeq to avoid format_frame duplication
- [x] 3.2 Export IMAGE_EXTS and VIDEO_EXTS to Python

---

## Test Verification

After fixes, run:
```bash
cargo test --lib
cargo build --features python
maturin develop --features python
python -c "import scanseq; print(scanseq.VFX_IMAGE_EXTS)"
```

---

## Notes

- All 38 existing tests pass
- No backwards compatibility concerns (internal fixes)
- Python API additions are additive (no breaking changes)
- `#[allow(dead_code)]` annotations are intentional for public library API
