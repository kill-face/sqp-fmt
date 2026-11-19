//! Span audit: checks that the source spans of the parsed AST are complete
//! enough for a formatter to map every token back to the node it belongs to.
//!
//! Checks, per parsed batch:
//!
//! 1. Top level: every token other than `;` (whitespace and comments
//!    excluded) lies inside exactly one top-level statement's span, and each
//!    statement's span runs from its first token to its last.
//! 2. Nested nodes: statements, queries, selects, expressions, table
//!    factors, joins, functions (with their `OVER` clause) and the DDL pieces
//!    (column definitions and options, constraints, parameters) have a
//!    non-empty span, inside their parent's, starting and ending on token
//!    boundaries.
//! 3. Lists: between consecutive items of a list (statements in a block,
//!    select items, column definitions, arguments, ...) there are only
//!    separators, so no token falls between the items' spans.
//! 4. Keywords and brackets: a node's span holds at least as many tokens as
//!    its `Display` output (for nodes without a query inside, whose `Display`
//!    can add `AS`), and starts and ends with the same tokens (nested nodes
//!    other than statements, whose `Display` normalises e.g. `EXEC`). This
//!    catches spans that miss a keyword or bracket in the middle of a parent.

use std::fmt::{self, Debug, Write as _};
use std::ops::ControlFlow;

use sqlparser::ast::*;
use sqlparser::dialect::MsSqlDialect;
use sqlparser::keywords::Keyword;
use sqlparser::parser::{Parser, ParserError, ParserOptions};
use sqlparser::tokenizer::{Location, Span, Token, TokenWithSpan, Tokenizer};

use crate::Batch;

/// One problem with one node's span.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// What is wrong, e.g. `empty span` or `misses leading tokens`.
    pub problem: String,
    /// Node kind, e.g. `Statement::Declare` or `ColumnOption::NotNull`.
    pub node: String,
    /// Where (the node, or for an empty span its nearest located ancestor).
    pub at: Location,
}

/// Totals over everything audited so far.
#[derive(Debug, Default)]
pub struct Audit {
    /// Top-level statements checked.
    pub statements: usize,
    /// Nested nodes checked (including nested statements).
    pub nodes: usize,
    /// Tokens other than `;`, whitespace and comments.
    pub tokens: usize,
    /// Tokens outside every top-level statement span.
    pub uncovered: usize,
    /// Tokens inside more than one top-level statement span.
    pub overlapping: usize,
    /// Nodes (top-level or nested) with an empty span.
    pub empty: usize,
    /// Nested nodes whose span isn't inside their parent's.
    pub outside_parent: usize,
    /// Spans that don't start at a token start or end at a token end.
    pub off_boundary: usize,
    /// Nodes whose span doesn't reach their first or last token.
    pub short: usize,
    /// Nodes whose span holds fewer tokens than their `Display` output, or
    /// starts or ends with a different token.
    pub unlike_display: usize,
    /// Tokens between list items that aren't separators.
    pub gap_tokens: usize,
    pub findings: Vec<Finding>,
}

impl Audit {
    fn find(&mut self, problem: impl Into<String>, node: impl Into<String>, at: Location) {
        self.findings.push(Finding {
            problem: problem.into(),
            node: node.into(),
            at,
        });
    }
}

/// Name of the enum variant (or struct) `v`, from its `Debug` output.
fn variant<T: Debug + ?Sized>(v: &T) -> String {
    struct Name(String);
    impl fmt::Write for Name {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            for c in s.chars() {
                if c.is_alphanumeric() || c == '_' {
                    self.0.push(c);
                } else {
                    return Err(fmt::Error);
                }
            }
            Ok(())
        }
    }
    let mut name = Name(String::new());
    let _ = write!(name, "{v:?}");
    name.0
}

