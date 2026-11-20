//! The `sqp-fmt` command: finding `sqp-fmt.toml`, `--config`,
//! `--stdin-filepath`, the user-level fallback configuration, the dialect
//! (`--dialect`, `[[dialect_paths]]`, `dialect`) and its `[dialects.<name>]`
//! section, configuration errors.

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
    // No user-level configuration unless a test sets one up.
    let none = std::env::temp_dir().join("sqp-fmt-cli-no-user-config");
    run_xdg(args, cwd, stdin, &none)
}

/// `run` with `XDG_CONFIG_HOME` set to `xdg`.
fn run_xdg(args: &[&str], cwd: &Path, stdin: Option<&str>, xdg: &Path) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_sqp-fmt"))
        .args(args)
        .current_dir(cwd)
        .env("XDG_CONFIG_HOME", xdg)
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

/// T-SQL with a `GO` batch separator: under `mssql` the `GO 2` line is kept
/// as written and the batches are formatted; under `generic` (no client
/// syntax) the file is one statement that doesn't parse, left as written.
const TSQL: &str = "select  a from t\nGO 2\nSELECT b FROM u\n";
const TSQL_AS_MSSQL: &str = "select a\nfrom t\nGO 2\nselect b\nfrom u\n";

#[test]
fn dialect_flag() {
    let t = Tree::new("dialect_flag");
    let q = t.file("q.sql", TSQL);
    let out = run(&["--dialect", "mssql", q.to_str().unwrap()], &t.0, None);
    assert_eq!(stdout(&out), TSQL_AS_MSSQL);
    let out = run(&["--dialect=tsql", q.to_str().unwrap()], &t.0, None);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown dialect \"tsql\""));
}

#[test]
fn user_config_is_the_fallback() {
    let t = Tree::new("user_config_is_the_fallback");
    let xdg = t.0.join("xdg");
    t.file("xdg/sqp-fmt/sqp-fmt.toml", "keyword_case = \"upper\"\n");
    t.file("proj/sqp-fmt.toml", "[where.groups]\nwrap = \"always\"\n");
    t.file("proj/q.sql", SQL);
    t.file("other/q.sql", SQL);
    // No project configuration: the user's.
    let out = run_xdg(&["other/q.sql"], &t.0, None, &xdg);
    assert_eq!(stdout(&out), UPPER);
    // A project configuration is the whole configuration (rustfmt's model).
    let out = run_xdg(&["proj/q.sql"], &t.0, None, &xdg);
    assert_eq!(stdout(&out), BLOCK);
    let out = run_xdg(
        &["--stdin-filepath", "other/new.sql"],
        &t.0,
        Some(SQL),
        &xdg,
    );
    assert_eq!(stdout(&out), UPPER);
}

#[test]
fn dialect_paths_map_files_to_dialects() {
    let t = Tree::new("dialect_paths_map_files_to_dialects");
    let xdg = t.0.join("xdg");
    let work = t.0.join("work");
    t.file(
        "xdg/sqp-fmt/sqp-fmt.toml",
        &format!(
            "dialect = \"postgres\"\n\
             [[dialect_paths]]\npath = \"{}\"\ndialect = \"mssql\"\n\
             [[dialect_paths]]\npath = \"{}/legacy/old.sql\"\ndialect = \"generic\"\n",
            work.display(),
            work.display()
        ),
    );
    t.file("work/repo/q.sql", TSQL);
    t.file("work/legacy/old.sql", TSQL);
    t.file("elsewhere/q.sql", TSQL);
    let out = run_xdg(&["work/repo/q.sql"], &t.0, None, &xdg);
    assert_eq!(stdout(&out), TSQL_AS_MSSQL);
    // The stdin buffer's path is mapped the same way.
    let out = run_xdg(
        &["--stdin-filepath", "work/repo/new.sql"],
        &t.0,
        Some(TSQL),
        &xdg,
    );
    assert_eq!(stdout(&out), TSQL_AS_MSSQL);
    // The most specific entry wins; `GO` is not client syntax there.
    let out = run_xdg(&["work/legacy/old.sql"], &t.0, None, &xdg);
    assert_eq!(stdout(&out), TSQL);
    // Unmapped: the configuration's `dialect`; `--dialect` wins over both.
    let out = run_xdg(&["elsewhere/q.sql"], &t.0, None, &xdg);
    assert_eq!(stdout(&out), TSQL);
    let out = run_xdg(&["--dialect", "mssql", "elsewhere/q.sql"], &t.0, None, &xdg);
    assert_eq!(stdout(&out), TSQL_AS_MSSQL);
    let out = run_xdg(
        &["--dialect", "generic", "work/repo/q.sql"],
        &t.0,
        None,
        &xdg,
    );
    assert_eq!(stdout(&out), TSQL);
}

#[test]
fn relative_dialect_paths_are_relative_to_the_config() {
    let t = Tree::new("relative_dialect_paths_are_relative_to_the_config");
    t.file(
        "proj/sqp-fmt.toml",
        "[[dialect_paths]]\npath = \"db\"\ndialect = \"mssql\"\n",
    );
    t.file("proj/db/q.sql", TSQL);
    let out = run(&["q.sql"], &t.0.join("proj/db"), None);
    assert_eq!(stdout(&out), TSQL_AS_MSSQL);
}

#[test]
fn dialect_sections_layer_over_the_general_style() {
    let t = Tree::new("dialect_sections_layer_over_the_general_style");
    t.file(
        "sqp-fmt.toml",
        "keyword_case = \"upper\"\n[where.groups]\nwrap = \"always\"\n\
         [dialects.mssql]\nkeyword_case = \"lower\"\n",
    );
    t.file("q.sql", SQL);
    // generic: the general style.
    let out = run(&["q.sql"], &t.0, None);
    assert_eq!(
        stdout(&out),
        "SELECT a, b\nFROM t\nWHERE (\n        x = 1\n        OR y = 2\n      )\n"
    );
    // mssql: its section's key replaces the general one; the rest inherited.
    let out = run(&["--dialect", "mssql", "q.sql"], &t.0, None);
    assert_eq!(stdout(&out), BLOCK);
}

#[test]
fn bad_dialect_section_points_at_the_key() {
    let t = Tree::new("bad_dialect_section_points_at_the_key");
    t.file(
        "sqp-fmt.toml",
        "[dialects.mssql.window]\nwarp = \"never\"\n",
    );
    let out = run(&[], &t.0, Some(SQL));
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("sqp-fmt.toml:2:1: unknown field `warp`"),
        "{err}"
    );
    t.file("sqp-fmt.toml", "[dialects.tsql]\nindent = 2\n");
    let out = run(&[], &t.0, Some(SQL));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("sqp-fmt.toml:1:1: unknown dialect \"tsql\""),
        "{err}"
    );
    t.file(
        "sqp-fmt.toml",
        "[[dialect_paths]]\npath = \"x\"\ndialect = \"tsql\"\n",
    );
    let out = run(&[], &t.0, Some(SQL));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("sqp-fmt.toml:3:11: unknown dialect \"tsql\""),
        "{err}"
    );
}
