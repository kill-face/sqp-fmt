//! Golden tests: every `tests/golden/NAME.sql` is formatted and compared
//! with `tests/golden/NAME.expected.sql`. Set `TSQLFMT_BLESS=1` to write the
//! expected files from the current output (then review the diff). A
//! `tests/golden/NAME.toml` next to the input is that test's configuration
//! (else the defaults).
//!
//! Property checks over every input and expected file:
//! - the output has the same tokens as the input (comments included; case
//!   only differs for unquoted words), checked independently of the
//!   formatter's own safety net, and the safety net never fell back;
//! - formatting is idempotent: `fmt(fmt(x)) == fmt(x)`, and expected files
//!   are fixed points.

use std::path::{Path, PathBuf};

use sqlparser::dialect::MsSqlDialect;
use sqlparser::tokenizer::{Token, Tokenizer, Whitespace};

fn cases() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            let name = p.file_name().unwrap().to_str().unwrap();
            name.ends_with(".sql") && !name.ends_with(".expected.sql")
        })
        .collect();
    v.sort();
    assert!(!v.is_empty(), "no golden tests in {}", dir.display());
    v
}

fn expected_path(input: &Path) -> PathBuf {
    input.with_extension("expected.sql")
}

/// The test's configuration: `NAME.toml`, or the defaults.
fn config(input: &Path) -> tsqlfmt::Config {
    let path = input.with_extension("toml");
    if path.exists() {
        tsqlfmt::Config::load(&path).unwrap_or_else(|e| panic!("{e}"))
    } else {
        tsqlfmt::Config::default()
    }
}

/// Tokens without whitespace; unquoted words lower-cased.
fn tokens(sql: &str) -> Vec<String> {
    Tokenizer::new(&MsSqlDialect {}, sql)
        .tokenize()
        .unwrap()
        .into_iter()
        .filter(|t| {
            !matches!(
                t,
                Token::Whitespace(Whitespace::Space | Whitespace::Tab | Whitespace::Newline)
                    | Token::EOF
            )
        })
        .map(|t| match &t {
            Token::Word(w) if w.quote_style.is_none() => w.value.to_ascii_lowercase(),
            Token::Whitespace(Whitespace::SingleLineComment { comment, prefix }) => {
                format!("{prefix}{}", comment.trim_end_matches('\r'))
            }
            t => t.to_string(),
        })
        .collect()
}

fn diff(a: &str, b: &str) -> String {
    let mut s = String::new();
    let (al, bl): (Vec<_>, Vec<_>) = (a.lines().collect(), b.lines().collect());
    for i in 0..al.len().max(bl.len()) {
        let (x, y) = (al.get(i).copied(), bl.get(i).copied());
        if x != y {
            s.push_str(&format!(
                "line {}:\n  expected: {:?}\n  got:      {:?}\n",
                i + 1,
                x,
                y
            ));
        }
    }
    s
}

#[test]
fn golden() {
    let bless = std::env::var_os("TSQLFMT_BLESS").is_some();
    let mut failures = Vec::new();
    for input in cases() {
        let src = std::fs::read_to_string(&input).unwrap();
        let cfg = config(&input);
        let (out, stats) = tsqlfmt::format_with(&src, &cfg);
        let exp_path = expected_path(&input);
        if bless {
            std::fs::write(&exp_path, &out).unwrap();
        }
        let expected = std::fs::read_to_string(&exp_path).unwrap_or_default();
        let name = input.file_name().unwrap().to_string_lossy().to_string();
        if out != expected {
            failures.push(format!("{name}: output differs\n{}", diff(&expected, &out)));
        }
        if stats.mismatches + stats.errors > 0 {
            failures.push(format!(
                "{name}: safety net fell back: {:?}",
                stats.problems
            ));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn tokens_preserved() {
    for input in cases() {
        let cfg = config(&input);
        for path in [input.clone(), expected_path(&input)] {
            let Ok(src) = std::fs::read_to_string(&path) else {
                continue;
            };
            let (out, _) = tsqlfmt::format_with(&src, &cfg);
            assert_eq!(
                tokens(&src),
                tokens(&out),
                "{}: tokens changed",
                path.display()
            );
        }
    }
}

#[test]
fn idempotent() {
    for input in cases() {
        let src = std::fs::read_to_string(&input).unwrap();
        let cfg = config(&input);
        let (once, _) = tsqlfmt::format_with(&src, &cfg);
        let (twice, _) = tsqlfmt::format_with(&once, &cfg);
        assert!(
            once == twice,
            "{}: not idempotent\n{}",
            input.display(),
            diff(&once, &twice)
        );
        if let Ok(expected) = std::fs::read_to_string(expected_path(&input)) {
            let (again, _) = tsqlfmt::format_with(&expected, &cfg);
            assert!(
                again == expected,
                "{}: expected output isn't a fixed point\n{}",
                input.display(),
                diff(&expected, &again)
            );
        }
    }
}