/// The tokens of `node`'s `Display` output, and whether their number is
/// comparable to the source: it isn't for nodes with a query or statement
/// inside, whose `Display` adds `AS`, `;`, ...
fn display_tokens<T: Visit + fmt::Display>(
    node: &T,
    is_statement: bool,
) -> Option<(Vec<Token>, bool)> {
    struct Nested(usize);
    impl Visitor for Nested {
        type Break = ();
        fn pre_visit_query(&mut self, _: &Query) -> ControlFlow<()> {
            ControlFlow::Break(())
        }
        fn pre_visit_statement(&mut self, _: &Statement) -> ControlFlow<()> {
            self.0 += 1;
            if self.0 > 1 {
                ControlFlow::Break(())
            } else {
                ControlFlow::Continue(())
            }
        }
    }
    let mut nested = Nested(usize::from(!is_statement));
    let countable = !node.visit(&mut nested).is_break();
    let text = node.to_string();
    let toks = Tokenizer::new(&MsSqlDialect {}, &text).tokenize().ok()?;
    let toks = toks
        .into_iter()
        .filter(|t| !matches!(t, Token::Whitespace(_) | Token::EOF | Token::SemiColon))
        .collect();
    Some((toks, countable))
}

/// Tokens compared case-insensitively, as `Display` upper-cases keywords.
fn same_token(a: &Token, b: &Token) -> bool {
    a.to_string().eq_ignore_ascii_case(&b.to_string())
}

fn contains(outer: Span, inner: Span) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

/// The batch's tokens, without whitespace and comments, in source order.
struct Tokens(Vec<TokenWithSpan>);

impl Tokens {
    fn new(text: &str) -> Result<Self, ParserError> {
        let toks = Tokenizer::new(&MsSqlDialect {}, text)
            .tokenize_with_location()
            .map_err(|e| ParserError::TokenizerError(e.to_string()))?;
        Ok(Tokens(
            toks.into_iter()
                .filter(|t| !matches!(t.token, Token::Whitespace(_) | Token::EOF))
                .collect(),
        ))
    }

    fn starting_at(&self, loc: Location) -> Option<usize> {
        let i = self.0.partition_point(|t| t.span.start < loc);
        (self.0.get(i)?.span.start == loc).then_some(i)
    }

    fn ending_at(&self, loc: Location) -> Option<usize> {
        let i = self.0.partition_point(|t| t.span.end < loc);
        (self.0.get(i)?.span.end == loc).then_some(i)
    }

    /// Tokens inside `span`.
    fn within(&self, span: Span) -> &[TokenWithSpan] {
        let a = self.0.partition_point(|t| t.span.start < span.start);
        let b = self.0.partition_point(|t| t.span.end <= span.end);
        &self.0[a..b.max(a)]
    }

    /// Tokens after `from` (an end) and before `to` (a start).
    fn between(&self, from: Location, to: Location) -> &[TokenWithSpan] {
        let a = self.0.partition_point(|t| t.span.start < from);
        let b = self.0.partition_point(|t| t.span.end <= to);
        &self.0[a..b.max(a)]
    }

    fn get(&self, i: usize) -> Option<&TokenWithSpan> {
        self.0.get(i)
    }

