//! Formatting a whole file: line endings, the client's script syntax (kept
//! as written), `@formatter:off` regions, one SQL part at a time; and the
//! safety net.

use std::panic::{AssertUnwindSafe, catch_unwind};

use sqlparser::ast::Statement;
use sqlparser::dialect::Dialect;
use sqlparser::parser::{Parser, ParserError, ParserOptions};
use sqlparser::tokenizer::{Location, Token, TokenWithSpan, Tokenizer, Whitespace};

use crate::config::Config;
use crate::layout::{self, Gap, Layout, Rule, Tok, Trivia, TriviaItem};
use crate::script::{self, Client, Lexed, PartKind, Positions};

/// What happened to the batches of a file. A batch is a part of the file
/// between client commands: a `GO` batch for sqlcmd; for the other clients,
/// which send one statement at a time, the statements between commands.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Stats {
    pub batches: usize,
    /// Formatted.
    pub formatted: usize,
    /// Formatted, but some of their statements don't parse and are left as
    /// written (clients that send statements one at a time).
    pub partial: usize,
    /// Left as written because they don't parse (or don't tokenize).
    pub unparsed: usize,
    /// Left as written because the output's tokens differed from the input's.
    pub mismatches: usize,
    /// Left as written because formatting panicked or failed internally.
    pub errors: usize,
    /// Left as written because they lie wholly in `@formatter:off` regions.
    pub off: usize,
    /// Statements left as written in partly formatted batches.
    pub unparsed_statements: usize,
    /// Details of mismatches and errors (file line, message).
    pub problems: Vec<(usize, String)>,
    /// Parser errors of what was left as written: the batch's, or each
    /// statement's in a partly formatted batch (file line, message).
    pub parse_errors: Vec<(usize, String)>,
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

    let script = script::split(&text, lines.len(), cfg);
    let off = off_lines(&script.markers, lines.len());
    let mut out: Vec<String> = Vec::with_capacity(lines.len());
    for part in &script.parts {
        let r = part.lines.clone();
        match &part.kind {
            PartKind::Sql { delimiter } => format_batch(
                &lines[r.clone()],
                &off[r.clone()],
                r.start,
                cfg,
                delimiter,
                &mut out,
                &mut stats,
            ),
            PartKind::Directive | PartKind::Data => {
                out.extend(lines[r].iter().map(|l| l.to_string()));
            }
        }
    }

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
/// `off` marker to the line with the `on` marker, both included), from the
/// markers in the file's comments.
fn off_lines(markers: &[(usize, bool)], n: usize) -> Vec<bool> {
    let mut off = vec![false; n];
    let mut active = false;
    let mut k = 0;
    for (i, o) in off.iter_mut().enumerate() {
        let was = active;
        while k < markers.len() && markers[k].0 <= i {
            if markers[k].0 == i {
                active = !markers[k].1;
            }
            k += 1;
        }
        *o = was || active;
    }
    off
}

