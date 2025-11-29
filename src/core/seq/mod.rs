//! Sequence grouping: groups files by mask pattern, finds frame numbers, creates sequences.
//!
//! Algorithm (zero-copy where possible):
//! 1. Group files by sig_hash (drive + path + mask + ext) - O(n), moves ownership
//! 2. For each group, find which digit group is the frame number (max unique values)
//! 3. Sub-group by "anchor" values (all other digit groups) - moves ownership, no cloning
//! 4. Create Seq for each sub-group with >= 2 files

use super::file::File;
use serde::Serialize;
use std::collections::{HashMap, HashSet};

/// Maximum gap size to expand into missed frames list (OOM protection)
const MAX_MISSED_GAP: i64 = 100_000;

/// Sequence of numbered files
#[derive(Debug, Clone, Serialize)]
pub struct Seq {
    /// Frame numbers actually present
    pub indices: Vec<i64>,
    /// Missing frame numbers
    pub missed: Vec<i64>,
    /// First frame
    pub start: i64,
    /// Last frame
    pub end: i64,
    /// Padding (0 = variable/unpadded, >0 = fixed width)
    pub padding: usize,
    /// Pattern string (cached)
    pattern: String,
}

impl Seq {
    /// Create sequence from file list and frame group index.
    /// Returns None if no valid frames found.
    pub(crate) fn from_files(files: &[File], frame_grp_idx: usize) -> Option<Self> {
        if files.is_empty() {
            return None;
        }

        // Extract frame numbers from each file using its own num_groups positions
        // This handles unpadded sequences where positions vary per file
        let mut frames: Vec<i64> = files
            .iter()
            .filter_map(|f| {
                if frame_grp_idx >= f.num_groups.len() {
                    return None;
                }
                let (start, len) = f.num_groups[frame_grp_idx];
                // Bounds check to prevent panic on malformed data
                let end = start.saturating_add(len);
                if end > f.name.len() {
                    return None;
                }
                f.name[start..end].parse::<i64>().ok()
            })
            .collect();

        frames.sort_unstable();
        frames.dedup();

        if frames.is_empty() {
            return None;
        }

        // Safe extraction with pattern matching (no unwrap)
        let (start, end) = match (frames.first(), frames.last()) {
            (Some(&s), Some(&e)) => (s, e),
            _ => return None,
        };

        // Find missing frames with OOM protection
        let mut missed = Vec::new();
        for i in 0..frames.len().saturating_sub(1) {
            // Use saturating_sub to prevent i64 overflow on extreme values
            let gap = frames[i + 1].saturating_sub(frames[i]);
            if gap > 1 && gap <= MAX_MISSED_GAP {
                missed.extend((frames[i] + 1)..frames[i + 1]);
            }
            // Skip gaps larger than MAX_MISSED_GAP (don't enumerate millions of frames)
        }

        // Determine padding: 0 if variable length, otherwise fixed width
        let padding = detect_padding(files, frame_grp_idx);

        // Generate pattern using first file as template
        let pattern = gen_pattern(&files[0], frame_grp_idx, padding);

        Some(Seq { indices: frames, missed, start, end, padding, pattern })
    }

    /// Get sequence length (number of files)
    #[must_use]
    pub fn len(&self) -> usize {
        self.indices.len()
    }

    /// Check if sequence is empty (required by Clippy when len() exists)
    #[must_use]
    #[allow(dead_code)] // Public API, may be used by external crates
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    /// Get pattern with @ or ####
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// Group files into sequences.
    /// Uses mask-based grouping to handle unpadded sequences correctly.
    pub fn group_seqs(flist: &mut Vec<File>) -> Vec<Seq> {
        // Phase 1: Group by sig_hash (drive + path + mask + ext)
        // Files with same mask pattern (e.g., "render_@_img_@") go together
        let estimated_groups = (flist.len() / 10).max(16);
        let mut by_hash: HashMap<u64, Vec<File>> = HashMap::with_capacity(estimated_groups);

        for file in flist.drain(..) {
            if !file.has_nums() {
                continue; // Skip files without digit groups
            }
            by_hash.entry(file.sig_hash()).or_default().push(file);
        }

        let mut seqs = Vec::new();

        // Phase 2: Process each hash group
        for (_hash, files) in by_hash {
            if files.len() < 2 {
                continue; // Single file is not a sequence
            }

            // Find which digit group is the frame number (most unique values)
            let frame_grp_idx = find_frame_group(&files);

            // Sub-group by anchor values (moves files, no cloning)
            let sub_groups = sub_group_by_anchors(files, frame_grp_idx);

            // Create Seq for each sub-group with >= 2 files
            for sub_files in sub_groups.into_values() {
                if sub_files.len() >= 2 {
                    if let Some(seq) = Seq::from_files(&sub_files, frame_grp_idx) {
                        seqs.push(seq);
                    }
                }
            }
        }

        seqs
    }
}