    /// Index of the `)` matching the `(` at `open`.
    fn matching_paren(&self, open: usize) -> Option<usize> {
        let mut depth = 0usize;
        for (i, t) in self.0.iter().enumerate().skip(open) {
            match t.token {
                Token::LParen => depth += 1,
                Token::RParen => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
        None
    }
}

fn is_keyword(t: Option<&TokenWithSpan>, kw: Keyword) -> bool {
    matches!(t, Some(TokenWithSpan { token: Token::Word(w), .. }) if w.keyword == kw)
}

/// Parse a batch one statement at a time, the way `parse_batch` does, also
/// returning where each statement's tokens start and end.
fn parse_with_extents(batch: &Batch) -> Result<Vec<(Statement, Span)>, ParserError> {
    let options = ParserOptions {
        require_semicolon_stmt_delimiter: false,
        ..ParserOptions::default()
    };
    let dialect = MsSqlDialect {};
    let mut parser = Parser::new(&dialect)
        .with_options(options)
        .try_with_sql(&batch.text)?;
    let mut out = Vec::new();
    loop {
        while parser.consume_token(&Token::SemiColon) {}
        if parser.peek_token_ref().token == Token::EOF {
            break;
        }
        let start = parser.peek_token_ref().span.start;
        let stmt = parser.parse_statement()?;
        let end = parser.get_current_token().span.end;
        out.push((stmt, Span::new(start, end)));
    }
    Ok(out)
}

/// Audit one batch, adding to `audit`. Fails if the batch doesn't parse.
pub fn audit_batch(batch: &Batch, audit: &mut Audit) -> Result<(), ParserError> {
    let stmts = parse_with_extents(batch)?;
    let toks = Tokens::new(&batch.text)?;

    // 1. Top level.
    let mut spans = Vec::new();
    for (stmt, extent) in &stmts {
        audit.statements += 1;
        let kind = format!("Statement::{}", variant(stmt));
        let own: Vec<&TokenWithSpan> = toks
            .within(*extent)
            .iter()
            .filter(|t| t.token != Token::SemiColon)
            .collect();
        let (Some(first), Some(last)) = (own.first(), own.last()) else {
            continue;
        };
        let span = stmt.span();
        spans.push(span);
        if span == Span::empty() {
            audit.empty += 1;
            audit.find("empty span", &kind, first.span.start);
            continue;
        }
        let mut short = false;
        if span.start > first.span.start {
            short = true;
            audit.find("misses leading tokens", &kind, first.span.start);
        } else if span.start < first.span.start {
            audit.find("starts before its first token", &kind, span.start);
        }
        if span.end < last.span.end {
            short = true;
            let missing = own.iter().find(|t| t.span.end > span.end).unwrap_or(last);
            audit.find("misses trailing tokens", &kind, missing.span.start);
        } else if span.end > last.span.end {
            audit.find("ends after its last token", &kind, span.start);
        }
        if short {
            audit.short += 1;
        }
        if toks.starting_at(span.start).is_none() || toks.ending_at(span.end).is_none() {
            audit.off_boundary += 1;
            audit.find("off token boundary", &kind, span.start);
        }
    }
    for t in &toks.0 {
        if t.token == Token::SemiColon {
            continue;
        }
        audit.tokens += 1;
        match spans.iter().filter(|s| contains(**s, t.span)).count() {
            0 => audit.uncovered += 1,
            1 => {}
            _ => audit.overlapping += 1,
        }
    }

    // 2. and 3. Nested nodes and lists.
    for (stmt, _) in &stmts {
        let mut walker = Walker {
            toks: &toks,
            audit,
            stack: Vec::new(),
        };
        let _ = stmt.visit(&mut walker);
    }
    Ok(())
}

struct Walker<'a> {
    toks: &'a Tokens,
    audit: &'a mut Audit,
    /// Enclosing checked nodes: kind and span.
    stack: Vec<(String, Span)>,
}

impl Walker<'_> {
    /// Nearest enclosing node with a span.
    fn parent(&self) -> Option<&(String, Span)> {
        self.stack.iter().rev().find(|(_, s)| *s != Span::empty())
    }

    fn anchor(&self) -> Location {
        self.parent().map_or(Location::empty(), |(_, s)| s.start)
    }

    /// Check one nested node's span.
    fn check(&mut self, kind: &str, span: Span) {
        self.audit.nodes += 1;
        if span == Span::empty() {
            self.audit.empty += 1;
            let at = self.anchor();
            self.audit.find("empty span", kind, at);
            return;
        }
        if let Some((pk, ps)) = self.parent()
            && !contains(*ps, span)
        {
            let problem = format!("outside parent {pk}");
            self.audit.outside_parent += 1;
            self.audit.find(problem, kind, span.start);
        }
        if self.toks.starting_at(span.start).is_none() || self.toks.ending_at(span.end).is_none() {
            self.audit.off_boundary += 1;
            self.audit.find("off token boundary", kind, span.start);
        }
    }

    /// Flag a node whose span holds fewer tokens than its `Display` output:
    /// it misses keywords or brackets.
    fn display<T: Visit + fmt::Display>(&mut self, kind: &str, span: Span, node: &T) {
        self.display_impl(kind, span, node, false);
    }

    fn display_impl<T: Visit + fmt::Display>(
        &mut self,
        kind: &str,
        span: Span,
        node: &T,
        is_statement: bool,
    ) {
        if span == Span::empty() {
            return;
        }
        let Some((shown, countable)) = display_tokens(node, is_statement) else {
            return;
        };
        let own: Vec<&TokenWithSpan> = self
            .toks
            .within(span)
            .iter()
            .filter(|t| t.token != Token::SemiColon)
            .collect();
        if countable && own.len() < shown.len() {
            self.audit.unlike_display += 1;
            self.audit
                .find("fewer tokens than its Display", kind, span.start);
        } else if !is_statement
            && let (Some(a), Some(b), Some(c), Some(d)) =
                (own.first(), shown.first(), own.last(), shown.last())
            && !(same_token(&a.token, b) && same_token(&c.token, d))
        {
            self.audit.unlike_display += 1;
            self.audit.find(
                "first or last token differs from its Display",
                kind,
                span.start,
            );
        }
    }

