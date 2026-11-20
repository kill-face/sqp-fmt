//! Sets the layout rules from the AST.
//!
//! Every gap starts out as `Rule::Src` (keep the source's line break,
//! re-indented relative to an anchor token). Handlers for the node kinds we
//! know replace that with the house layout; anything else keeps its source
//! layout, re-indented with its surroundings, with the nodes inside it still
//! formatted (see `generic`).
//!
//! A handler gets a node and its token range; it owns the gaps strictly
//! inside the range (not the gap before its first token, which the parent
//! sets). It first resets them to `Src` anchored at its first token, then
//! recurses into its children (which own their insides), then sets the gaps
//! between and before its children.

mod ddl;
mod dml;
mod expr;
mod query;
mod stmt;

use std::ops::ControlFlow;

use sqlparser::ast::*;
use sqlparser::tokenizer::{Location, Span, Token};

use crate::config::{Comma, Config, ListStyle, Wrap, WrapStyle};
use crate::layout::{Gap, Ind, Layout, Rule, Sp};

pub fn build(l: &mut Layout, stmts: &[Statement], cfg: &Config) {
    if l.toks.is_empty() {
        return;
    }
    let mut b = Builder {
        l,
        item: None,
        extend: Default::default(),
        exists_paren: None,
        if_anchor: None,
        cfg,
        ctx: Ctx::Other,
        chain: None,
    };
    b.keyword_starts(stmts);
    b.top_level(stmts);
}

pub(crate) struct Builder<'a> {
    pub l: &'a mut Layout,
    /// First token of the select item being built (window blocks line up
    /// with it).
    pub item: Option<usize>,
    /// Spans that start after a keyword they include in the source but not
    /// in the AST (`values` before the rows of a `values` query, `use`):
    /// span start -> the keyword's token.
    pub extend: std::collections::HashMap<Location, usize>,
    /// Where the `(` of an `exists` subquery goes, set by the condition
    /// being built (`if`, `where` chains).
    pub exists_paren: Option<Ind>,
    /// The token an `if` lines up with instead of its own `if`: the `else`
    /// of an `else if` kept on one line. Taken by the next `if`.
    pub if_anchor: Option<usize>,
    pub cfg: &'a Config,
    /// The condition the expression being built is part of: its
    /// parenthesised `and` / `or` groups take that condition's `groups`
    /// settings.
    pub ctx: Ctx,
    /// Set right before building a condition whose top-level `and` / `or`
    /// chain is laid out by `expr` (see `Expr::BinaryOp`); taken by the next
    /// `expr` call.
    pub chain: Option<Ctx>,
}

/// Which condition an expression is part of (see `Builder::ctx`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ctx {
    Where,
    Having,
    JoinOn,
    CaseWhen,
    If,
    While,
    MergeOn,
    MergeWhen,
    Check,
    Other,
}

