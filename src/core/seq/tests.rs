use super::*;

#[test]
fn test_seq_pattern_padded() {
    let files = vec![File::new("c:/temp/render_001.exr"), File::new("c:/temp/render_002.exr")];

    let seq = Seq::from_files(&files, 0).expect("should create sequence");
    assert_eq!(seq.pattern(), "c:/temp/render_###.exr");
    assert_eq!(seq.start, 1);
    assert_eq!(seq.end, 2);
    assert_eq!(seq.padding, 3);
    assert_eq!(seq.len(), 2);
}

#[test]
fn test_seq_pattern_unpadded() {
    let files = vec![File::new("c:/temp/file_1.exr"), File::new("c:/temp/file_2.exr")];

    let seq = Seq::from_files(&files, 0).expect("should create sequence");
    assert_eq!(seq.pattern(), "c:/temp/file_@.exr");
    assert_eq!(seq.start, 1);
    assert_eq!(seq.end, 2);
    assert_eq!(seq.padding, 1);
}

#[test]
fn test_seq_missed_frames() {
    let files = vec![
        File::new("c:/temp/aaa_001.exr"),
        File::new("c:/temp/aaa_002.exr"),
        File::new("c:/temp/aaa_005.exr"),
    ];

    let seq = Seq::from_files(&files, 0).expect("should create sequence");
    assert_eq!(seq.start, 1);
    assert_eq!(seq.end, 5);
    assert_eq!(seq.missed, vec![3, 4]);
    assert_eq!(seq.len(), 3);
}

#[test]
fn test_seq_display_no_missed() {
    let files = vec![File::new("c:/temp/render_01.exr"), File::new("c:/temp/render_02.exr")];

    let seq = Seq::from_files(&files, 0).expect("should create sequence");
    let display = format!("{}", seq);
    assert_eq!(display, "Seq(\"c:/temp/render_##.exr\", range: 1-2)");
}

#[test]
fn test_seq_display_with_missed() {
    let files = vec![File::new("c:/temp/a_1.exr"), File::new("c:/temp/a_5.exr")];

    let seq = Seq::from_files(&files, 0).expect("should create sequence");
    let display = format!("{}", seq);
    assert_eq!(display, "Seq(\"c:/temp/a_@.exr\", range: 1-5, missed: 3)");
}

#[test]
fn test_group_seqs_basic() {
    let mut files = vec![
        File::new("c:/temp/render_001.exr"),
        File::new("c:/temp/render_002.exr"),
        File::new("c:/temp/render_003.exr"),
    ];

    let seqs = Seq::group_seqs(&mut files);
    assert_eq!(seqs.len(), 1);
    assert_eq!(seqs[0].pattern(), "c:/temp/render_###.exr");
    assert_eq!(seqs[0].len(), 3);
    assert!(files.is_empty());
}

#[test]
fn test_group_seqs_discard_no_nums() {
    let mut files = vec![
        File::new("c:/temp/readme.txt"),
        File::new("c:/temp/render_001.exr"),
        File::new("c:/temp/render_002.exr"),
    ];

    let seqs = Seq::group_seqs(&mut files);
    assert_eq!(seqs.len(), 1);
    assert_eq!(seqs[0].pattern(), "c:/temp/render_###.exr");
    assert!(files.is_empty()); // All consumed, readme discarded
}

#[test]
fn test_group_seqs_min_two_files() {
    let mut files = vec![File::new("c:/temp/single_001.exr")];

    let seqs = Seq::group_seqs(&mut files);
    assert!(seqs.is_empty()); // Single file doesn't make a sequence
    assert!(files.is_empty()); // Consumed anyway
}

#[test]
fn test_group_seqs_multiple_groups() {
    let mut files = vec![
        File::new("c:/temp/shot_01_001.exr"),
        File::new("c:/temp/shot_01_002.exr"),
        File::new("c:/temp/shot_02_001.exr"),
        File::new("c:/temp/shot_02_002.exr"),
    ];

    let seqs = Seq::group_seqs(&mut files);
    // Should create 2 sequences: shot_01_### and shot_02_###
    assert_eq!(seqs.len(), 2);
}