/// Find which digit group is the frame number (has most unique values).
/// Tie-breaker: rightmost group (common convention: frame number is last).
fn find_frame_group(files: &[File]) -> usize {
    let num_groups = files.iter().map(|f| f.num_groups.len()).max().unwrap_or(0);
    if num_groups == 0 {
        return 0;
    }

    let mut best_idx = num_groups - 1; // Default: rightmost
    let mut best_count = 0;

    // Reuse HashSet to avoid allocations in loop
    let mut unique: HashSet<i64> = HashSet::with_capacity(files.len());
    for grp_idx in 0..num_groups {
        unique.clear();
        for f in files {
            if grp_idx < f.num_groups.len() {
                let (start, len) = f.num_groups[grp_idx];
                let end = start.saturating_add(len);
                if end <= f.name.len() {
                    if let Ok(val) = f.name[start..end].parse::<i64>() {
                        unique.insert(val);
                    }
                }
            }
        }
        // Prefer rightmost on tie (>= instead of >)
        if unique.len() >= best_count {
            best_count = unique.len();
            best_idx = grp_idx;
        }
    }

    best_idx
}

/// Sub-group files by anchor values (all digit groups except frame group).
/// Takes ownership to avoid cloning - files are moved into sub-groups.
fn sub_group_by_anchors(files: Vec<File>, frame_grp_idx: usize) -> HashMap<String, Vec<File>> {
    let mut groups: HashMap<String, Vec<File>> = HashMap::new();

    for file in files {
        let key = make_anchor_key(&file, frame_grp_idx);
        groups.entry(key).or_default().push(file); // Move, no clone
    }

    groups
}

/// Create anchor key from all digit groups except frame_grp_idx.
/// Example: "render_01_img_@.exr" with frame_grp_idx=1 -> anchor "01"
fn make_anchor_key(file: &File, frame_grp_idx: usize) -> String {
    let mut parts = Vec::new();
    for (idx, &(start, len)) in file.num_groups.iter().enumerate() {
        if idx != frame_grp_idx {
            let end = start.saturating_add(len);
            if end <= file.name.len() {
                parts.push(&file.name[start..end]);
            }
        }
    }
    parts.join("_")
}

/// Detect padding for frame group: 0 if variable, otherwise fixed width.
fn detect_padding(files: &[File], grp_idx: usize) -> usize {
    let mut lens: HashSet<usize> = HashSet::new();
    for f in files {
        if grp_idx < f.num_groups.len() {
            lens.insert(f.num_groups[grp_idx].1);
        }
    }
    // If all same length -> that's the padding; otherwise 0 (variable)
    // lens.len() == 1 guarantees next() returns Some
    if lens.len() == 1 { lens.into_iter().next().unwrap() } else { 0 }
}

impl std::fmt::Display for Seq {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.missed.is_empty() {
            write!(f, "Seq(\"{}\", range: {}-{})", self.pattern, self.start, self.end)
        } else {
            write!(f, "Seq(\"{}\", range: {}-{}, missed: {})", self.pattern, self.start, self.end, self.missed.len())
        }
    }
}

/// Generate pattern string from file template.
/// Uses mask for frame group, actual values for anchors.
/// padding=0 or 1 means unpadded (use @), padding>=2 means fixed width (use ####).
fn gen_pattern(file: &File, frame_grp_idx: usize, padding: usize) -> String {
    let mut result = String::with_capacity(file.name.len() + 10);
    let mut pos = 0;
    let name_len = file.name.len();

    for (idx, &(start, len)) in file.num_groups.iter().enumerate() {
        // Bounds check: ensure valid slice range
        let end = start.saturating_add(len);
        if start > name_len || end > name_len || pos > start {
            continue; // Skip malformed group
        }

        // Add text before this group
        result.push_str(&file.name[pos..start]);

        if idx == frame_grp_idx {
            // Frame group: use placeholder
            // @ for unpadded (padding <= 1), #### for padded (padding >= 2)
            if padding <= 1 {
                result.push('@');
            } else {
                result.push_str(&"#".repeat(padding));
            }
        } else {
            // Anchor group: keep actual value
            result.push_str(&file.name[start..end]);
        }
        pos = end;
    }

    // Add remaining text after last group
    if pos <= name_len {
        result.push_str(&file.name[pos..]);
    }

    // Build full path pattern
    format!("{}{}{}{}", file.drive, file.path.replace('\\', "/"), result, file.ext)
}

#[cfg(test)]
mod tests;
