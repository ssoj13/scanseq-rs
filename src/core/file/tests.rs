use super::*;

#[test]
fn test_parse_fpn_windows() {
    let (drive, path, name, ext, groups) = parse_fpn("c:\\temp\\aaa\\project.test.data");
    assert_eq!(drive, "c:");
    assert_eq!(path, "\\temp\\aaa\\");
    assert_eq!(name, "project.test");
    assert_eq!(ext, ".data");
    assert!(groups.is_empty());
}

#[test]
fn test_parse_fpn_unix() {
    let (drive, path, name, ext, groups) = parse_fpn("/mnt/c/temp/aaa/project.test.data");
    assert_eq!(drive, "");
    assert_eq!(path, "/mnt/c/temp/aaa/");
    assert_eq!(name, "project.test");
    assert_eq!(ext, ".data");
    assert!(groups.is_empty());
}

#[test]
fn test_parse_fpn_unix_root() {
    let (drive, path, name, ext, _) = parse_fpn("/temp/aaa/project.test.data");
    assert_eq!(drive, "");
    assert_eq!(path, "/temp/aaa/");
    assert_eq!(name, "project.test");
    assert_eq!(ext, ".data");
}

#[test]
fn test_parse_fpn_forward_slash() {
    let (drive, path, name, ext, _) = parse_fpn("c:/temp/aaa/project.test.*");
    assert_eq!(drive, "c:");
    assert_eq!(path, "/temp/aaa/");
    assert_eq!(name, "project.test");
    assert_eq!(ext, ".*");
}

#[test]
fn test_parse_fpn_multi_slash() {
    let (drive, path, name, ext, _) = parse_fpn("c://temp////aaa//project.test.*");
    assert_eq!(drive, "c:");
    assert_eq!(path, "//temp////aaa//");
    assert_eq!(name, "project.test");
    assert_eq!(ext, ".*");
}

#[test]
fn test_parse_fpn_no_ext() {
    let (drive, path, name, ext, _) = parse_fpn("c:/temp/filename");
    assert_eq!(drive, "c:");
    assert_eq!(path, "/temp/");
    assert_eq!(name, "filename");
    assert_eq!(ext, "");
}

#[test]
fn test_parse_fpn_no_path() {
    let (drive, path, name, ext, _) = parse_fpn("filename.txt");
    assert_eq!(drive, "");
    assert_eq!(path, "");
    assert_eq!(name, "filename");
    assert_eq!(ext, ".txt");
}

#[test]
fn test_num_groups_multiple() {
    let groups = extract_num_groups("render_000_123_45");
    assert_eq!(groups, vec![(7, 3), (11, 3), (15, 2)]);
}

#[test]
fn test_num_groups_single() {
    let groups = extract_num_groups("file_001");
    assert_eq!(groups, vec![(5, 3)]);
}

#[test]
fn test_num_groups_none() {
    let groups = extract_num_groups("nodigits");
    assert!(groups.is_empty());
}

#[test]
fn test_file_new_with_nums() {
    let f = File::new("c:/temp/render_001.exr");
    assert_eq!(f.drive, "c:");
    assert_eq!(f.path, "/temp/");
    assert_eq!(f.name, "render_001");
    assert_eq!(f.ext, ".exr");
    assert_eq!(f.num_groups, vec![(7, 3)]);
    assert!(f.has_nums());
}

#[test]
fn test_file_new_no_nums() {
    let f = File::new("c:/temp/readme.txt");
    assert_eq!(f.drive, "c:");
    assert_eq!(f.path, "/temp/");
    assert_eq!(f.name, "readme");
    assert_eq!(f.ext, ".txt");
    assert!(f.num_groups.is_empty());
    assert!(!f.has_nums());
}

#[test]
fn test_file_display() {
    let f = File::new("c:/temp/render_001.exr");
    let display = format!("{}", f);
    assert!(display.contains("File("));
    assert!(display.contains("c:/temp/render_001.exr"));
    assert!(display.contains("mask=\"render_@\"")); // mask with @ placeholder
}

#[test]
fn test_rebuild_fpn() {
    // Verify all parts joined rebuild fpn intact
    let original = "c:\\temp\\aaa\\project.027.exr";
    let (drive, path, name, ext, _) = parse_fpn(original);
    let rebuilt = format!("{}{}{}{}", drive, path, name, ext);
    assert_eq!(rebuilt, original);
}

#[test]
fn test_rebuild_fpn_unix() {
    let original = "/mnt/c/temp/file_123.data";
    let (drive, path, name, ext, _) = parse_fpn(original);
    let rebuilt = format!("{}{}{}{}", drive, path, name, ext);
    assert_eq!(rebuilt, original);
}

#[test]
fn test_rebuild_fpn_multi_slash() {
    let original = "c://temp////file.txt";
    let (drive, path, name, ext, _) = parse_fpn(original);
    let rebuilt = format!("{}{}{}{}", drive, path, name, ext);
    assert_eq!(rebuilt, original);
}