#[test]
fn test_group_seqs_unpadded() {
    // Test that unpadded sequences (1, 2, ..., 10, 11) are grouped correctly
    let mut files = vec![
        File::new("c:/temp/img_1.exr"),
        File::new("c:/temp/img_2.exr"),
        File::new("c:/temp/img_9.exr"),
        File::new("c:/temp/img_10.exr"),
        File::new("c:/temp/img_11.exr"),
        File::new("c:/temp/img_100.exr"),
    ];

    let seqs = Seq::group_seqs(&mut files);
    // All files should be in ONE sequence because mask is "img_@" for all
    assert_eq!(seqs.len(), 1);
    assert_eq!(seqs[0].len(), 6);
    assert_eq!(seqs[0].start, 1);
    assert_eq!(seqs[0].end, 100);
    assert_eq!(seqs[0].padding, 0); // Variable padding = 0
    assert_eq!(seqs[0].pattern(), "c:/temp/img_@.exr");
}

#[test]
fn test_group_seqs_mixed_anchors() {
    // Test multi-group files with different anchors
    let mut files = vec![
        File::new("c:/temp/render_01_img_001.exr"),
        File::new("c:/temp/render_01_img_002.exr"),
        File::new("c:/temp/render_02_img_001.exr"),
        File::new("c:/temp/render_02_img_002.exr"),
    ];

    let seqs = Seq::group_seqs(&mut files);
    // Should create 2 sequences: render_01_img_### and render_02_img_###
    assert_eq!(seqs.len(), 2);

    // Check patterns contain anchor values
    let patterns: Vec<&str> = seqs.iter().map(|s| s.pattern()).collect();
    assert!(patterns.iter().any(|p| p.contains("render_01_img_")));
    assert!(patterns.iter().any(|p| p.contains("render_02_img_")));
}

// --- Edge case tests ---

#[test]
fn test_from_files_empty_list() {
    let files: Vec<File> = vec![];
    let seq = Seq::from_files(&files, 0);
    assert!(seq.is_none(), "Empty file list should return None");
}

#[test]
fn test_from_files_invalid_frame_group_idx() {
    // frame_grp_idx beyond actual num_groups
    let files = vec![File::new("c:/temp/render_001.exr")];
    let seq = Seq::from_files(&files, 99);
    assert!(seq.is_none(), "Invalid frame_grp_idx should return None");
}

#[test]
fn test_seq_large_gap_no_oom() {
    // Test that large gaps don't cause OOM (MAX_MISSED_GAP protection)
    let files = vec![
        File::new("c:/temp/a_0001.exr"),
        File::new("c:/temp/a_9999999.exr"),
    ];

    let seq = Seq::from_files(&files, 0).expect("should create sequence");
    // Gap is ~10M frames, should NOT enumerate all missed frames
    assert_eq!(seq.start, 1);
    assert_eq!(seq.end, 9999999);
    assert!(seq.missed.is_empty(), "Large gaps should skip missed enumeration");
}

#[test]
fn test_seq_is_empty() {
    let files = vec![File::new("c:/temp/x_01.exr"), File::new("c:/temp/x_02.exr")];
    let seq = Seq::from_files(&files, 0).expect("should create sequence");
    assert!(!seq.is_empty());
}

#[test]
fn test_group_seqs_empty_input() {
    let mut files: Vec<File> = vec![];
    let seqs = Seq::group_seqs(&mut files);
    assert!(seqs.is_empty());
}

#[test]
fn test_group_seqs_all_without_nums() {
    let mut files = vec![
        File::new("c:/temp/readme.txt"),
        File::new("c:/temp/config.yaml"),
    ];
    let seqs = Seq::group_seqs(&mut files);
    assert!(seqs.is_empty(), "Files without numbers should produce no sequences");
}

// --- PathBuf / ergonomic API tests ---

#[test]
fn test_frames_iter_ascending_pairs() {
    use std::path::PathBuf;
    // Frames 1,2,5 (gap at 3,4) -> frames() must yield only PRESENT frames, ascending.
    let files = vec![
        File::new("c:/temp/aaa_001.exr"),
        File::new("c:/temp/aaa_002.exr"),
        File::new("c:/temp/aaa_005.exr"),
    ];
    let seq = Seq::from_files(&files, 0).expect("should create sequence");

    let pairs: Vec<(i64, PathBuf)> = seq.frames().collect();
    // Numbers match indices exactly (ascending, present-only).
    let nums: Vec<i64> = pairs.iter().map(|(n, _)| *n).collect();
    assert_eq!(nums, seq.indices);
    assert_eq!(nums, vec![1, 2, 5]);
    // Paths match the formatted, present-frame paths.
    let paths: Vec<PathBuf> = pairs.iter().map(|(_, p)| p.clone()).collect();
    assert_eq!(paths, seq.paths());
    assert_eq!(paths[0], PathBuf::from("c:/temp/aaa_001.exr"));
    assert_eq!(paths[2], PathBuf::from("c:/temp/aaa_005.exr"));
}

