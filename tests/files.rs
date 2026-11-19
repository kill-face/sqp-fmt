//! File-level behaviour: line endings, BOM, final newline, `GO` lines,
//! batches that can't be formatted.

fn fmt(s: &str) -> String {
    tsqlfmt::format(s).0
}

#[test]
fn crlf_is_kept() {
    let out = fmt("SELECT a,b\r\nGO\r\nselect 1\r\n");
    assert_eq!(out, "select a, b\r\nGO\r\nselect 1\r\n");
}

#[test]
fn bom_is_kept() {
    assert_eq!(fmt("\u{feff}SELECT 1\n"), "\u{feff}select 1\n");
}

#[test]
fn missing_final_newline_stays_missing() {
    assert_eq!(fmt("SELECT a"), "select a");
    assert_eq!(fmt("select 1\ngo"), "select 1\ngo");
}

#[test]
fn go_lines_are_kept_as_written() {
    assert_eq!(
        fmt("select 1\nGo 5\nselect 2\n  GO  -- end\n"),
        "select 1\nGo 5\nselect 2\n  GO  -- end\n"
    );
}

#[test]
fn mixed_line_endings_inside_comments_are_kept() {
    // The `\r` is inside a block comment, so it survives formatting.
    let src = "/* a\r\nb */\nSELECT 1\n";
    assert_eq!(fmt(src), "/* a\r\nb */\nselect 1\n");
}

#[test]
fn mixed_line_endings_in_code_leave_the_file_alone() {
    let src = "SELECT 1\r\nSELECT  2\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn unparsable_batch_is_verbatim_others_formatted() {
    let src = "SELECT  1\ngo\nselect ( from\ngo\nSELECT  2\n";
    let (out, stats) = tsqlfmt::format(src);
    assert_eq!(out, "select 1\ngo\nselect ( from\ngo\nselect 2\n");
    assert_eq!(stats.unparsed, 1);
    assert_eq!(stats.formatted, 2);
}

#[test]
fn whole_file_off_region() {
    let src = "-- @formatter:off\nSELECT   1\n";
    assert_eq!(fmt(src), src);
}

#[test]
fn off_region_spanning_go() {
    let src =
        "SELECT  1\n-- @formatter:off\nSELECT   2\ngo\nSELECT   3\n-- @formatter:on\nSELECT  4\n";
    assert_eq!(
        fmt(src),
        "select 1\n-- @formatter:off\nSELECT   2\ngo\nSELECT   3\n-- @formatter:on\nselect 4\n"
    );
}

#[test]
fn empty_and_blank_input() {
    assert_eq!(fmt(""), "");
    assert_eq!(fmt("\n"), "\n");
    assert_eq!(fmt("-- only a comment\n"), "-- only a comment\n");
}

#[test]
fn trailing_whitespace_inside_comments_and_strings_is_kept() {
    let src = "select 'a  \n  b' /* c  \n d */\n";
    assert_eq!(fmt(src), src);
}
