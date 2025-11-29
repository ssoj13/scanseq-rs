//! File parsing module: extracts path components and digit groups from file paths.
//!
//! This module is used by [`crate::core::seq`] for sequence detection.
//! It parses paths like `c:/renders/shot_01_frame_0001.exr` into:
//! - drive: `c:`
//! - path: `/renders/`
//! - name: `shot_01_frame_0001`
//! - ext: `.exr`
//! - num_groups: `[(5, 2), (14, 4)]` - positions and lengths of digit groups
//! - mask: `shot_@_frame_@` - name with digits replaced by `@`
//!
//! The mask is key for grouping: files with the same mask belong to the same
//! sequence family, even if they have different padding (e.g., `img_1` and `img_100`).

use std::path::PathBuf;

/// Parsed file with path components and digit group metadata.
///
/// Created via [`File::new()`], which parses any path string or PathBuf.
/// The `mask` field enables O(n) grouping of files into sequences.
#[derive(Debug, Clone)]
pub struct File {
    /// Original full path (for output and debugging)
    pub fpn: PathBuf,
    /// Drive letter with colon (e.g., "c:") or empty for Unix paths
    pub drive: String,
    /// Directory path including trailing slash (preserves original slashes)
    pub path: String,
    /// Filename without extension (e.g., "render_001")
    pub name: String,
    /// Extension with leading dot (e.g., ".exr") or empty if none
    pub ext: String,
    /// Digit groups in name: Vec<(start_pos, length)>
    /// Example: "shot_01_frame_0001" → [(5, 2), (14, 4)]
    pub num_groups: Vec<(usize, usize)>,
    /// Name with all digit groups replaced by @ (e.g., "shot_@_frame_@")
    /// Used for hash-based grouping - files with same mask are candidates for same sequence
    pub mask: String,
}

impl File {
    /// Parse a file path into components.
    ///
    /// Accepts any path format (Windows `c:\...`, Unix `/...`, mixed slashes).
    /// Extracts digit groups and creates mask for sequence grouping.
    /// On Windows, drive/path/ext/mask are lowercased for case-insensitive grouping.
    pub fn new<P: Into<PathBuf>>(path: P) -> Self {
        let fpn = path.into();
        let path_str = fpn.to_string_lossy().to_string();

        let (drive, path, name, ext, num_groups) = parse_fpn(&path_str);
        let mask = make_mask(&name, &num_groups);

        // On Windows, normalize case for grouping (paths are case-insensitive)
        #[cfg(windows)]
        let (drive, path, ext, mask) = (
            drive.to_lowercase(),
            path.to_lowercase(),
            ext.to_lowercase(),
            mask.to_lowercase(),
        );

        Self { fpn, drive, path, name, ext, num_groups, mask }
    }

    /// Compute signature hash for grouping files into sequence candidates.
    ///
    /// Hash includes: drive + path + mask + ext.
    /// Files with same hash have same mask pattern and are candidates for same sequence.
    pub(crate) fn sig_hash(&self) -> u64 {
        signature_hash(self)
    }

    /// Returns true if filename contains digit groups (potential sequence member)
    pub fn has_nums(&self) -> bool {
        !self.num_groups.is_empty()
    }
}

impl std::fmt::Display for File {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "File(\"{}\", mask=\"{}\")", self.fpn.display(), self.mask)
    }
}

/// Parse full path into components: (drive, path, name, ext, num_groups).
///
/// Handles Windows (`c:\temp\file.exr`), Unix (`/tmp/file.exr`), and mixed formats.
/// Drive is empty for Unix paths. Path includes trailing slash.
///
/// # Examples
/// ```ignore
/// parse_fpn("c:/renders/shot_001.exr")
/// // → ("c:", "/renders/", "shot_001", ".exr", [(5, 3)])
/// ```
pub fn parse_fpn(fpn: &str) -> (String, String, String, String, Vec<(usize, usize)>) {
    // Find drive: everything before first slash (\\ or / or any number of them)
    let first_slash = fpn.find(['\\', '/']);

    let (drive, after_drive) = match first_slash {
        Some(pos) => (fpn[..pos].to_string(), &fpn[pos..]),
        None => {
            // No slashes - entire thing is filename, parse it
            let last_dot = fpn.rfind('.');
            let (name, ext) = match last_dot {
                Some(pos) if pos > 0 => (&fpn[..pos], &fpn[pos..]),
                _ => (fpn, ""),
            };
            let num_groups = extract_num_groups(name);
            return (String::new(), String::new(), name.to_string(), ext.to_string(), num_groups);
        }
    };

    // Find last slash in after_drive - everything after is filename
    let last_slash = after_drive.rfind(['\\', '/']);

    let (path_part, filename) = match last_slash {
        Some(pos) => (&after_drive[..=pos], &after_drive[pos + 1..]),
        None => ("", after_drive),
    };

    // Parse filename: find last dot
    let last_dot = filename.rfind('.');
    let (name, ext) = match last_dot {
        Some(pos) if pos > 0 => (&filename[..pos], &filename[pos..]),
        _ => (filename, ""),
    };

    // Extract digit groups from name
    let num_groups = extract_num_groups(name);

    (drive, path_part.to_string(), name.to_string(), ext.to_string(), num_groups)
}

/// Extract positions of contiguous digit groups from filename.
///
/// Returns Vec<(start, len)> for each group. Used for frame number extraction.
/// Example: "shot_01_frame_0001" → [(5, 2), (14, 4)]
fn extract_num_groups(name: &str) -> Vec<(usize, usize)> {
    let mut groups = Vec::new();
    let mut in_digit = false;
    let mut start = 0;

    for (pos, ch) in name.char_indices() {
        if ch.is_ascii_digit() {
            if !in_digit {
                start = pos;
                in_digit = true;
            }
        } else if in_digit {
            groups.push((start, pos - start));
            in_digit = false;
        }
    }

    // Handle trailing digits
    if in_digit {
        groups.push((start, name.len() - start));
    }

    groups
}

/// Create mask by replacing all digit groups with `@` placeholder.
///
/// This enables grouping files with different padding into same sequence.
/// Example: "render_001" and "render_1" both become "render_@"
fn make_mask(name: &str, groups: &[(usize, usize)]) -> String {
    if groups.is_empty() {
        return name.to_string();
    }
    let mut result = String::with_capacity(name.len());
    let mut pos = 0;
    for &(start, len) in groups {
        result.push_str(&name[pos..start]);
        result.push('@');
        pos = start + len;
    }
    result.push_str(&name[pos..]);
    result
}

/// Calculate signature hash for sequence grouping.
///
/// Hashes: drive + path + mask + ext.
/// Files with identical hash are candidates for the same sequence family.
/// The mask ensures files with different padding (img_1 vs img_001) get same hash.
/// Note: On Windows, these fields are already lowercased in File::new().
pub(crate) fn signature_hash(file: &File) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    // Fields are pre-normalized in File::new() on Windows
    file.drive.hash(&mut hasher);
    file.path.hash(&mut hasher);
    file.mask.hash(&mut hasher);
    file.ext.hash(&mut hasher);
    hasher.finish()
}

#[cfg(test)]
mod tests;
