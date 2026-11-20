//! Formatting a whole file: line endings, `GO` / SQLCMD lines,
//! `@formatter:off` regions, and the per-batch safety net.

use std::panic::{AssertUnwindSafe, catch_unwind};

use sqlparser::ast::Statement;
use sqlparser::dialect::Dialect;
use sqlparser::parser::{Parser, ParserOptions};
use sqlparser::tokenizer::{Location, Token, TokenWithSpan, Tokenizer, Whitespace};

use crate::config::Config;
use crate::layout::{self, Gap, Layout, Rule, Tok, Trivia, TriviaItem};
use crate::{LexState, is_go_line, is_sqlcmd_line};

/// What happened to the batches of a file.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Stats {
    pub batches: usize,
    /// Formatted.
    pub formatted: usize,
    /// Left as written because they don't parse (or don't tokenize).
    pub unparsed: usize,
    /// Left as written because the output's tokens differed from the input's.
    pub mismatches: usize,
    /// Left as written because formatting panicked or failed internally.
    pub errors: usize,
    /// Left as written because they lie wholly in `@formatter:off` regions.
    pub off: usize,
    /// Details of mismatches and errors (batch start line, message).
    pub problems: Vec<(usize, String)>,
}

/// Formats a file with the default configuration (see `format_with`).
pub fn format(src: &str) -> (String, Stats) {
    format_with(src, &Config::default())
}

/// Formats a file. Never fails: anything that can't be formatted safely is
/// left exactly as written.
pub fn format_with(src: &str, cfg: &Config) -> (String, Stats) {
    let mut stats = Stats::default();
    let (bom, body) = match src.strip_prefix('\u{feff}') {
        Some(b) => ("\u{feff}", b),
        None => ("", src),
    };
    let crlf = body.contains("\r\n") && !body.replace("\r\n", "").contains(['\n', '\r']);
    // Mixed line endings: format as is; the result is only used if every
    // `\r` survived (they're inside comments or strings).
    let mixed = !crlf && body.contains('\r');
    let text = if crlf {
        body.replace("\r\n", "\n")
    } else {
        body.to_string()
    };
    let final_nl = text.ends_with('\n');
    let mut lines: Vec<&str> = text.split('\n').collect();
    if final_nl {
        lines.pop();
    }

    let off = off_lines(&lines);
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    let mut state = LexState::default();
    let mut batch_start = 0;
    let mut i = 0;
    let flush = |from: usize, to: usize, out: &mut Vec<String>, stats: &mut Stats| {
        if from < to {
            format_batch(&lines[from..to], &off[from..to], from, cfg, out, stats);
        }
    };
    while i < lines.len() {
        let line = lines[i];
        let top = state.at_top_level();
        if top && (is_go_line(line) || is_sqlcmd_line(line)) {
            flush(batch_start, i, &mut out, &mut stats);
            out.push(line.to_string());
            batch_start = i + 1;
        } else {
            state = state.advance(line);
        }
        i += 1;
    }
    flush(batch_start, lines.len(), &mut out, &mut stats);

    let mut result = out.join("\n");
    if final_nl {
        result.push('\n');
    }
    if crlf {
        result = result.replace('\n', "\r\n");
    }
    if mixed && result.matches('\r').count() != body.matches('\r').count() {
        return (src.to_string(), Stats::default());
    }
    (format!("{bom}{result}"), stats)
}

/// Which lines lie in `@formatter:off` regions (from the line with the
/// `off` marker to the line with the `on` marker, both included).
fn off_lines(lines: &[&str]) -> Vec<bool> {
    let mut off = vec![false; lines.len()];
    let mut state = LexState::default();
    let mut active = false;
    for (i, line) in lines.iter().enumerate() {
        let mut markers = Vec::new();
        state = state.advance_comments(line, |c| {
            if c.contains("@formatter:off") {
                markers.push(true);
            } else if c.contains("@formatter:on") {
                markers.push(false);
            }
        });
        let was = active;
        for m in markers {
            active = m;
        }
        off[i] = was || active;
    }
    off
}

