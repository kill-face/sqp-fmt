//! T-SQL formatter built on (a fork of) sqlparser-rs.
//!
//! Files are split into batches the way SSMS/sqlcmd do: on `GO` lines, but
//! not ones inside a block comment or a string. SQLCMD directives (`:r`,
//! `:setvar`, ...) are not SQL and are left out of parsing. Each batch keeps
//! its file line numbers by being prefixed with blank lines, so parser errors
//! point at the right line in the file.

use sqlparser::ast::Statement;
use sqlparser::ast::comments::Comments;
use sqlparser::dialect::MsSqlDialect;
use sqlparser::parser::{Parser, ParserError, ParserOptions};

/// One `GO`-separated batch of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Batch {
    /// 1-based line of the batch's first line in the file.
    pub start_line: usize,
    /// The batch text, prefixed with `start_line - 1` newlines.
    pub text: String,
}

/// `GO`, optionally with a repeat count, a `;` and/or a trailing `--` comment.
pub fn is_go_line(line: &str) -> bool {
    let line = match line.find("--") {
        Some(i) => &line[..i],
        None => line,
    };
    let line = line.trim();
    let line = line.strip_suffix(';').unwrap_or(line);
    let mut words = line.split_whitespace();
    match words.next() {
        Some(w) if w.eq_ignore_ascii_case("go") => {}
        _ => return false,
    }
    match words.next() {
        None => true,
        Some(n) => n.chars().all(|c| c.is_ascii_digit()) && words.next().is_none(),
    }
}

/// SQLCMD directive such as `:r file.sql` or `:setvar name value`.
pub fn is_sqlcmd_line(line: &str) -> bool {
    line.trim_start().starts_with(':')
}

/// What a line ends inside of, so a `GO` on the next line is only a batch
/// separator at the top level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct LexState {
    /// Nesting depth of `/* */` (T-SQL block comments nest).
    comment_depth: usize,
    /// Inside a `'...'` string (`''` is an escaped quote).
    in_string: bool,
    /// Inside a `[...]` quoted identifier (`]]` is an escaped bracket).
    in_bracket: bool,
}

impl LexState {
    fn at_top_level(self) -> bool {
        self == LexState::default()
    }

    fn advance(mut self, line: &str) -> Self {
        let b = line.as_bytes();
        let mut i = 0;
        while i < b.len() {
            let next = b.get(i + 1).copied();
            if self.comment_depth > 0 {
                match (b[i], next) {
                    (b'/', Some(b'*')) => {
                        self.comment_depth += 1;
                        i += 1;
                    }
                    (b'*', Some(b'/')) => {
                        self.comment_depth -= 1;
                        i += 1;
                    }
                    _ => {}
                }
            } else if self.in_string {
                if b[i] == b'\'' {
                    if next == Some(b'\'') {
                        i += 1;
                    } else {
                        self.in_string = false;
                    }
                }
            } else if self.in_bracket {
                if b[i] == b']' {
                    if next == Some(b']') {
                        i += 1;
                    } else {
                        self.in_bracket = false;
                    }
                }
            } else {
                match (b[i], next) {
                    (b'-', Some(b'-')) => break,
                    (b'/', Some(b'*')) => {
                        self.comment_depth = 1;
                        i += 1;
                    }
                    (b'\'', _) => self.in_string = true,
                    (b'[', _) => self.in_bracket = true,
                    _ => {}
                }
            }
            i += 1;
        }
        self
    }
}

pub fn split_batches(src: &str) -> Vec<Batch> {
    let src = src.strip_prefix('\u{feff}').unwrap_or(src);
    let mut batches = Vec::new();
    let mut current = String::new();
    let mut start_line = 1;
    let mut state = LexState::default();
    for (i, line) in src.lines().enumerate() {
        let lineno = i + 1;
        if state.at_top_level() && is_go_line(line) {
            batches.push(Batch { start_line, text: std::mem::take(&mut current) });
            start_line = lineno + 1;
            continue;
        }
        if current.is_empty() {
            current.push_str(&"\n".repeat(lineno - 1));
        }
        if state.at_top_level() && is_sqlcmd_line(line) {
            current.push('\n');
            continue;
        }
        state = state.advance(line);
        current.push_str(line);
        current.push('\n');
    }
    batches.push(Batch { start_line, text: current });
    batches.retain(|b| !b.text.trim().is_empty());
    batches
}

/// Parse one batch with the MS SQL dialect, keeping comments. Semicolons
/// between statements are optional, as in T-SQL (including inside
/// `begin ... end` and procedure bodies, with the fork).
pub fn parse_batch(batch: &Batch) -> Result<(Vec<Statement>, Comments), ParserError> {
    let options = ParserOptions {
        require_semicolon_stmt_delimiter: false,
        ..ParserOptions::default()
    };
    let mut parser = Parser::new(&MsSqlDialect {})
        .with_options(options)
        .try_with_sql(&batch.text)?;
    parser.parse_statements().map(|stmts| (stmts, parser.into_comments()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn go_lines() {
        assert!(is_go_line("go"));
        assert!(is_go_line("  GO  "));
        assert!(is_go_line("GO 5"));
        assert!(is_go_line("go;"));
        assert!(is_go_line("go -- end of batch"));
        assert!(!is_go_line("goto label"));
        assert!(!is_go_line("go x"));
    }

    #[test]
    fn batches_keep_line_numbers() {
        let b = split_batches("select 1\ngo\n\nselect 2\n:r other.sql\ngo\n");
        assert_eq!(b.len(), 2);
        assert_eq!(b[0].start_line, 1);
        assert_eq!(b[1].start_line, 3);
        assert_eq!(b[1].text, "\n\n\nselect 2\n\n");
    }

    #[test]
    fn go_inside_comments_and_strings_is_not_a_separator() {
        let src = "/* outer /* nested */\ngo\n*/\nselect 'it''s\ngo\n' as s, [a\ngo] as b\n-- /* not a comment start\ngo\nselect 2\n";
        let b = split_batches(src);
        assert_eq!(b.len(), 2, "{b:#?}");
        assert_eq!(b[1].start_line, 10);
    }

    #[test]
    fn statements_without_semicolons() {
        let b = split_batches("if 1 = 1\nbegin\n    declare @a int\n    set @a = 1\nend\nselect 1\n");
        let (stmts, _) = parse_batch(&b[0]).unwrap();
        assert_eq!(stmts.len(), 2);
    }
}
