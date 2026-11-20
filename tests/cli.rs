//! The `sqp-fmt` command: finding `sqp-fmt.toml`, `--config`,
//! `--stdin-filepath`, configuration errors.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const SQL: &str = "select a, b from t where (x = 1 or y = 2)\n";
const DEFAULT: &str = "select a, b\nfrom t\nwhere (x = 1 or y = 2)\n";
/// `SQL` with `[where.groups] wrap = "always"`.
const BLOCK: &str = "select a, b\nfrom t\nwhere (\n        x = 1\n        or y = 2\n      )\n";
/// `SQL` with `keyword_case = "upper"`.
const UPPER: &str = "SELECT a, b\nFROM t\nWHERE (x = 1 OR y = 2)\n";

/// A scratch directory tree, removed when dropped.
struct Tree(PathBuf);

impl Tree {
    fn new(name: &str) -> Tree {
        let root = std::env::temp_dir().join(format!("sqp-fmt-cli-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        Tree(root)
    }

    fn file(&self, rel: &str, text: &str) -> PathBuf {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, text).unwrap();
        p
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(args: &[&str], cwd: &Path, stdin: Option<&str>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_sqp-fmt"))
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    input.write_all(stdin.unwrap_or("").as_bytes()).unwrap();
    drop(input);
    child.wait_with_output().unwrap()
}

fn stdout(o: &Output) -> String {
    assert!(
        o.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8(o.stdout.clone()).unwrap()
}

fn tree_with_config(name: &str) -> Tree {
    let t = Tree::new(name);
    t.file("proj/sqp-fmt.toml", "[where.groups]\nwrap = \"always\"\n");
    t.file("proj/sub/deeper/q.sql", SQL);
    t.file("other/q.sql", SQL);
    t
}

#[test]
fn file_finds_the_config_above_it() {
    let t = tree_with_config("file_finds_the_config_above_it");
    let out = run(&["proj/sub/deeper/q.sql"], &t.0, None);
    assert_eq!(stdout(&out), BLOCK);
    // Absolute path, run from somewhere else.
    let abs = t.0.join("proj/sub/deeper/q.sql");
    let out = run(&[abs.to_str().unwrap()], &std::env::temp_dir(), None);
    assert_eq!(stdout(&out), BLOCK);
}

#[test]
fn nearest_config_wins() {
    let t = tree_with_config("nearest_config_wins");
    t.file("proj/sub/sqp-fmt.toml", "keyword_case = \"upper\"\n");
    let out = run(&["proj/sub/deeper/q.sql"], &t.0, None);
    // The nearer file is the whole configuration: no merging.
    assert_eq!(stdout(&out), UPPER);
}

#[test]
fn file_without_config_uses_defaults() {
    let t = tree_with_config("file_without_config_uses_defaults");
    let out = run(&["other/q.sql"], &t.0, None);
    assert_eq!(stdout(&out), DEFAULT);
}

#[test]
fn stdin_uses_stdin_filepath() {
    let t = tree_with_config("stdin_uses_stdin_filepath");
    // The path needn't exist; only its directory is searched.
    let out = run(&["--stdin-filepath", "proj/sub/new.sql"], &t.0, Some(SQL));
    assert_eq!(stdout(&out), BLOCK);
    let out = run(&["--stdin-filepath=other/x.sql", "-"], &t.0, Some(SQL));
    assert_eq!(stdout(&out), DEFAULT);
}

#[test]
fn stdin_searches_from_cwd() {
    let t = tree_with_config("stdin_searches_from_cwd");
    let out = run(&[], &t.0.join("proj/sub"), Some(SQL));
    assert_eq!(stdout(&out), BLOCK);
    let out = run(&["-"], &t.0.join("other"), Some(SQL));
    assert_eq!(stdout(&out), DEFAULT);
}

#[test]
fn config_flag_overrides_the_search() {
    let t = tree_with_config("config_flag_overrides_the_search");
    let c = t.file("elsewhere/style.toml", "keyword_case = \"upper\"\n");
    let out = run(
        &["--config", c.to_str().unwrap(), "proj/sub/deeper/q.sql"],
        &t.0,
        None,
    );
    assert_eq!(stdout(&out), UPPER);
    let out = run(
        &[
            &format!("--config={}", c.display()),
            "--stdin-filepath",
            "proj/q.sql",
        ],
        &t.0,
        Some(SQL),
    );
    assert_eq!(stdout(&out), UPPER);
}

#[test]
fn bad_config_is_an_error_with_no_output() {
    let t = Tree::new("bad");
    t.file(
        "sqp-fmt.toml",
        "margin = 100\n[window]\nwrap = \"sometimes\"\n",
    );
    let q = t.file("q.sql", SQL);
    let out = run(&[q.to_str().unwrap()], &t.0, None);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("sqp-fmt.toml:3:8: unknown variant `sometimes`"),
        "{err}"
    );

    t.file("sqp-fmt.toml", "[exists]\nwrap = \"never\"\n");
    let out = run(&[], &t.0, Some(SQL));
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains(
            "sqp-fmt.toml:2:8: exists.wrap = \"never\" is not implemented yet for exists.wrap"
        ),
        "{err}"
    );

    let out = run(&["--config", "missing.toml"], &t.0, Some(SQL));
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("missing.toml"));
}

#[test]
fn stdin_filepath_needs_stdin() {
    let t = tree_with_config("stdin_filepath_needs_stdin");
    let out = run(
        &["--stdin-filepath", "proj/q.sql", "other/q.sql"],
        &t.0,
        None,
    );
    assert_eq!(out.status.code(), Some(2));
}