/// Formats one batch (lines between `GO` / SQLCMD lines), appending the
/// output lines to `out`.
fn format_batch(
    lines: &[&str],
    off: &[bool],
    first_line: usize,
    cfg: &Config,
    out: &mut Vec<String>,
    stats: &mut Stats,
) {
    let verbatim = |out: &mut Vec<String>| out.extend(lines.iter().map(|l| l.to_string()));
    if lines.iter().all(|l| l.trim().is_empty()) {
        // Blank lines between GO lines: collapse to one.
        out.push(String::new());
        return;
    }
    stats.batches += 1;
    if lines
        .iter()
        .zip(off)
        .all(|(l, o)| *o || l.trim().is_empty())
    {
        stats.off += 1;
        verbatim(out);
        return;
    }
    let mut text = lines.join("\n");
    text.push('\n');
    let result = catch_unwind(AssertUnwindSafe(|| format_text(&text, off, cfg)));
    match result {
        Ok(Ok(formatted)) => {
            stats.formatted += 1;
            out.extend(formatted);
        }
        Ok(Err(BatchError::Unparsed)) => {
            stats.unparsed += 1;
            verbatim(out);
        }
        Ok(Err(BatchError::Mismatch(msg))) => {
            stats.mismatches += 1;
            stats.problems.push((first_line + 1, msg));
            verbatim(out);
        }
        Ok(Err(BatchError::Internal(msg))) => {
            stats.errors += 1;
            stats.problems.push((first_line + 1, msg));
            verbatim(out);
        }
        Err(e) => {
            let msg = e
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            stats.errors += 1;
            stats
                .problems
                .push((first_line + 1, format!("panic: {msg}")));
            verbatim(out);
        }
    }
}

#[derive(Debug)]
pub enum BatchError {
    Unparsed,
    Mismatch(String),
    Internal(String),
}

fn options() -> ParserOptions {
    ParserOptions {
        require_semicolon_stmt_delimiter: false,
        ..ParserOptions::default()
    }
}

pub fn parse(text: &str, dialect: &dyn Dialect) -> Option<Vec<Statement>> {
    let mut parser = Parser::new(dialect)
        .with_options(options())
        .try_with_sql(text)
        .ok()?;
    parser.parse_statements().ok()
}

fn parse_tokens(toks: Vec<TokenWithSpan>, dialect: &dyn Dialect) -> Option<Vec<Statement>> {
    let mut parser = Parser::new(dialect)
        .with_options(options())
        .with_tokens_with_locations(toks);
    parser.parse_statements().ok()
}

/// Formats the text of one batch (ending in a newline). `off` marks the
/// batch's lines in `@formatter:off` regions. Returns the output lines.
pub fn format_text(text: &str, off: &[bool], cfg: &Config) -> Result<Vec<String>, BatchError> {
    let dialect = cfg.dialect();
    let dialect = dialect.as_ref();
    let timing = std::env::var_os("SQP_FMT_TIMING").is_some();
    let t = std::time::Instant::now();
    let lap = |what: &str| {
        if timing {
            eprintln!("{what}: {:?}", t.elapsed());
        }
    };
    let raw_toks = Tokenizer::new(dialect, text)
        .tokenize_with_location()
        .map_err(|_| BatchError::Unparsed)?;
    lap("tokenize");
    let stmts = parse_tokens(raw_toks.clone(), dialect).ok_or(BatchError::Unparsed)?;
    lap("parse");
    let mut l = layout_from_tokens(text, &raw_toks, off)?;
    l.margin = cfg.margin;
    lap("layout");
    crate::case::apply(&mut l, &stmts, cfg);
    lap("case");
    crate::build::build(&mut l, &stmts, cfg);
    lap("build");
    let lines = layout::print(&l);
    lap("print");
    let mut out = lines.join("\n");
    out.push('\n');
    verify(&raw_toks, &l, &out, dialect)?;
    lap("verify");
    Ok(lines)
}

