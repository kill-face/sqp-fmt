//! The script layer: a SQL file as its client sees it.
//!
//! SQL files are scripts for a command-line client, and each client has
//! syntax of its own around the SQL: commands it runs itself, a statement
//! terminator it may let you change, data it reads from the script. None of
//! that is SQL, so none of it is formatted: it is kept exactly as written,
//! and it tells the formatter where the SQL is. Each dialect's client, from
//! its documentation:
//!
//! - `mssql`, sqlcmd ("Commands in the sqlcmd utility"): `GO [count]` ends a
//!   batch; `:r`, `:setvar`, `:connect`, `:on error`, ... and the
//!   colon-less `ED`, `RESET`, `QUIT`, `EXIT`, `!!`. "sqlcmd recognizes
//!   commands only if they appear at the start of a line"; "each command
//!   must be on a separate line".
//! - `mysql`, the mysql client ("mysql Client Commands"): long-form commands
//!   (`delimiter`, `source`, `use`, `charset`, ...) first on a line, when no
//!   statement is pending; backslash short forms (`\d`, `\.`, ...) at the
//!   start of a line; `\g` / `\G` anywhere send the statement. `delimiter
//!   str` changes the statement delimiter, which "mysql interprets ...
//!   anywhere it occurs, except within quoted strings" (and comments).
//! - `postgresql`, psql ("Meta-Commands"): "Anything you enter in psql that
//!   begins with an unquoted backslash is a psql meta-command"; a line that
//!   starts with one is the command's. Mid-line, the commands that send the
//!   query buffer (`\g`, `\gx`, `\gset`, `\gexec`, `\gdesc`,
//!   `\crosstabview`, `\watch`) end the statement and take the rest of the
//!   line. After `COPY ... FROM STDIN;` and `\copy ... from stdin`, the data
//!   rows follow in the script, "until a line containing only `\.`".
//! - `sqlite`, the sqlite3 shell ("Rules for dot-commands"): "Dot-commands
//!   must begin with a "." at the left margin with no preceding
//!   whitespace", on one line, not "in the middle of an ordinary SQL
//!   statement"; and whole-line `#` comments (`process_input` in the
//!   shell's source, shell.c, skips a line starting with `#` between
//!   statements).
//! - `duckdb`, the DuckDB CLI ("Dot Commands"), a fork of the sqlite3 shell:
//!   the same rules ("begin the line with a period").
//! - `snowflake`, SnowSQL and Snowflake CLI ("Using SnowSQL", "Commands
//!   reference"): "All commands in SnowSQL start with an exclamation point
//!   (`!`)" (`!set`, `!source`, `!define`, ...). Recognised first on a line
//!   between statements, the way the other clients' named commands are
//!   (no SQL statement starts with `!`).
//! - `teradata`, BTEQ ("BTEQ Command Syntax"): a period identifies a BTEQ
//!   command, which doesn't share its line with SQL. (Commands written
//!   without the period aren't recognised: they can't be told from SQL.)
//! - `generic` and the other dialects: no client syntax. `generic` is the
//!   union of the dialects' grammars, but the clients conflict (`GO` and
//!   `DELIMITER` are identifiers or keywords elsewhere, `\` and `.` mean
//!   different things to different clients), so it recognises none.
//!
//! Lexing is sqlparser's tokenizer for the dialect throughout: it decides
//! what is inside a string, quoted identifier, comment or dollar quote, so
//! a `GO` or `\.` there is not a command.

use std::ops::Range;

use sqlparser::dialect::Dialect;
use sqlparser::tokenizer::{
    Location, Span, Token, TokenWithSpan, Tokenizer, TokenizerError, Whitespace,
};

use crate::config::Config;

/// The command-line client whose script syntax a dialect's files use.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Client {
    /// No client syntax.
    None,
    /// SQL Server's sqlcmd (also SSMS's SQLCMD mode).
    Sqlcmd,
    /// The mysql client.
    Mysql,
    /// PostgreSQL's psql.
    Psql,
    /// The sqlite3 shell (and the DuckDB CLI, its fork).
    Sqlite3,
    /// Teradata's BTEQ.
    Bteq,
    /// SnowSQL and Snowflake CLI.
    Snowsql,
}

impl Client {
    /// The client of a dialect (by its canonical name).
    pub fn of(dialect: &str) -> Client {
        match dialect {
            "mssql" => Client::Sqlcmd,
            "mysql" => Client::Mysql,
            "postgresql" => Client::Psql,
            "sqlite" | "duckdb" => Client::Sqlite3,
            "snowflake" => Client::Snowsql,
            "teradata" => Client::Bteq,
            _ => Client::None,
        }
    }