    fn enter(&mut self, kind: String, span: Span) {
        self.check(&kind, span);
        self.stack.push((kind, span));
    }

    /// Check a node and its children (checked by `f`) as one unit.
    fn node(&mut self, kind: String, span: Span, f: impl FnOnce(&mut Self)) {
        self.enter(kind, span);
        f(self);
        self.stack.pop();
    }

    /// Only separators (`sep`) may sit between consecutive items of a list.
    /// With `close`, the token after the last item must be one of them.
    fn list(&mut self, list: &str, items: &[(String, Span)], sep: Token, close: &[Token]) {
        let mut items: Vec<&(String, Span)> =
            items.iter().filter(|(_, s)| *s != Span::empty()).collect();
        items.sort_by_key(|(_, s)| s.start);
        for pair in items.windows(2) {
            let ((ak, a), (_, b)) = (pair[0], pair[1]);
            if b.start < a.end {
                self.audit
                    .find(format!("items overlap in {list}"), ak, b.start);
                continue;
            }
            let stray: Vec<&TokenWithSpan> = self
                .toks
                .between(a.end, b.start)
                .iter()
                .filter(|t| t.token != sep)
                .collect();
            if let Some(t) = stray.first() {
                self.audit.gap_tokens += stray.len();
                self.audit
                    .find(format!("tokens after the item in {list}"), ak, t.span.start);
            }
        }
        if let (Some((lk, l)), false) = (items.last(), close.is_empty()) {
            let next = self
                .toks
                .ending_at(l.end)
                .and_then(|i| self.toks.get(i + 1));
            if let Some(t) = next
                && !close.contains(&t.token)
            {
                let n = self
                    .toks
                    .0
                    .iter()
                    .skip_while(|x| x.span.start < t.span.start);
                self.audit.gap_tokens += n.take_while(|x| !close.contains(&x.token)).count();
                self.audit.find(
                    format!("tokens after the last item in {list}"),
                    lk,
                    t.span.start,
                );
            }
        }
    }

    fn statements(&mut self, list: &str, stmts: &[Statement]) {
        let items: Vec<(String, Span)> = stmts
            .iter()
            .map(|s| (format!("Statement::{}", variant(s)), s.span()))
            .collect();
        self.list(list, &items, Token::SemiColon, &[]);
    }

    fn conditional(&mut self, list: &str, block: &ConditionalStatementBlock) {
        self.statements(list, block.conditional_statements.statements());
    }

    /// Where a function call really ends: after its argument list(s),
    /// `WITHIN GROUP (...)`, `FILTER (...)`, `IGNORE/RESPECT NULLS` and
    /// `OVER (...)` / `OVER name`.
    fn function_end(&self, f: &Function) -> Option<Location> {
        let mut i = self.toks.ending_at(f.name.span().end)? + 1;
        let mut end = f.name.span().end;
        loop {
            let t = self.toks.get(i);
            match t.map(|t| &t.token) {
                Some(Token::LParen) => {
                    let j = self.toks.matching_paren(i)?;
                    end = self.toks.get(j)?.span.end;
                    i = j + 1;
                }
                _ if is_keyword(t, Keyword::WITHIN)
                    && is_keyword(self.toks.get(i + 1), Keyword::GROUP) =>
                {
                    i += 2;
                }
                _ if is_keyword(t, Keyword::FILTER) => i += 1,
                _ if (is_keyword(t, Keyword::IGNORE) || is_keyword(t, Keyword::RESPECT))
                    && is_keyword(self.toks.get(i + 1), Keyword::NULLS) =>
                {
                    end = self.toks.get(i + 1)?.span.end;
                    i += 2;
                }
                _ if is_keyword(t, Keyword::OVER) => {
                    i += 1;
                    if let Some(TokenWithSpan {
                        token: Token::Word(_),
                        span,
                    }) = self.toks.get(i)
                    {
                        return Some(span.end);
                    }
                }
                _ => return Some(end),
            }
        }
    }