#[test]
fn test_get_path_present_and_missing() {
    use std::path::PathBuf;
    let files = vec![
        File::new("c:/temp/aaa_001.exr"),
        File::new("c:/temp/aaa_002.exr"),
        File::new("c:/temp/aaa_005.exr"),
    ];
    let seq = Seq::from_files(&files, 0).expect("should create sequence");

    assert_eq!(seq.get_path(2), Some(PathBuf::from("c:/temp/aaa_002.exr")));
    assert_eq!(seq.get_path(3), None, "missing frame must be None");
    assert_eq!(seq.get_path(999), None, "out-of-range frame must be None");
}

#[test]
fn test_first_last_path() {
    use std::path::PathBuf;
    let files = vec![
        File::new("c:/temp/aaa_001.exr"),
        File::new("c:/temp/aaa_005.exr"),
    ];
    let seq = Seq::from_files(&files, 0).expect("should create sequence");
    // first_path mirrors first_file (real stored path), last_path mirrors last_file (formatted).
    assert_eq!(seq.first_path(), PathBuf::from(seq.first_file()));
    assert_eq!(seq.last_path(), PathBuf::from(seq.last_file()));
}

// --- Subset selection (Seq::select / select_indices) ---

/// Build a padded 1..=10 sequence for selection tests (frames 1,2,...,10).
fn ten_frame_seq() -> Seq {
    let files: Vec<File> = (1..=10)
        .map(|n| File::new(format!("c:/temp/f_{n:04}.exr")))
        .collect();
    Seq::from_files(&files, 0).expect("should create sequence")
}

#[test]
fn test_select_every_third() {
    // `::3` over 10 frames -> positions 0,3,6,9 -> frame numbers 1,4,7,10.
    let seq = ten_frame_seq();
    let sub = seq.select(&"::3".parse().expect("slice"));
    assert_eq!(sub.indices, vec![1, 4, 7, 10]);
    assert_eq!(sub.start, 1);
    assert_eq!(sub.end, 10);
    // Pattern/padding carry over unchanged.
    assert_eq!(sub.pattern(), seq.pattern());
    assert_eq!(sub.padding, seq.padding);
    // Dropped frames between kept ones become "missed".
    assert_eq!(sub.missed, vec![2, 3, 5, 6, 8, 9]);
}

#[test]
fn test_select_from_index_preserves_first_file_reformat() {
    // `5:` -> positions 5..10 -> frames 6..=10. First frame changed, so first_file
    // is re-derived from the pattern for frame 6.
    let seq = ten_frame_seq();
    let sub = seq.select(&"5:".parse().expect("slice"));
    assert_eq!(sub.indices, vec![6, 7, 8, 9, 10]);
    assert_eq!(sub.start, 6);
    assert_eq!(sub.first_file(), "c:/temp/f_0006.exr");
}

#[test]
fn test_select_keeps_first_file_when_head_retained() {
    // Head retained -> original-case first_file_path preserved verbatim.
    let seq = ten_frame_seq();
    let sub = seq.select(&":4".parse().expect("slice"));
    assert_eq!(sub.indices, vec![1, 2, 3, 4]);
    assert_eq!(sub.first_file(), seq.first_file());
}

#[test]
fn test_select_empty_selection() {
    // start beyond len -> 0 frames; Seq is empty, not a panic.
    let seq = ten_frame_seq();
    let sub = seq.select(&"50:".parse().expect("slice"));
    assert!(sub.is_empty());
    assert_eq!(sub.len(), 0);
}

#[test]
fn test_select_indices_dedup_and_sort() {
    // Out-of-order, duplicate, and out-of-range positions are normalized.
    let seq = ten_frame_seq();
    let sub = seq.select_indices(&[9, 0, 0, 3, 999]);
    assert_eq!(sub.indices, vec![1, 4, 10]);
}

#[test]
fn test_select_last_fifty_semantics_small() {
    // `-3:` over 10 frames -> last 3 -> frames 8,9,10.
    let seq = ten_frame_seq();
    let sub = seq.select(&"-3:".parse().expect("slice"));
    assert_eq!(sub.indices, vec![8, 9, 10]);
}