    /// Whether a `;` inside parentheses doesn't end a statement: psql's lexer
    /// counts parentheses (`paren_depth` in psqlscan.l); the mysql client
    /// and the sqlite3 shell end a statement at any `;` outside strings and
    /// comments.
    pub fn counts_parens(self) -> bool {
        self == Client::Psql
    }

    /// Whether the client sends each statement to the server on its own
    /// (all but sqlcmd, which sends `GO` batches).
    pub fn sends_statements(self) -> bool {
        self != Client::Sqlcmd
    }
}

/// What a range of lines is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartKind {
    /// SQL, formatted. `delimiter` is the mysql client's statement
    /// delimiter in effect (`;` for the other clients).
    Sql { delimiter: String },
    /// A client command, kept as written.
    Directive,
    /// Data the client reads from the script (`COPY ... FROM STDIN`), kept
    /// as written.
    Data,
}

/// A range of whole lines (0-based) of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Part {
    pub kind: PartKind,
    pub lines: Range<usize>,
}

/// A file split into parts, and its `@formatter:off` / `@formatter:on`
/// markers: (0-based line, `true` for `on`), in order.
#[derive(Debug, Clone, Default)]
pub struct Script {
    pub parts: Vec<Part>,
    pub markers: Vec<(usize, bool)>,
}

/// Byte offsets of a text's lines, to convert between tokenizer locations
/// (1-based line and column, columns in chars) and byte offsets.
pub struct Positions<'a> {
    text: &'a str,
    starts: Vec<usize>,
    ascii: Vec<bool>,
}

impl<'a> Positions<'a> {
    pub fn new(text: &'a str) -> Self {
        let starts: Vec<usize> = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        let ascii = text.split('\n').map(|l| l.is_ascii()).collect();
        Positions {
            text,
            starts,
            ascii,
        }
    }

    /// Byte offset of line `line` (0-based).
    pub fn line_start(&self, line: usize) -> usize {
        self.starts.get(line).copied().unwrap_or(self.text.len())
    }

    /// Byte offset of a location (clamped to the text).
    pub fn byte(&self, loc: Location) -> usize {
        if loc.line == 0 {
            return 0;
        }
        let li = (loc.line - 1) as usize;
        let Some(&ls) = self.starts.get(li) else {
            return self.text.len();
        };
        let col = (loc.column.max(1) - 1) as usize;
        if self.ascii.get(li).copied().unwrap_or(false) {
            return (ls + col).min(self.text.len());
        }
        self.text[ls..]
            .char_indices()
            .nth(col)
            .map_or(self.text.len(), |(b, _)| ls + b)
    }

    /// Location of a byte offset.
    pub fn loc(&self, byte: usize) -> Location {
        let li = self.starts.partition_point(|&s| s <= byte) - 1;
        let col = self.text[self.starts[li]..byte].chars().count();
        Location::new(li as u64 + 1, col as u64 + 1)
    }

    /// The source text of a span.
    pub fn slice(&self, span: Span) -> &'a str {
        let (a, b) = (self.byte(span.start), self.byte(span.end));
        &self.text[a.min(b)..b]
    }
}

/// A text's tokens, as the client and the server see them.
#[derive(Debug, Default)]
pub struct Lexed {
    /// In source order, covering the text; no EOF. A client's statement
    /// terminator (a mysql delimiter other than `;`, `\g`, psql's
    /// `\gset ...`) is one `SemiColon` token whose span is its text.
    pub toks: Vec<TokenWithSpan>,
    /// Indices of the client's terminators in `toks`, ascending.
    pub client_terms: Vec<usize>,
    /// Where the text stopped tokenizing (then `toks` ends before it).
    pub error: Option<TokenizerError>,
}

impl Lexed {
    pub fn is_client_term(&self, i: usize) -> bool {
        self.client_terms.binary_search(&i).is_ok()
    }

    /// Token index ranges of the statements, as the client sends them: up
    /// to and including each terminator (a client terminator, or a `;`,
    /// outside parentheses for psql, unless a mysql delimiter replaces it).
    pub fn statements(&self, client: Client, delimiter: &str) -> Vec<Range<usize>> {
        let semis = !(client == Client::Mysql && delimiter != ";");
        let mut out = Vec::new();
        let mut start = 0;
        let mut depth = 0i32;
        for (i, t) in self.toks.iter().enumerate() {
            match t.token {
                Token::LParen if client.counts_parens() => depth += 1,
                Token::RParen if client.counts_parens() => depth -= 1,
                Token::SemiColon if self.is_client_term(i) || (semis && depth <= 0) => {
                    out.push(start..i + 1);
                    start = i + 1;
                    depth = 0;
                }
                _ => {}
            }
        }
        if start < self.toks.len() {
            out.push(start..self.toks.len());
        }
        out
    }
}

