//! Sets the layout rules from the AST.
//!
//! Every gap starts out as `Rule::Src`: keep the source's line break,
//! re-indented relative to an anchor token, so anything not handled keeps
//! its source layout, moved along with its surroundings. For now only the
//! top-level statements are placed (each starts a line at column 0); the
//! nodes inside them are visited so each keeps its layout relative to its own
//! first token.

use std::collections::HashMap;
use std::ops::ControlFlow;

use sqlparser::ast::*;
use sqlparser::tokenizer::{Location, Span, Token};

use crate::layout::{Gap, Ind, Layout, Rule, Sp};

pub fn build(l: &mut Layout, stmts: &[Statement]) {
    if l.toks.is_empty() {
        return;
    }
    let mut b = Builder {
        l,
        extend: HashMap::new(),
    };
    b.keyword_starts(stmts);
    b.top_level(stmts);
}

pub(crate) struct Builder<'a> {
    pub l: &'a mut Layout,
    /// Spans that start after a keyword they include in the source but not
    /// in the AST (`values` before the rows of a `values` query, `use`):
    /// span start -> the keyword's token.
    pub extend: HashMap<Location, usize>,
}

/// Inclusive token index range.
pub(crate) type R = (usize, usize);

pub(crate) fn loc_lt(a: Location, b: Location) -> bool {
    (a.line, a.column) < (b.line, b.column)
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

/// A direct child node found by `generic`.
#[derive(Clone, Copy)]
enum Child {
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
    fn n(&self) -> usize {
        self.l.toks.len()
    }

    fn is_kw(&self, i: usize, kw: &str) -> bool {
        i < self.n() && self.l.toks[i].is_word(kw)
    }

    /// Token range of a span, if it maps onto tokens.
    fn range(&self, span: Span) -> Option<R> {
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

    fn rng<T: Spanned>(&self, node: &T) -> Option<R> {
        self.range(node.span())
    }

    /// The token starting at `loc`.
    fn at(&self, loc: Location) -> Option<usize> {
        if loc.line == 0 {
            return None;
        }
        let toks = &self.l.toks;
        let i = toks.partition_point(|t| loc_lt(t.start, loc));
        (i < toks.len() && toks[i].start == loc).then_some(i)
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

    /// Sets the rule of gap `i` (the gap before token `i`); gaps next to a
    /// `@formatter:off` region are left alone.
    fn set(&mut self, i: usize, rule: Rule) {
        if i == 0 || i >= self.n() || self.l.raw[i] || self.l.raw[i - 1] {
            return;
        }
        self.l.gaps[i].rule = rule;
    }

    /// Resets the gaps inside `r` to the source layout relative to `r.0`.
    fn src(&mut self, r: R) {
        for i in r.0 + 1..=r.1.min(self.n() - 1) {
            self.l.gaps[i] = Gap {
                rule: Rule::Src,
                anchor: r.0,
                align: None,
            };
        }
    }

    fn top_level(&mut self, stmts: &[Statement]) {
        for i in 1..self.n() {
            self.l.gaps[i].anchor = 0;
        }
        for s in stmts {
            let Some(r) = self.rng(s) else { continue };
            self.set(r.0, Rule::Break(Ind::Abs(0)));
            self.statement(s);
        }
        // `;` hugs the token before it unless the source had it on its own line.
        for i in 1..self.n() {
            if matches!(self.l.toks[i].token, Token::SemiColon) && self.l.gaps[i].rule == Rule::Src
            {
                let anchor = self.l.gaps[i].anchor;
                self.set(i, Rule::Keep(Ind::Src(anchor), Sp::No));
            }
        }
    }

    fn statement(&mut self, s: &Statement) {
        if let Some(r) = self.rng(s) {
            self.src(r);
        }
        self.generic(s, true);
    }

    fn query(&mut self, q: &Query) {
        if let Some(r) = self.rng(q) {
            self.src(r);
        }
        self.generic(q, true);
    }

    fn expr(&mut self, e: &Expr) {
        if let Some(r) = self.rng(e) {
            self.src(r);
        }
        self.generic(e, true);
    }

    /// Recurses into the outermost expressions, queries and statements below
    /// `node`. `is_node`: the node is itself an `Expr` / `Query` /
    /// `Statement` (and so visits itself first).
    fn generic<V: Visit>(&mut self, node: &V, is_node: bool) {
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
}