    fn function(&mut self, f: &Function) {
        let kind = if f.over.is_some() {
            "Function with OVER"
        } else {
            "Function"
        };
        let span = f.span();
        if span != Span::empty()
            && let Some(end) = self.function_end(f)
            && span.end < end
        {
            self.audit.short += 1;
            self.audit.find("misses closing tokens", kind, span.start);
        }
        if let FunctionArguments::List(list) = &f.args {
            let items: Vec<(String, Span)> = list
                .args
                .iter()
                .map(|a| (format!("FunctionArg::{}", variant(a)), a.span()))
                .collect();
            let close: &[Token] = if list.clauses.is_empty() {
                &[Token::RParen]
            } else {
                &[]
            };
            self.list("function arguments", &items, Token::Comma, close);
        }
        if let Some(WindowType::WindowSpec(w)) = f.over.as_deref() {
            let parts: Vec<(String, Span)> = w
                .partition_by
                .iter()
                .map(|e| (format!("Expr::{}", variant(e)), e.span()))
                .collect();
            self.list("OVER (PARTITION BY ...)", &parts, Token::Comma, &[]);
            let order: Vec<(String, Span)> = w
                .order_by
                .iter()
                .map(|o| ("OrderByExpr".to_string(), o.span()))
                .collect();
            self.list("OVER (ORDER BY ...)", &order, Token::Comma, &[]);
        }
    }

    fn table_with_joins(&mut self, list: &str, from: &[TableWithJoins]) {
        let mut items = Vec::new();
        for twj in from {
            let span = twj.span();
            items.push(("TableWithJoins".to_string(), span));
            self.node("TableWithJoins".into(), span, |w| {
                for j in &twj.joins {
                    let kind = format!("Join::{}", variant(&j.join_operator));
                    w.check(&kind, j.span());
                    w.display(&kind, j.span(), j);
                }
            });
        }
        self.list(list, &items, Token::Comma, &[]);
    }

    fn column_def(&mut self, c: &ColumnDef) {
        self.display("ColumnDef", c.span(), c);
        self.node("ColumnDef".into(), c.span(), |w| {
            for o in &c.options {
                let kind = format!("ColumnOption::{}", variant(&o.option));
                w.check(&kind, o.span());
                w.display(&kind, o.span(), o);
            }
        });
    }

    fn constraint(&mut self, c: &TableConstraint) {
        let kind = format!("TableConstraint::{}", variant(c));
        self.check(&kind, c.span());
        self.display(&kind, c.span(), c);
    }