/// Whether a token's text is quoted: a string, quoted identifier, comment
/// or `COPY` data. Client syntax is never recognised inside one.
pub fn is_literal(t: &Token) -> bool {
    match t {
        Token::Word(w) => w.quote_style.is_some(),
        Token::Whitespace(w) => matches!(
            w,
            Whitespace::SingleLineComment { .. } | Whitespace::MultiLineComment(_)
        ),
        Token::SingleQuotedString(_)
        | Token::DoubleQuotedString(_)
        | Token::TripleSingleQuotedString(_)
        | Token::TripleDoubleQuotedString(_)
        | Token::DollarQuotedString(_)
        | Token::SingleQuotedByteStringLiteral(_)
        | Token::DoubleQuotedByteStringLiteral(_)
        | Token::TripleSingleQuotedByteStringLiteral(_)
        | Token::TripleDoubleQuotedByteStringLiteral(_)
        | Token::SingleQuotedRawStringLiteral(_)
        | Token::DoubleQuotedRawStringLiteral(_)
        | Token::TripleSingleQuotedRawStringLiteral(_)
        | Token::TripleDoubleQuotedRawStringLiteral(_)
        | Token::NationalStringLiteral(_)
        | Token::QuoteDelimitedStringLiteral(_)
        | Token::NationalQuoteDelimitedStringLiteral(_)
        | Token::EscapedStringLiteral(_)
        | Token::UnicodeStringLiteral(_)
        | Token::HexStringLiteral(_)
        | Token::CopyData { .. } => true,
        _ => false,
    }
}

/// Tokenizes `text[from..]`, with locations in `text`.
fn tokenize_from(
    pos: &Positions,
    from: usize,
    to: usize,
    dialect: &dyn Dialect,
) -> (Vec<TokenWithSpan>, Option<TokenizerError>) {
    let base = pos.loc(from);
    let shift = |l: Location| {
        if l.line == 0 {
            l
        } else if l.line == 1 {
            Location::new(base.line, base.column + l.column - 1)
        } else {
            Location::new(base.line + l.line - 1, l.column)
        }
    };
    let mut buf = Vec::new();
    let r = Tokenizer::new(dialect, &pos.text[from..to]).tokenize_with_location_into_buf(&mut buf);
    let mut toks: Vec<TokenWithSpan> = buf
        .into_iter()
        .filter(|t| t.token != Token::EOF)
        .map(|t| TokenWithSpan::new(t.token, Span::new(shift(t.span.start), shift(t.span.end))))
        .collect();
    // `COPY ... FROM STDIN` data comes before its `;` in the tokenizer's
    // output; put it in source order.
    if !toks.is_sorted_by_key(|t| (t.span.start.line, t.span.start.column)) {
        toks.sort_by_key(|t| (t.span.start.line, t.span.start.column));
    }
    let err = r.err().map(|mut e| {
        e.location = shift(e.location);
        e
    });
    (toks, err)
}

/// Replaces the tokens the tokenizer made of a MySQL `/*! ... */` version
/// comment (the dialects that `supports_multiline_comment_hints` expand it
/// into its contents, with locations that don't match the source) with one
/// comment token: to the formatter it is a comment, kept as written.
/// Checks that the tokens cover the text from `from` without gaps or
/// overlaps; `Err` if they don't.
fn collapse_hints(
    pos: &Positions,
    from: usize,
    mut toks: Vec<TokenWithSpan>,
    dialect: &dyn Dialect,
) -> Result<Vec<TokenWithSpan>, String> {
    let text = pos.text;
    let hints = dialect.supports_multiline_comment_hints();
    let mut out = Vec::with_capacity(toks.len());
    let mut expect = from;
    let mut i = 0;
    let comment = |a: usize, b: usize| {
        let inner = text[a..b]
            .strip_prefix("/*")
            .and_then(|s| s.strip_suffix("*/"))
            .unwrap_or(&text[a + 2..b]);
        TokenWithSpan::new(
            Token::Whitespace(Whitespace::MultiLineComment(inner.to_string())),
            Span::new(pos.loc(a), pos.loc(b)),
        )
    };
    while i < toks.len() {
        let a = pos.byte(toks[i].span.start);
        if hints && text[expect..].starts_with("/*!") && a >= expect {
            let is_comment = a == expect
                && matches!(
                    toks[i].token,
                    Token::Whitespace(Whitespace::MultiLineComment(_))
                );
            if !is_comment {
                let end = text[expect + 3..]
                    .find("*/")
                    .map_or(text.len(), |e| expect + 3 + e + 2);
                out.push(comment(expect, end));
                while i < toks.len() && pos.byte(toks[i].span.start) < end {
                    i += 1;
                }
                expect = end;
                continue;
            }
        }
        if a != expect {
            return Err(format!(
                "the tokens don't cover the text at {}",
                pos.loc(expect.min(a))
            ));
        }
        expect = pos.byte(toks[i].span.end);
        out.push(std::mem::replace(
            &mut toks[i],
            TokenWithSpan::wrap(Token::EOF),
        ));
        i += 1;
    }
    if hints && text[expect..].starts_with("/*!") {
        let end = text[expect + 3..]
            .find("*/")
            .map_or(text.len(), |e| expect + 3 + e + 2);
        out.push(comment(expect, end));
    }
    Ok(out)
}