/// Builds the token and trivia lists.
fn layout_from_tokens(
    text: &str,
    raw_toks: &[sqlparser::tokenizer::TokenWithSpan],
    off: &[bool],
) -> Result<Layout, BatchError> {
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
    let line_ascii: Vec<bool> = text.split('\n').map(|l| l.is_ascii()).collect();
    let byte_of = |line: u64, col: u64| -> usize {
        let li = (line - 1) as usize;
        let ls = line_starts[li];
        if line_ascii.get(li).copied().unwrap_or(false) {
            return (ls + (col - 1) as usize).min(text.len());
        }
        let rest = &text[ls..];
        rest.char_indices()
            .nth((col - 1) as usize)
            .map_or(text.len(), |(b, _)| ls + b)
    };
    let is_off = |line0: usize| off.get(line0).copied().unwrap_or(false);

    let mut l = Layout::default();
    let mut trivia = Trivia::default();
    let mut space = false;
    // The region (first line) we're inside of, if any.
    let mut region: Option<usize> = None;
    for t in raw_toks {
        if matches!(t.token, Token::EOF) {
            continue;
        }
        let line0 = (t.span.start.line - 1) as usize;
        let end_line0 = (t.span.end.line - 1) as usize;
        let is_nl = matches!(t.token, Token::Whitespace(Whitespace::Newline));
        // A newline belongs to the line it ends.
        let in_off = is_off(line0) && (!is_nl || is_off(line0 + 1));
        if in_off {
            let last_nonnl = if is_nl { line0 } else { end_line0 };
            if !is_off(last_nonnl) {
                return Err(BatchError::Internal(
                    "token crosses a formatter:off boundary".into(),
                ));
            }
            // Region start: the first line of this run of off lines.
            let mut start = line0;
            while start > 0 && is_off(start - 1) {
                start -= 1;
            }
            if region != Some(start) {
                region = Some(start);
                let mut end = start;
                while is_off(end + 1) && end + 1 < line_starts.len() - 1 {
                    end += 1;
                }
                let from = line_starts[start];
                let to = line_starts.get(end + 1).map_or(text.len(), |e| e - 1);
                trivia
                    .items
                    .push(TriviaItem::Raw(text[from..to].to_string(), start));
            }
            if !matches!(t.token, Token::Whitespace(_)) {
                push_tok(
                    &mut l,
                    t,
                    text,
                    &byte_of,
                    std::mem::take(&mut trivia),
                    space,
                    true,
                );
                space = false;
            }
            continue;
        }
        if !is_nl && end_line0 != line0 && (line0..=end_line0).any(is_off) {
            return Err(BatchError::Internal(
                "token crosses a formatter:off boundary".into(),
            ));
        }
        region = None;
        match &t.token {
            Token::Whitespace(ws) => {
                match layout::trivia_item(ws, (t.span.start.column - 1) as usize, line0) {
                    Some(TriviaItem::Newlines(n)) => match trivia.items.last_mut() {
                        Some(TriviaItem::Newlines(m)) => *m += n,
                        _ => trivia.items.push(TriviaItem::Newlines(n)),
                    },
                    Some(item) => trivia.items.push(item),
                    None => space = true,
                }
            }
            _ => {
                push_tok(
                    &mut l,
                    t,
                    text,
                    &byte_of,
                    std::mem::take(&mut trivia),
                    space,
                    false,
                );
                space = false;
            }
        }
    }
    l.trivia.push(trivia);
    l.src_space.push(space);
    l.line_indent = text
        .split('\n')
        .map(|line| line.chars().take_while(|c| *c == ' ' || *c == '\t').count())
        .collect();
    let n = l.toks.len();
    l.gaps = vec![
        Gap {
            rule: Rule::Src,
            anchor: 0,
            align: None
        };
        n.max(1)
    ];
    Ok(l)
}

fn push_tok(
    l: &mut Layout,
    t: &sqlparser::tokenizer::TokenWithSpan,
    text: &str,
    byte_of: &dyn Fn(u64, u64) -> usize,
    trivia: Trivia,
    space: bool,
    raw: bool,
) {
    let (a, b) = (
        byte_of(t.span.start.line, t.span.start.column),
        byte_of(t.span.end.line, t.span.end.column),
    );
    l.toks.push(Tok {
        token: t.token.clone(),
        start: t.span.start,
        end: t.span.end,
        text: text[a..b].to_string(),
        case_changed: false,
        col: (t.span.start.column - 1) as usize,
        line: (t.span.start.line - 1) as usize,
    });
    l.trivia.push(trivia);
    l.src_space.push(space);
    l.raw.push(raw);
}

/// The safety net: the output must have the same tokens as the input,
/// comments included; only whitespace may differ, and case only for words
/// the formatter lower-cased.
fn verify(
    input: &[TokenWithSpan],
    l: &Layout,
    out: &str,
    dialect: &dyn Dialect,
) -> Result<(), BatchError> {
    let toks = Tokenizer::new(dialect, out)
        .tokenize()
        .map_err(|e| BatchError::Mismatch(format!("output doesn't tokenize: {e}")))?;
    let significant = |t: &Token| {
        !matches!(
            t,
            Token::Whitespace(Whitespace::Space | Whitespace::Tab | Whitespace::Newline)
                | Token::EOF
        )
    };
    let changed: std::collections::HashSet<Location> = l
        .toks
        .iter()
        .filter(|t| t.case_changed)
        .map(|t| t.start)
        .collect();
    let mut got = toks.iter().filter(|t| significant(t));
    for want in input.iter().filter(|t| significant(&t.token)) {
        match got.next() {
            Some(g) if same(&want.token, g, changed.contains(&want.span.start)) => {}
            other => {
                return Err(BatchError::Mismatch(format!(
                    "{:?} at {} became {other:?}",
                    want.token, want.span.start
                )));
            }
        }
    }
    if let Some(extra) = got.next() {
        return Err(BatchError::Mismatch(format!(
            "extra output token {extra:?}"
        )));
    }
    Ok(())
}

fn same(want: &Token, got: &Token, case_changed: bool) -> bool {
    if want == got {
        return true;
    }
    match (want, got) {
        (Token::Word(a), Token::Word(b)) => {
            case_changed
                && a.quote_style.is_none()
                && b.quote_style.is_none()
                && a.value.eq_ignore_ascii_case(&b.value)
        }
        _ => false,
    }
}