    fn ddl(&mut self, stmt: &Statement) {
        match stmt {
            Statement::CreateTable(ct) => {
                let mut items = Vec::new();
                for c in &ct.columns {
                    self.column_def(c);
                    items.push(("ColumnDef".to_string(), c.span()));
                }
                for c in &ct.constraints {
                    self.constraint(c);
                    items.push((format!("TableConstraint::{}", variant(c)), c.span()));
                }
                self.list(
                    "CREATE TABLE elements",
                    &items,
                    Token::Comma,
                    &[Token::RParen, Token::Comma],
                );
            }
            Statement::AlterTable(at) => {
                for op in &at.operations {
                    let kind = format!("AlterTableOperation::{}", variant(op));
                    self.display(&kind, op.span(), op);
                    self.node(kind, op.span(), |w| match op {
                        AlterTableOperation::AddColumn { column_def, .. } => {
                            w.column_def(column_def)
                        }
                        AlterTableOperation::AddConstraint { constraint, .. } => {
                            w.constraint(constraint)
                        }
                        _ => {}
                    });
                }
            }
            Statement::CreateIndex(ci) => {
                let items: Vec<(String, Span)> = ci
                    .columns
                    .iter()
                    .map(|c| ("IndexColumn".to_string(), c.span()))
                    .collect();
                for (k, s) in &items {
                    self.check(k, *s);
                }
                self.list(
                    "CREATE INDEX columns",
                    &items,
                    Token::Comma,
                    &[Token::RParen],
                );
            }
            Statement::CreateFunction(cf) => {
                let items: Vec<(String, Span)> = cf
                    .args
                    .iter()
                    .flatten()
                    .map(|a| ("OperateFunctionArg".to_string(), a.span()))
                    .collect();
                for (k, s) in &items {
                    self.check(k, *s);
                }
                self.list(
                    "function parameters",
                    &items,
                    Token::Comma,
                    &[Token::RParen],
                );
                if let Some(CreateFunctionBody::AsBeginEnd(b)) = &cf.function_body {
                    self.statements("function body", &b.statements);
                }
            }
            Statement::CreateProcedure { params, body, .. } => {
                let items: Vec<(String, Span)> = params
                    .iter()
                    .flatten()
                    .map(|p| ("ProcedureParam".to_string(), p.span()))
                    .collect();
                for (k, s) in &items {
                    self.check(k, *s);
                }
                self.list("procedure parameters", &items, Token::Comma, &[]);
                if let CreateProcedureBody::As(cs) = body {
                    self.statements("procedure body", cs.statements());
                }
            }
            Statement::CreateTrigger(ct) => {
                if let Some(cs) = &ct.statements {
                    self.statements("trigger body", cs.statements());
                }
            }
            Statement::If(s) => {
                self.conditional("IF block", &s.if_block);
                for b in &s.elseif_blocks {
                    self.conditional("ELSEIF block", b);
                }
                if let Some(b) = &s.else_block {
                    self.conditional("ELSE block", b);
                }
            }
            Statement::While(s) => self.conditional("WHILE block", &s.while_block),
            Statement::StartTransaction { statements, .. } => {
                self.statements("BEGIN block", statements)
            }
            Statement::Declare { stmts, .. } => {
                let items: Vec<(String, Span)> = stmts
                    .iter()
                    .map(|d| ("Declare".to_string(), d.span()))
                    .collect();
                for (d, (k, s)) in stmts.iter().zip(&items) {
                    self.check(k, *s);
                    self.display(k, *s, d);
                }
                self.list("DECLARE", &items, Token::Comma, &[]);
            }
            Statement::Update(u) => {
                let items: Vec<(String, Span)> = u
                    .assignments
                    .iter()
                    .map(|a| ("Assignment".to_string(), a.span()))
                    .collect();
                for (k, s) in &items {
                    self.check(k, *s);
                }
                self.list("UPDATE SET", &items, Token::Comma, &[]);
                self.table_with_joins("UPDATE target", std::slice::from_ref(&u.table));
                match &u.from {
                    Some(UpdateTableFromKind::BeforeSet(f) | UpdateTableFromKind::AfterSet(f)) => {
                        self.table_with_joins("UPDATE FROM", f)
                    }
                    None => {}
                }
            }
            Statement::Delete(d) => match &d.from {
                FromTable::WithFromKeyword(f) | FromTable::WithoutKeyword(f) => {
                    self.table_with_joins("DELETE FROM", f)
                }
            },
            _ => {}
        }
    }
}