/// The psql meta-commands that send the query buffer, ending the statement.
const PSQL_SENDS: &[&str] = &["g", "gx", "gset", "gexec", "gdesc", "crosstabview", "watch"];

/// The first client terminator in `toks` at or after token `from`: (index of
/// the token it starts in, start byte, end byte).
fn find_terminator(
    pos: &Positions,
    toks: &[TokenWithSpan],
    client: Client,
    delimiter: &str,
) -> Option<(usize, usize, usize)> {
    let text = pos.text;
    for (k, t) in toks.iter().enumerate() {
        if is_literal(&t.token) || matches!(t.token, Token::Whitespace(_)) {
            continue;
        }
        let (a, b) = (pos.byte(t.span.start), pos.byte(t.span.end));
        match client {
            Client::Mysql => {
                if t.token == Token::Backslash
                    && matches!(text[b..].chars().next(), Some('g' | 'G'))
                {
                    return Some((k, a, b + 1));
                }
                if delimiter != ";"
                    && let Some(p) = text[a..].find(delimiter)
                    && a + p < b
                {
                    return Some((k, a + p, a + p + delimiter.len()));
                }
            }
            Client::Psql => {
                if t.token == Token::Backslash
                    && let Some(Token::Word(w)) = toks.get(k + 1).map(|n| &n.token)
                    && w.quote_style.is_none()
                    && PSQL_SENDS.contains(&w.value.as_str())
                {
                    let eol = text[a..].find('\n').map_or(text.len(), |e| a + e);
                    return Some((k, a, eol));
                }
            }
            _ => return None,
        }
    }
    None
}

/// Lexes `text` the way its client and server see it (see `Lexed`).
pub fn lex(text: &str, dialect: &dyn Dialect, client: Client, delimiter: &str) -> Lexed {
    lex_from(&Positions::new(text), 0, dialect, client, delimiter)
}

/// `lex` for the text from byte `from` on, with locations in the whole text.
fn lex_from(
    pos: &Positions,
    from: usize,
    dialect: &dyn Dialect,
    client: Client,
    delimiter: &str,
) -> Lexed {
    let text = pos.text;
    let mut out = Lexed::default();
    let tokenize = |from: usize, to: usize| -> (Vec<TokenWithSpan>, Option<TokenizerError>) {
        let (toks, err) = tokenize_from(pos, from, to, dialect);
        match collapse_hints(pos, from, toks, dialect) {
            Ok(t) => (t, err),
            Err(message) => (
                Vec::new(),
                Some(TokenizerError {
                    message,
                    location: pos.loc(from),
                }),
            ),
        }
    };
    let (mut cur, mut err) = tokenize(from, text.len());
    loop {
        let Some((k, d, e)) = find_terminator(pos, &cur, client, delimiter) else {
            out.toks.extend(cur);
            out.error = err;
            return out;
        };
        let tk = pos.byte(cur[k].span.start);
        let rest: Vec<TokenWithSpan> = cur.split_off(k);
        out.toks.extend(cur);
        if tk < d {
            // The terminator starts inside a token (`END$$`): the text
            // before it on its own, as the server gets it.
            let (frag, ferr) = tokenize(tk, d);
            out.toks.extend(frag);
            if ferr.is_some() {
                out.error = ferr;
                return out;
            }
        }
        out.client_terms.push(out.toks.len());
        out.toks.push(TokenWithSpan::new(
            Token::SemiColon,
            Span::new(pos.loc(d), pos.loc(e)),
        ));
        // Carry on with the tokens after it if one starts right there,
        // else tokenize again from there.
        match rest.iter().position(|t| pos.byte(t.span.start) >= e) {
            Some(j) if pos.byte(rest[j].span.start) == e => cur = rest[j..].to_vec(),
            _ if e >= text.len() => return out,
            _ => (cur, err) = tokenize(e, text.len()),
        }
    }
}