/// Finds the spans whose keyword the AST leaves out (see `Builder::extend`).
struct KeywordStarts(Vec<(Location, &'static str)>);

impl Visitor for KeywordStarts {
    type Break = ();
    fn pre_visit_query(&mut self, q: &Query) -> ControlFlow<()> {
        if let SetExpr::Values(_) = q.body.as_ref() {
            self.0.push((q.span().start, "values"));
        }
        ControlFlow::Continue(())
    }
    fn pre_visit_statement(&mut self, s: &Statement) -> ControlFlow<()> {
        if let Statement::Use(_) = s {
            self.0.push((s.span().start, "use"));
        }
        ControlFlow::Continue(())
    }
}

/// Inclusive token index range.
pub(crate) type R = (usize, usize);

pub(crate) fn loc_lt(a: Location, b: Location) -> bool {
    (a.line, a.column) < (b.line, b.column)
}

/// A direct child node found by `children`.
#[derive(Clone, Copy)]
pub(crate) enum Child {
    Expr(*const Expr),
    Query(*const Query),
    Stmt(*const Statement),
}

/// Collects the outermost `Expr` / `Query` / `Statement` nodes below a node.
struct Children {
    depth: usize,
    target: usize,
    out: Vec<Child>,
}

impl Children {
    fn enter(&mut self, c: Child) {
        if self.depth == self.target {
            self.out.push(c);
        }
        self.depth += 1;
    }
}

impl Visitor for Children {
    type Break = ();
    fn pre_visit_expr(&mut self, e: &Expr) -> ControlFlow<()> {
        self.enter(Child::Expr(e));
        ControlFlow::Continue(())
    }
    fn post_visit_expr(&mut self, _: &Expr) -> ControlFlow<()> {
        self.depth -= 1;
        ControlFlow::Continue(())
    }
    fn pre_visit_query(&mut self, q: &Query) -> ControlFlow<()> {
        self.enter(Child::Query(q));
        ControlFlow::Continue(())
    }
    fn post_visit_query(&mut self, _: &Query) -> ControlFlow<()> {
        self.depth -= 1;
        ControlFlow::Continue(())
    }
    fn pre_visit_statement(&mut self, s: &Statement) -> ControlFlow<()> {
        self.enter(Child::Stmt(s));
        ControlFlow::Continue(())
    }
    fn post_visit_statement(&mut self, _: &Statement) -> ControlFlow<()> {
        self.depth -= 1;
        ControlFlow::Continue(())
    }
}

impl Builder<'_> {
    pub fn n(&self) -> usize {
        self.l.toks.len()
    }

    pub fn tok(&self, i: usize) -> &crate::layout::Tok {
        &self.l.toks[i]
    }

    pub fn is_kw(&self, i: usize, kw: &str) -> bool {
        i < self.n() && self.l.toks[i].is_word(kw)
    }

    pub fn is(&self, i: usize, t: &Token) -> bool {
        i < self.n() && &self.l.toks[i].token == t
    }

    /// Token range of a span, if it maps onto tokens.
    pub fn range(&self, span: Span) -> Option<R> {
        if span.start.line == 0 || span.end.line == 0 {
            return None;
        }
        let toks = &self.l.toks;
        let a = toks.partition_point(|t| loc_lt(t.start, span.start));
        let b = toks.partition_point(|t| !loc_lt(span.end, t.end));
        if a >= toks.len() || b == 0 || b - 1 < a {
            return None;
        }
        let a = self.extend.get(&span.start).copied().unwrap_or(a);
        Some((a, b - 1))
    }

    fn keyword_starts(&mut self, stmts: &[Statement]) {
        let mut k = KeywordStarts(Vec::new());
        for s in stmts {
            let _ = s.visit(&mut k);
        }
        for (loc, kw) in k.0 {
            if let Some(i) = self.at(loc)
                && i > 0
                && self.is_kw(i - 1, kw)
            {
                self.extend.insert(loc, i - 1);
            }
        }
    }

    pub fn rng<T: Spanned>(&self, node: &T) -> Option<R> {
        self.range(node.span())
    }

    /// The token starting at `loc`.
    pub fn at(&self, loc: Location) -> Option<usize> {
        if loc.line == 0 {
            return None;
        }
        let toks = &self.l.toks;
        let i = toks.partition_point(|t| loc_lt(t.start, loc));
        (i < toks.len() && toks[i].start == loc).then_some(i)
    }

    pub fn att(&self, t: &helpers::attached_token::AttachedToken) -> Option<usize> {
        self.at(t.0.span.start)
    }

    /// Sets the rule of gap `i` (the gap before token `i`).
    /// Sets the rule of gap `i` (the gap before token `i`). Gaps next to a
    /// `@formatter:off` region, and rules that indent relative to a token in
    /// one, are left alone.
    pub fn set(&mut self, i: usize, rule: Rule) {
        if i == 0 || i >= self.n() || self.l.raw[i] || self.l.raw[i - 1] {
            return;
        }
        let ind = match rule {
            Rule::Break(ind)
            | Rule::BreakIf(_, ind, _)
            | Rule::Keep(ind, _)
            | Rule::Fill(_, ind, _) => Some(ind),
            _ => None,
        };
        if let Some(Ind::Col(t, _) | Ind::LineOf(t, _) | Ind::Src(t)) = ind
            && self.l.raw[t]
        {
            return;
        }
        self.l.gaps[i].rule = rule;
    }

    pub fn rule(&self, i: usize) -> Rule {
        self.l.gaps[i].rule
    }

    /// Resets the gaps inside `r` to the source layout relative to `r.0`.
    pub fn src(&mut self, r: R) {
        for i in r.0 + 1..=r.1.min(self.n() - 1) {
            self.l.gaps[i] = Gap {
                rule: Rule::Src,
                anchor: r.0,
                align: None,
            };
        }
    }

    /// Marks gap `i` as a cell of alignment group `a`, for the item starting
    /// at token `start`.
    pub fn align(&mut self, i: usize, a: usize, start: usize) {
        if i > 0 && i < self.n() {
            self.l.gaps[i].align = Some((a, start));
        }
    }

    /// Like `align`, for an item that may start on an earlier line.
    pub fn align_any(&mut self, i: usize, a: usize) {
        if i > 0 && i < self.n() {
            self.l.gaps[i].align = Some((a, usize::MAX));
        }
    }

    pub fn group(&mut self, first: usize, last: usize, keep: bool) -> usize {
        self.l.group(first, last, keep)
    }

    pub fn align_group(&mut self) -> usize {
        self.l.align_group()
    }

    /// Whether the source had a line break before token `i`.
    pub fn nl_before(&self, i: usize) -> bool {
        i < self.n() && self.l.trivia[i].newlines() > 0
    }

    /// Whether the source had a line break anywhere inside `r`.
    pub fn multiline(&self, r: R) -> bool {
        (r.0 + 1..=r.1).any(|i| self.nl_before(i))
            || (r.0..=r.1).any(|i| self.l.toks[i].multiline())
    }

    /// The `)` matching the `(` at `open`.
    pub fn close_paren(&self, open: usize) -> Option<usize> {
        let mut depth = 0usize;
        for i in open..self.n() {
            match self.l.toks[i].token {
                Token::LParen => depth += 1,
                Token::RParen => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// The `(` matching the `)` at `close`.
    pub fn open_paren(&self, close: usize) -> Option<usize> {
        let mut depth = 0usize;
        for i in (0..=close).rev() {
            match self.l.toks[i].token {
                Token::RParen => depth += 1,
                Token::LParen => {
                    depth = depth.checked_sub(1)?;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
        None
    }

    /// First token in `from..=to` that is keyword `kw`, outside parentheses.
    pub fn find_kw(&self, from: usize, to: usize, kw: &str) -> Option<usize> {
        let mut depth = 0i32;
        for i in from..=to.min(self.n().saturating_sub(1)) {
            match self.l.toks[i].token {
                Token::LParen => depth += 1,
                Token::RParen => depth = (depth - 1).max(0),
                _ if depth == 0 && self.l.toks[i].is_word(kw) => return Some(i),
                _ => {}
            }
        }
        None
    }

    /// Lays out a comma-separated list of items: commas hug the item before
    /// them; the source's line breaks are kept (relative to `anchor`).
    pub fn comma_flat(&mut self, items: &[R], anchor: usize) {
        for w in items.windows(2) {
            let (a, b) = (w[0], w[1]);
            if a.1 + 2 == b.0 && self.is(a.1 + 1, &Token::Comma) {
                self.set(a.1 + 1, Rule::Keep(Ind::Src(anchor), Sp::No));
                self.set(b.0, Rule::Keep(Ind::Src(anchor), Sp::One));
            }
        }
    }

    /// Chops a comma-separated list with leading commas when group `g`
    /// breaks: each `, item` starts a line at `ind`.
    ///
    /// A comment written right after a comma can't move before it, so such a
    /// comma stays at the end of its line and the next item starts a line
    /// at the items' column.
    pub fn comma_chop(&mut self, items: &[R], g: usize, ind: Ind) {
        let item_ind = match ind {
            Ind::Col(t, off) => Ind::Col(t, off + 2),
            Ind::LineOf(t, off) => Ind::LineOf(t, off + 2),
            other => other,
        };
        for w in items.windows(2) {
            let (a, b) = (w[0], w[1]);
            if a.1 + 2 == b.0 && self.is(a.1 + 1, &Token::Comma) {
                if self.l.trivia[b.0].has_comments() {
                    self.set(a.1 + 1, Rule::Flat(Sp::No));
                    self.set(b.0, Rule::BreakIf(g, item_ind, Sp::One));
                } else {
                    self.set(a.1 + 1, Rule::BreakIf(g, ind, Sp::No));
                    self.set(b.0, Rule::FlatIn(g, Sp::One));
                }
            }
        }
    }

    /// Whether `items` are separated by single commas.
    pub fn comma_separated(&self, items: &[R]) -> bool {
        items
            .windows(2)
            .all(|w| w[0].1 + 2 == w[1].0 && self.is(w[0].1 + 1, &Token::Comma))
    }

    /// The items of the comma-separated list between the parentheses at
    /// `open` and `close` (split at the commas outside nested parentheses).
    pub fn split_commas(&self, open: usize, close: usize) -> Vec<R> {
        let mut items = Vec::new();
        let mut depth = 0usize;
        let mut start = open + 1;
        for i in open + 1..close {
            match self.l.toks[i].token {
                Token::LParen => depth += 1,
                Token::RParen => depth = depth.saturating_sub(1),
                Token::Comma if depth == 0 => {
                    if i > start {
                        items.push((start, i - 1));
                    }
                    start = i + 1;
                }
                _ => {}
            }
        }
        if close > start {
            items.push((start, close - 1));
        }
        items
    }

    /// One indent level (`indent`).
    pub fn ind(&self) -> isize {
        self.cfg.indent as isize
    }

    /// A construct's own indent setting, else one indent level.
    pub fn ind_or(&self, v: Option<usize>) -> isize {
        v.map_or(self.ind(), |v| v as isize)
    }

    /// The block indent of the current condition's parenthesised groups.
    pub fn groups_indent(&self) -> isize {
        let c = self.cfg;
        let g = match self.ctx {
            Ctx::Where => &c.where_.groups,
            Ctx::Having => &c.having.groups,
            Ctx::JoinOn => &c.join_on.groups,
            Ctx::CaseWhen => &c.case_when.groups,
            Ctx::If => &c.if_condition.groups,
            Ctx::While => &c.while_condition.groups,
            Ctx::MergeOn => &c.merge_on.groups,
            Ctx::MergeWhen => &c.merge_when_condition.groups,
            Ctx::Check => &c.check_constraint.groups,
            Ctx::Other => &c.other_conditions.groups,
        };
        self.ind_or(g.indent)
    }

    /// The top-level `and` / `or` chain settings of a condition.
    pub fn chain_style(&self, ctx: Ctx) -> WrapStyle {
        let c = self.cfg;
        match ctx {
            Ctx::Where => c.where_.style(),
            Ctx::Having => c.having.style(),
            Ctx::JoinOn => c.join_on.style(),
            Ctx::CaseWhen => c.case_when.style(),
            Ctx::If => c.if_condition.style(),
            Ctx::While => c.while_condition.style(),
            Ctx::MergeOn => c.merge_on.style(),
            Ctx::MergeWhen => c.merge_when_condition.style(),
            Ctx::Check => c.check_constraint.style(),
            Ctx::Other => c.other_conditions.style(),
        }
    }

    /// The parenthesised-group settings of the current condition.
    pub fn groups_style(&self) -> WrapStyle {
        let c = self.cfg;
        match self.ctx {
            Ctx::Where => c.where_.groups.style(),
            Ctx::Having => c.having.groups.style(),
            Ctx::JoinOn => c.join_on.groups.style(),
            Ctx::CaseWhen => c.case_when.groups.style(),
            Ctx::If => c.if_condition.groups.style(),
            Ctx::While => c.while_condition.groups.style(),
            Ctx::MergeOn => c.merge_on.groups.style(),
            Ctx::MergeWhen => c.merge_when_condition.groups.style(),
            Ctx::Check => c.check_constraint.groups.style(),
            Ctx::Other => c.other_conditions.groups.style(),
        }
    }

    /// Runs `f` with the condition context set to `ctx`.
    pub fn in_ctx(&mut self, ctx: Ctx, f: impl FnOnce(&mut Self)) {
        let saved = self.ctx;
        self.ctx = ctx;
        f(self);
        self.ctx = saved;
    }

    /// A comma-separated list without parentheses of its own (`select`
    /// items, `group by`, `values` rows, `exec` arguments), laid out by
    /// `st`. Broken, `, item` starts a line at `comma` (leading commas), or
    /// `item` at `comma` + 2 (trailing). Kept line breaks of a `never` list
    /// are re-indented relative to `anchor`.
    pub fn bare_list(&mut self, items: &[R], st: ListStyle, comma: Ind, anchor: usize) {
        let (Some(&first), Some(&last)) = (items.first(), items.last()) else {
            return;
        };
        if items.len() < 2 || !self.comma_separated(items) {
            return;
        }
        let item_ind = shift(comma, 2);
        match st.wrap {
            Wrap::Never if st.keep => self.comma_flat(items, anchor),
            Wrap::Never => {
                for w in items.windows(2) {
                    self.set(w[0].1 + 1, Rule::Flat(Sp::No));
                    self.set(w[1].0, Rule::Flat(Sp::One));
                }
            }
            Wrap::IfLong => {
                let g = self.group(first.0, last.1, st.keep);
                for w in items.windows(2) {
                    let (c, b) = (w[0].1 + 1, w[1].0);
                    if st.comma == Comma::Leading && !self.l.trivia[b].has_comments() {
                        self.set(c, Rule::Fill(g, comma, Sp::No));
                        self.set(b, Rule::FlatIn(g, Sp::One));
                    } else {
                        self.set(c, Rule::FlatIn(g, Sp::No));
                        self.set(b, Rule::Fill(g, item_ind, Sp::One));
                    }
                }
            }
            Wrap::ChopIfLong | Wrap::Always => {
                let g = self.group(first.0, last.1, st.keep);
                self.l.groups[g].force = st.wrap == Wrap::Always;
                if st.comma == Comma::Leading {
                    self.comma_chop(items, g, comma);
                } else {
                    for w in items.windows(2) {
                        self.set(w[0].1 + 1, Rule::FlatIn(g, Sp::No));
                        self.set(w[1].0, Rule::BreakIf(g, item_ind, Sp::One));
                    }
                }
            }
        }
    }

    /// The inside of a parenthesised comma-separated list `( a, b, c )`
    /// (not the gap before the `(`), laid out by `st`: `never` on one line;
    /// `if_long` filled, continuation lines under the first item (leading
    /// commas under the `(`, which gets a space after it); `chop_if_long` /
    /// `always` a block: `(` ends its line, items 2 in from the `(`, `)`
    /// under it. Kept line breaks of a `never` list are re-indented
    /// relative to `anchor`.
    pub fn paren_list(
        &mut self,
        open: usize,
        items: &[R],
        close: usize,
        st: ListStyle,
        anchor: usize,
    ) {
        let (Some(&first), Some(&last)) = (items.first(), items.last()) else {
            return;
        };
        if first.0 != open + 1 || last.1 + 1 != close || !self.comma_separated(items) {
            return;
        }
        match st.wrap {
            Wrap::Never if st.keep => {
                self.set(first.0, Rule::Keep(Ind::Src(anchor), Sp::No));
                self.comma_flat(items, anchor);
                self.set(close, Rule::Keep(Ind::Src(anchor), Sp::No));
            }
            Wrap::Never => {
                self.set(first.0, Rule::Flat(Sp::No));
                for w in items.windows(2) {
                    self.set(w[0].1 + 1, Rule::Flat(Sp::No));
                    self.set(w[1].0, Rule::Flat(Sp::One));
                }
                self.set(close, Rule::Flat(Sp::No));
            }
            Wrap::IfLong => {
                let g = self.group(open, close, st.keep);
                let leading = st.comma == Comma::Leading;
                if leading {
                    self.set(first.0, Rule::SpaceIf(g, Sp::One, Sp::No));
                } else {
                    self.set(first.0, Rule::Flat(Sp::No));
                }
                for w in items.windows(2) {
                    let (c, b) = (w[0].1 + 1, w[1].0);
                    if leading {
                        self.set(c, Rule::Fill(g, Ind::Col(open, 0), Sp::No));
                        self.set(b, Rule::FlatIn(g, Sp::One));
                    } else {
                        self.set(c, Rule::FlatIn(g, Sp::No));
                        self.set(b, Rule::Fill(g, Ind::Col(open, 1), Sp::One));
                    }
                }
                self.set(close, Rule::Flat(Sp::No));
            }
            Wrap::ChopIfLong | Wrap::Always => {
                let g = self.group(open, close, st.keep);
                self.l.groups[g].force = st.wrap == Wrap::Always;
                self.set(
                    first.0,
                    Rule::BreakIf(g, Ind::Col(open, self.ind()), Sp::No),
                );
                if st.comma == Comma::Leading {
                    self.comma_chop(items, g, Ind::Col(open, 0));
                } else {
                    for w in items.windows(2) {
                        self.set(w[0].1 + 1, Rule::FlatIn(g, Sp::No));
                        self.set(
                            w[1].0,
                            Rule::BreakIf(g, Ind::Col(open, self.ind()), Sp::One),
                        );
                    }
                }
                self.set(close, Rule::BreakIf(g, Ind::Col(open, 0), Sp::No));
            }
        }
    }

    /// Recurses into the outermost expressions, queries and statements
    /// below `node`, keeping the source layout around them. `is_node`: the
    /// node is itself an `Expr` / `Query` / `Statement` (and so visits
    /// itself first).
    pub fn generic<V: Visit>(&mut self, node: &V, is_node: bool) {
        let mut c = Children {
            depth: 0,
            target: usize::from(is_node),
            out: Vec::new(),
        };
        let _ = node.visit(&mut c);
        for child in c.out {
            // SAFETY: the visitor hands out references into `node`, which is
            // borrowed for this whole call.
            unsafe {
                match child {
                    Child::Expr(e) => self.expr(&*e),
                    Child::Query(q) => self.query(&*q),
                    Child::Stmt(s) => self.statement(&*s),
                }
            }
        }
    }

    fn top_level(&mut self, stmts: &[Statement]) {
        let n = self.n();
        for i in 1..n {
            self.l.gaps[i].anchor = 0;
        }
        // The first statement starts at the first token.
        if let Some(r) = stmts.first().and_then(|s| self.rng(s))
            && r.0 > 0
            && (0..r.0).all(|i| self.is_leading_keyword(i))
        {
            self.extend.insert(stmts[0].span().start, 0);
        }
        self.statements(stmts, Ind::Abs(0));
        self.semicolons();
    }

    /// `;` hugs the token before it unless the source had it on its own line.
    fn semicolons(&mut self) {
        for i in 1..self.n() {
            if matches!(self.l.toks[i].token, Token::SemiColon) && self.rule(i) == Rule::Src {
                let anchor = self.l.gaps[i].anchor;
                self.set(i, Rule::Keep(Ind::Src(anchor), Sp::No));
            }
        }
    }

    /// A list of statements, each starting a line at `ind`. `begin try` /
    /// `begin catch` are statements of their own in the list (as are
    /// `end try` / `end catch`); the statements between are indented under
    /// them.
    pub fn statements(&mut self, stmts: &[Statement], ind: Ind) {
        let mut open: Vec<usize> = Vec::new();
        let mut prev_end: Option<usize> = None;
        for s in stmts {
            let Some(mut r) = self.rng(s) else { continue };
            // Leading keywords some spans leave out (`truncate table`):
            // back up over keywords right after the previous statement.
            if let Some(pe) = prev_end {
                let mut a = r.0;
                while a > pe + 1 && self.is_leading_keyword(a - 1) {
                    a -= 1;
                }
                if a < r.0 {
                    self.extend.insert(s.span().start, a);
                    r.0 = a;
                }
            }
            prev_end = Some(r.1);
            let is_begin = matches!(
                s,
                Statement::StartTransaction {
                    modifier: Some(TransactionModifier::Try | TransactionModifier::Catch),
                    has_end_keyword: false,
                    ..
                }
            );
            let is_end = matches!(
                s,
                Statement::Commit {
                    end: true,
                    modifier: Some(TransactionModifier::Try | TransactionModifier::Catch),
                    ..
                }
            );
            let here = if is_end && !open.is_empty() {
                Ind::Col(open.pop().unwrap(), 0)
            } else {
                open.last().map_or(ind, |&b| Ind::Col(b, self.ind()))
            };
            self.stmt_start(r.0, here);
            self.statement(s);
            if is_begin {
                open.push(r.0);
            }
        }
    }

    fn is_leading_keyword(&self, i: usize) -> bool {
        matches!(&self.l.toks[i].token, Token::Word(w) if w.quote_style.is_none()
            && w.keyword != sqlparser::keywords::Keyword::NoKeyword
            && !["end", "begin", "else", "go", "try", "catch"].iter().any(|k| w.value.eq_ignore_ascii_case(k)))
    }

    /// Starts a statement at token `i` on a new line. A `;` written right
    /// before it (`;with`) stays with it.
    pub fn stmt_start(&mut self, i: usize, ind: Ind) {
        if i > 0
            && self.is(i - 1, &Token::SemiColon)
            && !self.l.src_space[i]
            && self.l.trivia[i].items.is_empty()
            && (i < 2 || self.nl_before(i - 1))
        {
            self.set(i - 1, Rule::Break(ind));
            self.set(i, Rule::Flat(Sp::No));
        } else {
            self.set(i, Rule::Break(ind));
        }
    }
}

/// `ind` moved `by` columns right.
pub(crate) fn shift(ind: Ind, by: isize) -> Ind {
    match ind {
        Ind::Col(t, off) => Ind::Col(t, off + by),
        Ind::LineOf(t, off) => Ind::LineOf(t, off + by),
        Ind::Abs(c) => Ind::Abs((c as isize + by).max(0) as usize),
        other => other,
    }
}