/// Formats one batch (lines between client commands), appending the output
/// lines to `out`.
fn format_batch(
    lines: &[&str],
    off: &[bool],
    first_line: usize,
    cfg: &Config,
    delimiter: &str,
    out: &mut Vec<String>,
    stats: &mut Stats,
) {
    let verbatim = |out: &mut Vec<String>| out.extend(lines.iter().map(|l| l.to_string()));
    if lines.iter().all(|l| l.trim().is_empty()) {
        // Blank lines between client commands: collapse to one.
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
    // 1-based file line of a 0-based line of the batch.
    let at = |line: usize| first_line + line + 1;
    let result = catch_unwind(AssertUnwindSafe(|| format_text(&text, off, cfg, delimiter)));
    match result {
        Ok(Ok(f)) => {
            if f.unparsed.is_empty() {
                stats.formatted += 1;
            } else {
                stats.partial += 1;
                stats.unparsed_statements += f.unparsed.len();
                stats
                    .parse_errors
                    .extend(f.unparsed.into_iter().map(|(l, m)| (at(l), m)));
            }
            out.extend(f.lines);
        }
        Ok(Err(BatchError::Unparsed(errors))) => {
            stats.unparsed += 1;
            stats
                .parse_errors
                .extend(errors.into_iter().map(|(l, m)| (at(l), m)));
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
    /// Doesn't tokenize or parse: the errors, by 0-based line in the batch.
    Unparsed(Vec<(usize, String)>),
    Mismatch(String),
    Internal(String),
}

/// A formatted batch.
#[derive(Debug)]
pub struct Formatted {
    pub lines: Vec<String>,
    /// The statements left as written: their parser errors, by 0-based
    /// line in the batch.
    pub unparsed: Vec<(usize, String)>,
}

/// Parser options for a client's batches. sqlcmd sends a whole `GO` batch,
/// in which T-SQL lets statements follow each other without `;`. The other
/// clients send one statement at a time, ended by the client's terminator:
/// a statement must end there (or at the end of the text), so a statement
/// the parser only partly understands is reported, not split in two.
pub fn options(client: Client) -> ParserOptions {
    ParserOptions {
        require_semicolon_stmt_delimiter: client.sends_statements(),
        ..ParserOptions::default()
    }
}

/// Parses tokens (see `script::lex`) as a whole.
pub fn parse_tokens(
    toks: &[TokenWithSpan],
    dialect: &dyn Dialect,
    client: Client,
) -> Result<Vec<Statement>, ParserError> {
    Parser::new(dialect)
        .with_options(options(client))
        .with_tokens_with_locations(toks.to_vec())
        .parse_statements()
}

/// 0-based line of a parser error (its "at Line: n" position, else 0).
fn error_line(e: &ParserError) -> usize {
    let msg = e.to_string();
    msg.split_once(" at Line: ")
        .and_then(|(_, p)| p.split(',').next()?.trim().parse::<usize>().ok())
        .map_or(0, |l| l.saturating_sub(1))
}

/// The parse of a batch: its statements, and the statements that don't
/// parse (their lines and errors).
pub struct Parsed {
    pub stmts: Vec<Statement>,
    /// 0-based lines of the statements that don't parse.
    pub failed_lines: Vec<bool>,
    /// Parser errors of the statements that don't parse, by 0-based line.
    pub unparsed: Vec<(usize, String)>,
}

/// The first and last 0-based line a token is on (a token ending at the
/// start of a line, like a newline, doesn't reach into it).
fn token_lines(t: &TokenWithSpan) -> (usize, usize) {
    let first = t.span.start.line.max(1) as usize - 1;
    let mut last = t.span.end.line.max(1) as usize - 1;
    if t.span.end.column == 1 && last > first {
        last -= 1;
    }
    (first, last)
}

/// Parses a batch (`nlines` lines). sqlcmd sends the batch as a whole, so
/// it parses as a whole or not at all (`Unparsed`). The other clients send
/// each statement on its own, so a statement that doesn't parse doesn't
/// affect the others: the parser reads statement after statement, and
/// after an error resumes past the client statement (up to its terminator)
/// the error is in; the failed statement's lines are left as written.
pub fn parse_batch(
    lx: &Lexed,
    nlines: usize,
    dialect: &dyn Dialect,
    client: Client,
    delimiter: &str,
) -> Result<Parsed, BatchError> {
    let mut failed_lines = vec![false; nlines];
    if !client.sends_statements() {
        return match parse_tokens(&lx.toks, dialect, client) {
            Ok(stmts) => Ok(Parsed {
                stmts,
                failed_lines,
                unparsed: Vec::new(),
            }),
            Err(e) => Err(BatchError::Unparsed(vec![(error_line(&e), e.to_string())])),
        };
    }
    // Where each client statement ends (exclusive token index).
    let ends: Vec<usize> = lx
        .statements(client, delimiter)
        .iter()
        .map(|r| r.end)
        .collect();
    let end_of = |i: usize| {
        let k = ends.partition_point(|&e| e <= i);
        ends.get(k).copied().unwrap_or(lx.toks.len())
    };
    let mut stmts = Vec::new();
    let mut unparsed = Vec::new();
    let mut from = 0;
    'restart: while from < lx.toks.len() {
        let mut p = Parser::new(dialect)
            .with_options(options(client))
            .with_tokens_with_locations(lx.toks[from..].to_vec());
        loop {
            while p.consume_token(&Token::SemiColon) {}
            if p.peek_token_ref().token == Token::EOF {
                break 'restart;
            }
            // The statement's first token (the parser skips whitespace).
            let start = from + p.index() + {
                let rest = &lx.toks[from + p.index()..];
                rest.iter()
                    .position(|t| !matches!(t.token, Token::Whitespace(_)))
                    .unwrap_or(0)
            };
            let r = p
                .parse_statement()
                .and_then(|s| match p.peek_token_ref().token {
                    Token::SemiColon | Token::EOF => Ok(s),
                    _ => p.expected_ref("end of statement", p.peek_token_ref()),
                });
            match r {
                Ok(s) => stmts.push(s),
                Err(e) => {
                    // The last token the parser read (it stops on or right
                    // after the one it can't take).
                    let at = (from + p.index().saturating_sub(1))
                        .clamp(start, lx.toks.len().saturating_sub(1));
                    let to = end_of(at).max(start + 1);
                    let (a, _) = token_lines(&lx.toks[start]);
                    let (_, b) = token_lines(&lx.toks[to - 1]);
                    for f in failed_lines.iter_mut().take(b + 1).skip(a) {
                        *f = true;
                    }
                    unparsed.push((error_line(&e).max(a), e.to_string()));
                    from = to;
                    continue 'restart;
                }
            }
        }
    }
    if stmts.is_empty() && !unparsed.is_empty() {
        return Err(BatchError::Unparsed(unparsed));
    }
    // A token can't be partly left as written.
    loop {
        let mut grew = false;
        for t in &lx.toks {
            let (a, b) = token_lines(t);
            if a != b && (a..=b).any(|i| failed_lines.get(i) == Some(&true)) {
                for f in failed_lines.iter_mut().take(b + 1).skip(a) {
                    grew |= !*f;
                    *f = true;
                }
            }
        }
        if !grew {
            break;
        }
    }
    Ok(Parsed {
        stmts,
        failed_lines,
        unparsed,
    })
}

/// Formats the text of one batch (ending in a newline). `off` marks the
/// batch's lines in `@formatter:off` regions; `delimiter` is the mysql
/// client's delimiter in effect.
pub fn format_text(
    text: &str,
    off: &[bool],
    cfg: &Config,
    delimiter: &str,
) -> Result<Formatted, BatchError> {
    let dialect = cfg.dialect();
    let dialect = dialect.as_ref();
    let client = Client::of(cfg.dialect_name());
    let timing = std::env::var_os("SQP_FMT_TIMING").is_some();
    let t = std::time::Instant::now();
    let lap = |what: &str| {
        if timing {
            eprintln!("{what}: {:?}", t.elapsed());
        }
    };
    let lx = script::lex(text, dialect, client, delimiter);
    if let Some(e) = &lx.error {
        let line = e.location.line.max(1) as usize - 1;
        return Err(BatchError::Unparsed(vec![(line, e.to_string())]));
    }
    lap("tokenize");
    let nlines = text.split('\n').count();
    let parsed = parse_batch(&lx, nlines, dialect, client, delimiter)?;
    lap("parse");
    let off: Vec<bool> = (0..nlines)
        .map(|i| off.get(i).copied().unwrap_or(false) || parsed.failed_lines[i])
        .collect();
    let mut l = layout_from_tokens(text, &lx.toks, &off)?;
    l.margin = cfg.margin;
    lap("layout");
    crate::case::apply(&mut l, &parsed.stmts, cfg);
    lap("case");
    crate::build::build(&mut l, &parsed.stmts, cfg);
    l.apart = apart(&l, dialect);
    lap("build");
    let lines = layout::print(&l);
    lap("print");
    let mut out = lines.join("\n");
    out.push('\n');
    verify(text, &lx, &l, &out, cfg, dialect, client, delimiter)?;
    lap("verify");
    Ok(Formatted {
        lines,
        unparsed: parsed.unparsed,
    })
}

/// Builds the token and trivia lists.
fn layout_from_tokens(
    text: &str,
    raw_toks: &[sqlparser::tokenizer::TokenWithSpan],
    off: &[bool],
) -> Result<Layout, BatchError> {
    let pos = Positions::new(text);
    let line_starts: Vec<usize> = std::iter::once(0)
        .chain(text.match_indices('\n').map(|(i, _)| i + 1))
        .collect();
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
        let (line0, end_line0) = token_lines(t);
        let is_nl = matches!(t.token, Token::Whitespace(Whitespace::Newline));
        // A newline belongs to the line it ends.
        let in_off = is_off(line0) && (!is_nl || is_off(line0 + 1));
        if in_off {
            if !is_off(end_line0) {
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
                    &pos,
                    std::mem::take(&mut trivia),
                    space,
                    true,
                );
                space = false;
            }
            continue;
        }
        if end_line0 != line0 && (line0..=end_line0).any(is_off) {
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
                    &pos,
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

/// Which gaps must keep a space (see `Layout::apart`): the source had
/// whitespace there, the gap's rule may print it without, and the two
/// tokens written together tokenize as something else.
fn apart(l: &Layout, dialect: &dyn Dialect) -> Vec<bool> {
    (0..l.toks.len())
        .map(|i| {
            if i == 0 || !l.trivia[i].had_space(l.src_space[i]) || !layout::may_join(l, i) {
                return false;
            }
            let (a, b) = (&l.toks[i - 1], &l.toks[i]);
            let glued = format!("{}{}", a.text, b.text);
            match Tokenizer::new(dialect, &glued).tokenize() {
                Ok(t) => {
                    let t: Vec<&Token> = t.iter().filter(|t| **t != Token::EOF).collect();
                    // Case aside: the words may have been re-cased already.
                    !(t.len() == 2 && same(&a.token, t[0], true) && same(&b.token, t[1], true))
                }
                Err(_) => true,
            }
        })
        .collect()
}

fn push_tok(
    l: &mut Layout,
    t: &sqlparser::tokenizer::TokenWithSpan,
    text: &str,
    pos: &Positions,
    trivia: Trivia,
    space: bool,
    raw: bool,
) {
    let (a, b) = (pos.byte(t.span.start), pos.byte(t.span.end));
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
/// the formatter re-cased. A client terminator (a mysql delimiter, psql's
/// `\g ...`) must keep its text, and the output must still be one batch
/// of SQL to the client (no line of it now read as a client command).
#[allow(clippy::too_many_arguments)]
fn verify(
    text: &str,
    input: &Lexed,
    l: &Layout,
    out: &str,
    cfg: &Config,
    dialect: &dyn Dialect,
    client: Client,
    delimiter: &str,
) -> Result<(), BatchError> {
    let got = script::lex(out, dialect, client, delimiter);
    if let Some(e) = &got.error {
        return Err(BatchError::Mismatch(format!(
            "output doesn't tokenize: {e}"
        )));
    }
    let significant = |t: &&TokenWithSpan| {
        !matches!(
            t.token,
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
    let (pin, pout) = (Positions::new(text), Positions::new(out));
    let mut it = got.toks.iter().filter(significant);
    for want in input.toks.iter().filter(significant) {
        match it.next() {
            Some(g)
                if same(&want.token, &g.token, changed.contains(&want.span.start))
                    && (want.token != Token::SemiColon
                        || pin.slice(want.span) == pout.slice(g.span)) => {}
            other => {
                return Err(BatchError::Mismatch(format!(
                    "{:?} at {} became {:?}",
                    want.token,
                    want.span.start,
                    other.map(|g| &g.token)
                )));
            }
        }
    }
    if let Some(extra) = it.next() {
        return Err(BatchError::Mismatch(format!(
            "extra output token {:?}",
            extra.token
        )));
    }
    let nlines = out.strip_suffix('\n').unwrap_or(out).split('\n').count();
    let parts = script::split_lexed(out, nlines, cfg, delimiter, Some(got)).parts;
    if parts
        .iter()
        .any(|p| !matches!(p.kind, PartKind::Sql { .. }))
    {
        return Err(BatchError::Mismatch(
            "the output has a line the client would read as a command".into(),
        ));
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