impl Visitor for Walker<'_> {
    type Break = ();

    fn pre_visit_statement(&mut self, stmt: &Statement) -> ControlFlow<()> {
        let kind = format!("Statement::{}", variant(stmt));
        if self.stack.is_empty() {
            // Checked at the top level.
            self.stack.push((kind.clone(), stmt.span()));
        } else {
            self.enter(kind.clone(), stmt.span());
        }
        self.display_impl(&kind, stmt.span(), stmt, true);
        self.ddl(stmt);
        ControlFlow::Continue(())
    }

    fn post_visit_statement(&mut self, _: &Statement) -> ControlFlow<()> {
        self.stack.pop();
        ControlFlow::Continue(())
    }

    fn pre_visit_query(&mut self, q: &Query) -> ControlFlow<()> {
        self.enter("Query".into(), q.span());
        self.check(&format!("SetExpr::{}", variant(&*q.body)), q.body.span());
        if let SetExpr::SetOperation { left, right, .. } = &*q.body {
            let span = q.body.span();
            self.node("SetExpr::SetOperation".into(), span, |w| {
                w.check(&format!("SetExpr::{}", variant(&**left)), left.span());
                w.check(&format!("SetExpr::{}", variant(&**right)), right.span());
            });
        }
        ControlFlow::Continue(())
    }

    fn post_visit_query(&mut self, _: &Query) -> ControlFlow<()> {
        self.stack.pop();
        ControlFlow::Continue(())
    }

    fn pre_visit_select(&mut self, s: &Select) -> ControlFlow<()> {
        self.enter("Select".into(), s.span());
        let items: Vec<(String, Span)> = s
            .projection
            .iter()
            .map(|i| (format!("SelectItem::{}", variant(i)), i.span()))
            .collect();
        for (k, sp) in &items {
            self.check(k, *sp);
        }
        self.list("select list", &items, Token::Comma, &[]);
        self.table_with_joins("FROM", &s.from);
        if let GroupByExpr::Expressions(exprs, _) = &s.group_by {
            let items: Vec<(String, Span)> = exprs
                .iter()
                .map(|e| (format!("Expr::{}", variant(e)), e.span()))
                .collect();
            self.list("GROUP BY", &items, Token::Comma, &[]);
        }
        ControlFlow::Continue(())
    }

    fn post_visit_select(&mut self, _: &Select) -> ControlFlow<()> {
        self.stack.pop();
        ControlFlow::Continue(())
    }

    fn pre_visit_order_by(&mut self, o: &OrderBy) -> ControlFlow<()> {
        if let OrderByKind::Expressions(exprs) = &o.kind {
            let items: Vec<(String, Span)> = exprs
                .iter()
                .map(|e| ("OrderByExpr".to_string(), e.span()))
                .collect();
            for (k, sp) in &items {
                self.check(k, *sp);
            }
            self.list("ORDER BY", &items, Token::Comma, &[]);
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_table_factor(&mut self, t: &TableFactor) -> ControlFlow<()> {
        let kind = format!("TableFactor::{}", variant(t));
        self.display(&kind, t.span(), t);
        self.enter(kind, t.span());
        if let TableFactor::NestedJoin {
            table_with_joins, ..
        } = t
        {
            self.table_with_joins("nested join", std::slice::from_ref(&**table_with_joins));
        }
        ControlFlow::Continue(())
    }

    fn post_visit_table_factor(&mut self, _: &TableFactor) -> ControlFlow<()> {
        self.stack.pop();
        ControlFlow::Continue(())
    }

    fn pre_visit_expr(&mut self, e: &Expr) -> ControlFlow<()> {
        let kind = format!("Expr::{}", variant(e));
        self.display(&kind, e.span(), e);
        self.enter(kind, e.span());
        if let Expr::Function(f) = e {
            self.function(f);
        }
        ControlFlow::Continue(())
    }

    fn post_visit_expr(&mut self, _: &Expr) -> ControlFlow<()> {
        self.stack.pop();
        ControlFlow::Continue(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::split_batches;

    fn audit(sql: &str) -> Audit {
        let mut audit = Audit::default();
        for batch in split_batches(sql) {
            audit_batch(&batch, &mut audit).unwrap();
        }
        audit
    }

    #[test]
    fn complete_spans_have_no_findings() {
        let sql = "create procedure dbo.p (@a int, @b nvarchar(10) = N'x')\n\
                   as\n\
                   begin\n\
                   \x20   set nocount on\n\
                   \x20   declare @c decimal(18, 2) = -1\n\
                   \x20   if @a is not null and (@b = 'y' or not exists (select 1 from t))\n\
                   \x20       select row_number() over (partition by x order by y desc) as rn\n\
                   \x20            , cast(z as int)\n\
                   \x20       from t inner join u on t.id = u.id\n\
                   \x20   else\n\
                   \x20       print 'none'\n\
                   end\n\
                   go\n\
                   create table dbo.t (id int not null constraint pk_t primary key clustered, x int)\n";
        let audit = audit(sql);
        assert_eq!(audit.findings, vec![], "{audit:#?}");
        assert_eq!(audit.uncovered, 0);
        assert_eq!(audit.statements, 2);
    }

    #[test]
    fn reports_tokens_outside_statements() {
        // `USE` keeps no token for its keyword.
        let audit = audit("use master\n");
        assert_eq!(audit.uncovered, 1);
        assert!(
            audit
                .findings
                .iter()
                .any(|f| f.problem == "misses leading tokens"
                    && f.node == "Statement::Use"
                    && f.at == Location::new(1, 1)),
            "{audit:#?}"
        );
    }

    #[test]
    fn reports_spans_missing_a_bracket() {
        // Table hints keep no closing paren: `)` falls outside the table factor.
        let audit = audit("select a from t with (nolock)\n");
        assert!(audit.unlike_display > 0, "{audit:#?}");
    }
}