/// What a client command line does besides being kept.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Command {
    Plain,
    /// mysql `delimiter str`: the new delimiter (`None`: missing, the old
    /// one stays).
    Delimiter(Option<String>),
    /// psql `\copy ... from stdin`: data lines follow.
    CopyIn,
}

/// sqlcmd's batch terminator: `GO`, optionally with a repeat count, a `;`
/// and/or a trailing `--` comment.
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

/// The mysql client's long-form commands ("mysql Client Commands").
const MYSQL_COMMANDS: &[&str] = &[
    "charset",
    "clear",
    "connect",
    "delimiter",
    "edit",
    "ego",
    "exit",
    "go",
    "help",
    "nopager",
    "notee",
    "nowarning",
    "pager",
    "print",
    "prompt",
    "query_attributes",
    "quit",
    "rehash",
    "resetconnection",
    "source",
    "ssl_session_data_print",
    "status",
    "system",
    "tee",
    "use",
    "warnings",
];

/// The argument of mysql's `delimiter` / `\d`: quoted (up to the matching
/// quote on the line) or up to the first space.
fn mysql_delimiter_arg(rest: &str) -> Option<String> {
    let rest = rest.trim_start_matches([' ', '\t']);
    let mut chars = rest.chars();
    let q = chars.next()?;
    if matches!(q, '\'' | '"' | '`') {
        let mut out = String::new();
        let mut esc = false;
        for c in chars {
            if esc {
                out.push(c);
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == q {
                break;
            } else {
                out.push(c);
            }
        }
        (!out.is_empty()).then_some(out)
    } else {
        let d: String = rest.chars().take_while(|c| !c.is_whitespace()).collect();
        (!d.is_empty()).then_some(d)
    }
}

/// The first word of `s` (ASCII letters, digits, `_`), lower-cased, and
/// whether what follows it ends the word (whitespace, `;`, end of line).
fn first_word(s: &str) -> Option<(String, &str)> {
    let w: String = s
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
        .collect();
    let rest = &s[w.len()..];
    let ends = rest.is_empty() || rest.starts_with([' ', '\t', ';', '\r']);
    (!w.is_empty() && ends).then(|| (w.to_ascii_lowercase(), rest))
}

/// Whether `line`, which starts outside any token, is a client command;
/// `pending`: a statement has started and not ended.
fn command(client: Client, line: &str, pending: bool) -> Option<Command> {
    let t = line.trim_start_matches([' ', '\t']);
    match client {
        Client::None => None,
        Client::Sqlcmd => {
            if is_go_line(line) || t.starts_with(':') || t.starts_with("!!") {
                return Some(Command::Plain);
            }
            // The commands that work without their colon.
            let (w, rest) = first_word(t)?;
            let rest = rest.trim();
            let bare = match w.as_str() {
                "ed" | "reset" | "quit" => rest.is_empty(),
                "exit" => rest.is_empty() || rest.starts_with('('),
                _ => false,
            };
            bare.then_some(Command::Plain)
        }
        Client::Mysql => {
            if pending {
                return None;
            }
            if let Some(r) = t.strip_prefix('\\') {
                return Some(match r.strip_prefix('d') {
                    Some(arg) if arg.is_empty() || arg.starts_with([' ', '\t']) => {
                        Command::Delimiter(mysql_delimiter_arg(arg))
                    }
                    _ => Command::Plain,
                });
            }
            let (w, rest) = first_word(t)?;
            if !MYSQL_COMMANDS.contains(&w.as_str()) {
                return None;
            }
            Some(if w == "delimiter" {
                Command::Delimiter(mysql_delimiter_arg(rest))
            } else {
                Command::Plain
            })
        }
        Client::Psql => {
            let r = t.strip_prefix('\\')?;
            let lower = r.to_ascii_lowercase();
            let words: Vec<&str> = lower.split_whitespace().collect();
            let copy_in = words.first() == Some(&"copy")
                && words
                    .windows(2)
                    .any(|w| w[0] == "from" && matches!(w[1], "stdin" | "pstdin"));
            Some(if copy_in {
                Command::CopyIn
            } else {
                Command::Plain
            })
        }
        Client::Sqlite3 => {
            (!pending && (line.starts_with('.') || line.starts_with('#'))).then_some(Command::Plain)
        }
        Client::Bteq => {
            let mut c = t.chars();
            (!pending && c.next() == Some('.') && c.next().is_some_and(|c| c.is_ascii_alphabetic()))
                .then_some(Command::Plain)
        }
        Client::Snowsql => {
            let mut c = t.chars();
            (!pending && c.next() == Some('!') && c.next().is_some_and(|c| c.is_ascii_alphabetic()))
                .then_some(Command::Plain)
        }
    }
}

/// The end (exclusive) of the data lines starting at line `from`: after
/// the line holding only `\.`, else the end of the file.
fn data_end(lines: &[&str], from: usize) -> usize {
    (from..lines.len())
        .find(|&l| lines[l] == "\\.")
        .map_or(lines.len(), |l| l + 1)
}

/// The `@formatter:off` / `on` markers in a comment token.
fn markers(t: &TokenWithSpan, out: &mut Vec<(usize, bool)>) {
    let text = match &t.token {
        Token::Whitespace(Whitespace::SingleLineComment { comment, .. }) => comment.as_str(),
        Token::Whitespace(Whitespace::MultiLineComment(c)) => c.as_str(),
        _ => return,
    };
    let line = (t.span.start.line - 1) as usize;
    let mut found: Vec<(usize, bool)> = text
        .match_indices("@formatter:off")
        .map(|(i, _)| (i, false))
        .chain(text.match_indices("@formatter:on").map(|(i, _)| (i, true)))
        .collect();
    found.sort();
    for (i, on) in found {
        out.push((line + text[..i].matches('\n').count(), on));
    }
}

/// Splits a file (`text`, its first `nlines` lines) into SQL, client
/// commands and client data, for its dialect's client.
pub fn split(text: &str, nlines: usize, cfg: &Config) -> Script {
    split_from(text, nlines, cfg, ";")
}

/// `split`, starting with the mysql client's delimiter set to `delimiter`.
pub fn split_from(text: &str, nlines: usize, cfg: &Config, delimiter: &str) -> Script {
    split_lexed(text, nlines, cfg, delimiter, None)
}

/// `split_from`, given `text` already lexed (`lex(text, ..., delimiter)`).
pub fn split_lexed(
    text: &str,
    nlines: usize,
    cfg: &Config,
    delimiter: &str,
    mut first: Option<Lexed>,
) -> Script {
    let dialect = cfg.dialect();
    let dialect = dialect.as_ref();
    let client = Client::of(cfg.dialect_name());
    let pos = Positions::new(text);
    let all: Vec<&str> = text.split('\n').collect();
    let lines = &all[..nlines.min(all.len())];
    let n = lines.len();
    let mut s = Script::default();
    let mut delimiter = delimiter.to_string();
    let mut seg = 0;
    let mut line = 0;
    let close = |s: &mut Script, from: usize, to: usize, delimiter: &str| {
        if from < to {
            s.parts.push(Part {
                kind: PartKind::Sql {
                    delimiter: delimiter.to_string(),
                },
                lines: from..to,
            });
        }
    };
    'lex: while line < n {
        let lx = match first.take() {
            Some(lx) if line == 0 => lx,
            _ => lex_from(&pos, pos.line_start(line), dialect, client, &delimiter),
        };
        let semis = !(client == Client::Mysql && delimiter != ";");
        let err_line = lx
            .error
            .as_ref()
            .map(|e| e.location.line.max(1) as usize - 1);
        let byte = |i: usize| pos.byte(lx.toks[i].span.start);
        let mut ti = 0;
        let mut pending = false;
        let mut depth = 0i32;
        // `COPY ... FROM STDIN` (psql): the statement's first word was
        // `copy`, the last word was `from`, `from stdin` was seen.
        let (mut copy, mut from_kw, mut stdin) = (false, false, false);
        let mut l = line;
        while l < n {
            let next_line = pos.line_start(l + 1);
            // The line starts inside a token, or where the text no longer
            // tokenizes: no client syntax there.
            let inside = (ti > 0 && pos.byte(lx.toks[ti - 1].span.end) > pos.line_start(l))
                || err_line.is_some_and(|e| l > e);
            // Data the tokenizer read for `COPY ... FROM STDIN` (generic).
            if !inside
                && let Some(t) = lx.toks.get(ti)
                && matches!(t.token, Token::CopyData { .. })
                && t.span.start.line as usize == l + 1
            {
                let end = t.span.end;
                let last = end.line as usize - usize::from(end.column == 1);
                close(&mut s, seg, l, &delimiter);
                let to = last.clamp(l + 1, n);
                s.parts.push(Part {
                    kind: PartKind::Data,
                    lines: l..to,
                });
                ti += 1;
                (seg, l) = (to, to);
                (pending, depth) = (false, 0);
                continue;
            }
            if let Some(cmd) = command(client, lines[l], pending).filter(|_| !inside) {
                close(&mut s, seg, l, &delimiter);
                s.parts.push(Part {
                    kind: PartKind::Directive,
                    lines: l..l + 1,
                });
                let mut crosses = false;
                while ti < lx.toks.len() && byte(ti) < next_line {
                    markers(&lx.toks[ti], &mut s.markers);
                    crosses |= pos.byte(lx.toks[ti].span.end) > next_line;
                    ti += 1;
                }
                (pending, depth) = (false, 0);
                (copy, from_kw, stdin) = (false, false, false);
                seg = l + 1;
                match cmd {
                    Command::Delimiter(d) => {
                        if let Some(d) = d {
                            delimiter = d;
                        }
                        line = l + 1;
                        continue 'lex;
                    }
                    Command::CopyIn => {
                        let to = data_end(lines, l + 1);
                        if to > l + 1 {
                            s.parts.push(Part {
                                kind: PartKind::Data,
                                lines: l + 1..to,
                            });
                        }
                        (seg, line) = (to, to);
                        continue 'lex;
                    }
                    Command::Plain if crosses => {
                        line = l + 1;
                        continue 'lex;
                    }
                    Command::Plain => {
                        l += 1;
                        continue;
                    }
                }
            }
            // SQL.
            let mut data_next = false;
            while ti < lx.toks.len() && byte(ti) < next_line {
                let t = &lx.toks[ti];
                markers(t, &mut s.markers);
                match &t.token {
                    Token::Whitespace(_) => {}
                    Token::SemiColon if lx.is_client_term(ti) || (semis && depth <= 0) => {
                        data_next |= client == Client::Psql && copy && stdin;
                        (pending, depth) = (false, 0);
                        (copy, from_kw, stdin) = (false, false, false);
                    }
                    tok => {
                        match tok {
                            Token::LParen if client.counts_parens() => depth += 1,
                            Token::RParen if client.counts_parens() => depth -= 1,
                            _ => {}
                        }
                        let word = match tok {
                            Token::Word(w) if w.quote_style.is_none() => {
                                Some(w.value.to_ascii_lowercase())
                            }
                            _ => None,
                        };
                        if !pending {
                            copy = word.as_deref() == Some("copy");
                        }
                        stdin |= from_kw && word.as_deref() == Some("stdin");
                        from_kw = word.as_deref() == Some("from");
                        pending = true;
                    }
                }
                ti += 1;
            }
            if data_next {
                close(&mut s, seg, l + 1, &delimiter);
                let to = data_end(lines, l + 1);
                if to > l + 1 {
                    s.parts.push(Part {
                        kind: PartKind::Data,
                        lines: l + 1..to,
                    });
                }
                (seg, line) = (to, to);
                continue 'lex;
            }
            l += 1;
        }
        break;
    }
    close(&mut s, seg, n, &delimiter);
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parts(sql: &str, dialect: &str) -> Vec<(char, usize, usize)> {
        let cfg = Config::default().for_dialect(dialect);
        let n = sql.strip_suffix('\n').unwrap_or(sql).split('\n').count();
        split(sql, n, &cfg)
            .parts
            .iter()
            .map(|p| {
                let k = match p.kind {
                    PartKind::Sql { .. } => 's',
                    PartKind::Directive => 'd',
                    PartKind::Data => 'x',
                };
                (k, p.lines.start, p.lines.end)
            })
            .collect()
    }

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
    fn sqlcmd_batches() {
        let sql = "select 1\ngo\n\nselect 2\n:r other.sql\nGO 2\n";
        assert_eq!(
            parts(sql, "mssql"),
            vec![
                ('s', 0, 1),
                ('d', 1, 2),
                ('s', 2, 4),
                ('d', 4, 5),
                ('d', 5, 6)
            ]
        );
        // Not client syntax in another dialect.
        assert_eq!(parts(sql, "generic"), vec![('s', 0, 6)]);
    }

    #[test]
    fn go_inside_comments_and_strings_is_not_a_separator() {
        let src = "/* outer /* nested */\ngo\n*/\nselect 'it''s\ngo\n' as s, [a\ngo] as b\n-- /* not a comment start\ngo\nselect 2\n";
        assert_eq!(
            parts(src, "mssql"),
            vec![('s', 0, 8), ('d', 8, 9), ('s', 9, 10)]
        );
    }

    #[test]
    fn mysql_delimiter() {
        let sql = "select 1;\nDELIMITER //\ncreate procedure p()\nbegin\n  select 1;\nend //\ndelimiter ;\nselect 2;\n";
        let cfg = Config::default().for_dialect("mysql");
        let s = split(sql, 8, &cfg);
        let kinds: Vec<_> = s
            .parts
            .iter()
            .map(|p| (p.kind.clone(), p.lines.clone()))
            .collect();
        let sql_ = |d: &str| PartKind::Sql {
            delimiter: d.to_string(),
        };
        assert_eq!(
            kinds,
            vec![
                (sql_(";"), 0..1),
                (PartKind::Directive, 1..2),
                (sql_("//"), 2..6),
                (PartKind::Directive, 6..7),
                (sql_(";"), 7..8),
            ]
        );
        // The delimiter, outside strings and comments, ends a statement.
        let lx = lex(
            "select '//' /* // */ from t//select 2 //",
            &sqlparser::dialect::MySqlDialect {},
            Client::Mysql,
            "//",
        );
        assert_eq!(lx.client_terms.len(), 2);
        assert_eq!(lx.statements(Client::Mysql, "//").len(), 2);
        // Inside a word: `END$$`.
        let lx = lex(
            "end$$ select 1$$",
            &sqlparser::dialect::MySqlDialect {},
            Client::Mysql,
            "$$",
        );
        let texts: Vec<String> = lx.toks.iter().map(|t| t.token.to_string()).collect();
        assert_eq!(texts, ["end", ";", " ", "select", " ", "1", ";"]);
    }

    #[test]
    fn mysql_commands_need_an_empty_buffer() {
        let sql = "select\n  source\nfrom t;\nsource x.sql\nuse db;\n";
        assert_eq!(
            parts(sql, "mysql"),
            vec![('s', 0, 3), ('d', 3, 4), ('d', 4, 5)]
        );
    }

    #[test]
    fn psql_meta_commands_and_copy_data() {
        let sql = "\\set x 1\nselect 1 \\gset\nCOPY t (a) FROM stdin;\n1\t'a\n\\.\nselect 2;\n";
        assert_eq!(
            parts(sql, "postgresql"),
            vec![('d', 0, 1), ('s', 1, 3), ('x', 3, 5), ('s', 5, 6)]
        );
        let lx = lex(
            "select 1 \\gset p\nselect 2",
            &sqlparser::dialect::PostgreSqlDialect {},
            Client::Psql,
            ";",
        );
        assert_eq!(lx.client_terms.len(), 1);
        let p = Positions::new("select 1 \\gset p\nselect 2");
        assert_eq!(p.slice(lx.toks[lx.client_terms[0]].span), "\\gset p");
        // A backslash inside a string is not a meta-command.
        let sql = "select E'\n\\\\';\n";
        assert_eq!(parts(sql, "postgresql"), vec![('s', 0, 2)]);
    }

    #[test]
    fn sqlite_dot_commands_at_the_margin() {
        let sql = ".headers on\nselect 1\n  .x;\nselect 2;\n";
        assert_eq!(parts(sql, "sqlite"), vec![('d', 0, 1), ('s', 1, 4)]);
    }

    #[test]
    fn duckdb_dot_commands_and_snowsql_commands() {
        let sql = ".mode line\nselect 1;\n";
        assert_eq!(parts(sql, "duckdb"), vec![('d', 0, 1), ('s', 1, 2)]);
        let sql = "!set variable_substitution=true\nselect 1\n!= 2;\n!source x.sql\n";
        assert_eq!(
            parts(sql, "snowflake"),
            vec![('d', 0, 1), ('s', 1, 3), ('d', 3, 4)]
        );
    }

    #[test]
    fn generic_has_no_client_syntax() {
        let sql = "go\n\\set x 1\n.headers on\ndelimiter //\n!set a=1\n";
        assert_eq!(parts(sql, "generic"), vec![('s', 0, 5)]);
    }

    #[test]
    fn version_comments_are_comments() {
        let text = "create table t (a int) /*!50100 engine=innodb */;";
        let lx = lex(
            text,
            &sqlparser::dialect::MySqlDialect {},
            Client::Mysql,
            ";",
        );
        assert!(lx.error.is_none());
        assert!(lx.toks.iter().any(|t| matches!(
            &t.token,
            Token::Whitespace(Whitespace::MultiLineComment(c)) if c == "!50100 engine=innodb "
        )));
        let p = Positions::new(text);
        let joined: String = lx.toks.iter().map(|t| p.slice(t.span)).collect();
        assert_eq!(joined, text);
    }
}
