//! SQL formatter built on (a fork of) sqlparser-rs, for every dialect it
//! parses. It only changes whitespace and keyword case.
//!
//! A file is read the way its dialect's command-line client reads it (see
//! `script`): client commands and data are kept as written, and the SQL
//! between them is formatted one batch at a time. Lexing is sqlparser's
//! tokenizer for the dialect throughout.

pub mod build;
pub mod case;
pub mod config;
pub mod format;
pub mod layout;
pub mod script;
pub mod spans;

pub use config::Config;
pub use format::{Stats, format, format_with};

use script::{Client, Lexed, PartKind};

/// A batch: SQL the client sends between two client commands (a sqlcmd
/// `GO` batch; for the other clients, the statements between commands).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Batch {
    /// 1-based line of the batch's first line in the file.
    pub start_line: usize,
    /// The batch text, prefixed with `start_line - 1` newlines, so that
    /// token locations and parser errors are the file's.
    pub text: String,
    /// The mysql client's statement delimiter in effect (`;` otherwise).
    pub delimiter: String,
}

/// Splits a file into its batches, for `cfg`'s dialect. Batches of blank
/// lines only are left out.
pub fn split_batches(src: &str, cfg: &Config) -> Vec<Batch> {
    let text = src
        .strip_prefix('\u{feff}')
        .unwrap_or(src)
        .replace("\r\n", "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    let n = lines.len() - usize::from(text.ends_with('\n'));
    script::split(&text, n, cfg)
        .parts
        .into_iter()
        .filter_map(|p| {
            let PartKind::Sql { delimiter } = p.kind else {
                return None;
            };
            let body = &lines[p.lines.clone()];
            if body.iter().all(|l| l.trim().is_empty()) {
                return None;
            }
            let mut text = "\n".repeat(p.lines.start);
            for l in body {
                text.push_str(l);
                text.push('\n');
            }
            Some(Batch {
                start_line: p.lines.start + 1,
                text,
                delimiter,
            })
        })
        .collect()
}

/// A batch's tokens, as its client and server see them (see `script::lex`).
pub fn lex_batch(batch: &Batch, cfg: &Config) -> Lexed {
    script::lex(
        &batch.text,
        cfg.dialect().as_ref(),
        Client::of(cfg.dialect_name()),
        &batch.delimiter,
    )
}

/// Parses a batch the way the formatter does (see `format::parse_batch`):
/// its statements, and the errors of the statements that don't parse
/// (file line, 1-based; message). A batch that doesn't tokenize is one
/// error.
pub fn parse_batch(
    batch: &Batch,
    cfg: &Config,
) -> (Vec<sqlparser::ast::Statement>, Vec<(usize, String)>) {
    let lx = lex_batch(batch, cfg);
    if let Some(e) = &lx.error {
        return (Vec::new(), vec![(e.location.line as usize, e.to_string())]);
    }
    let nlines = batch.text.split('\n').count();
    let dialect = cfg.dialect();
    let client = Client::of(cfg.dialect_name());
    match format::parse_batch(&lx, nlines, dialect.as_ref(), client, &batch.delimiter) {
        Ok(p) => (
            p.stmts,
            p.unparsed.into_iter().map(|(l, m)| (l + 1, m)).collect(),
        ),
        Err(format::BatchError::Unparsed(errors)) => (
            Vec::new(),
            errors.into_iter().map(|(l, m)| (l + 1, m)).collect(),
        ),
        Err(e) => (Vec::new(), vec![(batch.start_line, format!("{e:?}"))]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mssql() -> Config {
        Config::default().for_dialect("mssql")
    }

    #[test]
    fn batches_keep_line_numbers() {
        let b = split_batches("select 1\ngo\n\nselect 2\n:r other.sql\ngo\n", &mssql());
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].start_line, 1);
        assert_eq!(b[1].start_line, 3);
        assert_eq!(b[1].text, "\n\n\nselect 2\n");
    }

    #[test]
    fn statements_without_semicolons() {
        let b = split_batches(
            "if 1 = 1\nbegin\n    declare @a int\n    set @a = 1\nend\nselect 1\n",
            &mssql(),
        );
        let (stmts, errors) = parse_batch(&b[0], &mssql());
        assert_eq!((stmts.len(), errors.len()), (2, 0));
    }

    #[test]
    fn statement_clients_report_each_failing_statement() {
        let cfg = Config::default().for_dialect("postgresql");
        let b = split_batches("select 1;\nselect 1 +;\nselect 2;\nfoo;\n", &cfg);
        assert_eq!(b.len(), 1);
        let (stmts, errors) = parse_batch(&b[0], &cfg);
        assert_eq!(stmts.len(), 2);
        let lines: Vec<usize> = errors.iter().map(|e| e.0).collect();
        assert_eq!(lines, [2, 4]);
    }
}
