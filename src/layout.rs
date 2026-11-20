//! The layout model: a batch's tokens, the trivia (comments, line breaks)
//! between them, and a rule per gap between two tokens saying what whitespace
//! goes there. The rules are set by `build` from the AST; `print` turns them
//! into text.
//!
//! The output is the input's tokens in the same order with only the
//! whitespace between them (and the case of some words) changed, so the
//! printer can't drop or reorder anything by construction.

use sqlparser::tokenizer::{Location, Token, Whitespace};

/// One non-whitespace, non-comment token.
#[derive(Debug, Clone)]
pub struct Tok {
    pub token: Token,
    /// Source location (1-based line and column, in chars).
    pub start: Location,
    pub end: Location,
    /// Text to print (the source text, maybe lower-cased).
    pub text: String,
    /// Whether `text` differs from the source text in case.
    pub case_changed: bool,
    /// 0-based source column of the first char.
    pub col: usize,
    /// 0-based source line.
    pub line: usize,
}

impl Tok {
    pub fn is_word(&self, kw: &str) -> bool {
        matches!(&self.token, Token::Word(w) if w.quote_style.is_none() && w.value.eq_ignore_ascii_case(kw))
    }

    pub fn width(&self) -> usize {
        self.text.chars().count()
    }

    pub fn multiline(&self) -> bool {
        self.text.contains('\n')
    }
}

#[derive(Debug, Clone)]
pub enum TriviaItem {
    Newlines(usize),
    Comment {
        text: String,
        /// `--` comment (needs a line break after it).
        line_comment: bool,
        /// 0-based source column.
        col: usize,
        line: usize,
    },
    /// A `@formatter:off` region (or the part of one in this gap), printed
    /// exactly as written on lines of its own; with its first source line
    /// (0-based).
    Raw(String, usize),
}

/// What lies between two consecutive tokens in the source.
#[derive(Debug, Clone, Default)]
pub struct Trivia {
    pub items: Vec<TriviaItem>,
}

impl Trivia {
    pub fn newlines(&self) -> usize {
        self.items
            .iter()
            .map(|i| match i {
                TriviaItem::Newlines(n) => *n,
                _ => 0,
            })
            .sum()
    }

    pub fn has_comments(&self) -> bool {
        self.items
            .iter()
            .any(|i| !matches!(i, TriviaItem::Newlines(_)))
    }

    /// Whether the trivia forces a line break between the tokens around it:
    /// a `--` comment, a comment on a line of its own, or a raw region.
    pub fn forces_break(&self) -> bool {
        let mut nl = false;
        for i in &self.items {
            match i {
                TriviaItem::Newlines(_) => nl = true,
                TriviaItem::Comment { line_comment, .. } => {
                    if nl || *line_comment {
                        return true;
                    }
                }
                TriviaItem::Raw(..) => return true,
            }
        }
        false
    }

    /// Whether the source had any whitespace at all here.
    pub fn had_space(&self, src_space: bool) -> bool {
        src_space || !self.items.is_empty()
    }
}

/// Horizontal spacing for a gap printed on one line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sp {
    /// No space.
    No,
    /// One space.
    One,
    /// As the source had it, normalised (a space if there was any
    /// whitespace, except around `,` `.` `(` `)` where the usual spacing
    /// applies).
    Src,
    /// Exactly as the source had it: a space if there was any whitespace.
    Asis,
}

/// Where a line starts when a gap breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ind {
    /// The printed column of a token printed earlier, plus an offset.
    Col(usize, isize),
    /// The indent of the line a token earlier was printed on, plus an offset.
    LineOf(usize, isize),
    /// Relative to an anchor token: the anchor's printed column plus how far
    /// right of the anchor the next token was in the source.
    Src(usize),
    /// An absolute column.
    Abs(usize),
}

/// The rule for one gap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// Keep the source: a line break (indented relative to the anchor) if
    /// there was one, else spacing as in the source.
    Src,
    /// Always on one line.
    Flat(Sp),
    /// Always a line break.
    Break(Ind),
    /// A line break if the group is broken, else on one line.
    BreakIf(usize, Ind, Sp),
    /// A line break if the source had one here, else on one line.
    Keep(Ind, Sp),
    /// On one line, but a source line break here counts as the author
    /// breaking the group (e.g. after a comma, for a list chopped before
    /// its commas).
    FlatIn(usize, Sp),
    /// On one line, spaced one way if the group is broken, another if not.
    SpaceIf(usize, Sp, Sp),
    /// Filled: a line break if the source had one here or right after
    /// (after a comma), or if what follows up to the group's next `Fill` gap
    /// doesn't fit on the line.
    Fill(usize, Ind, Sp),
}

#[derive(Debug, Clone, Copy)]
pub struct Gap {
    pub rule: Rule,
    /// Anchor for `Rule::Src` and for line breaks forced by comments in a
    /// gap that would otherwise be on one line.
    pub anchor: usize,
    /// Alignment column group: when the gap is printed on one line, pad it so
    /// the next token lines up with the other tokens of the group on other
    /// lines. The `usize` is the token that starts the aligned item: a cell
    /// only counts when it's on the same line as that token.
    pub align: Option<(usize, usize)>,
}

/// A group of `BreakIf` gaps that break together: the group is broken when
/// it doesn't fit on the line, holds a forced break, or (`keep`) when the
/// source had a line break at any of its own gaps.
#[derive(Debug, Clone)]
pub struct Group {
    pub first: usize,
    pub last: usize,
    pub keep: bool,
    /// Broken no matter what.
    pub force: bool,
}

/// Everything the printer needs for one batch.
#[derive(Debug, Default)]
pub struct Layout {
    pub toks: Vec<Tok>,
    /// `trivia[i]` lies before `toks[i]`; `trivia[toks.len()]` after the last.
    pub trivia: Vec<Trivia>,
    /// Whether the source had a space or tab right before `toks[i]`.
    pub src_space: Vec<bool>,
    /// Whether `toks[i - 1]` and `toks[i]`, written apart in the source,
    /// would read as other tokens written together (`- -` as `--`, `a b` as
    /// `ab`): then they stay apart whatever the rule says.
    pub apart: Vec<bool>,
    /// `gaps[i]` is the gap before `toks[i]` (`gaps[0]` is unused).
    pub gaps: Vec<Gap>,
    pub groups: Vec<Group>,
    pub align_groups: usize,
    /// Right margin.
    pub margin: usize,
    /// Tokens inside `@formatter:off` regions (printed by the `Raw` trivia).
    pub raw: Vec<bool>,
    /// Indent (in chars) of each source line.
    pub line_indent: Vec<usize>,
}

impl Layout {
    pub fn group(&mut self, first: usize, last: usize, keep: bool) -> usize {
        self.groups.push(Group {
            first,
            last,
            keep,
            force: false,
        });
        self.groups.len() - 1
    }

    pub fn align_group(&mut self) -> usize {
        self.align_groups += 1;
        self.align_groups - 1
    }
}

/// Spacing between two tokens printed on one line when no rule says
/// otherwise.
pub fn default_space(prev: &Tok, next: &Tok, src: bool) -> bool {
    use Token::*;
    match (&prev.token, &next.token) {
        (_, Comma | SemiColon | Period | RParen) => false,
        (Period | LParen, _) => false,
        (Comma, _) => true,
        (DoubleColon, _) | (_, DoubleColon) => false,
        _ => src,
    }
}

/// Whether the rule of the gap before `toks[i]` may print it without a
/// space.
pub fn may_join(l: &Layout, i: usize) -> bool {
    let no = |sp: Sp| match sp {
        Sp::No => true,
        Sp::One | Sp::Asis => false,
        Sp::Src => !default_space(&l.toks[i - 1], &l.toks[i], true),
    };
    match l.gaps[i].rule {
        Rule::Src => no(Sp::Src),
        Rule::Break(_) => false,
        Rule::Flat(sp)
        | Rule::BreakIf(_, _, sp)
        | Rule::Keep(_, sp)
        | Rule::FlatIn(_, sp)
        | Rule::Fill(_, _, sp) => no(sp),
        Rule::SpaceIf(_, a, b) => no(a) || no(b),
    }
}

/// Whether the gap before `toks[i]`, printed flat with spacing `sp`, gets a
/// space.
fn space(l: &Layout, i: usize, sp: Sp, prev: &Tok, next: &Tok) -> bool {
    let had = l.trivia[i].had_space(l.src_space[i]);
    let sp = match sp {
        Sp::No => false,
        Sp::One => true,
        Sp::Src => default_space(prev, next, had),
        Sp::Asis => had,
    };
    sp || l.apart.get(i).copied().unwrap_or(false)
}

/// Prints a layout. Returns the output lines.
///
/// Alignment padding is applied as the lines are printed, so later tokens'
/// positions (which other indents refer to) include it. The column each
/// alignment group pads to comes from the previous pass; passes repeat until
/// the columns settle.
pub fn print(l: &Layout) -> Vec<String> {
    let mut targets: Vec<Option<usize>> = vec![None; l.align_groups];
    let mut out = Vec::new();
    for _ in 0..4 {
        let (lines, next) = print_pass(l, &targets);
        out = lines;
        if next == targets {
            break;
        }
        targets = next;
    }
    out
}

fn print_pass(l: &Layout, targets: &[Option<usize>]) -> (Vec<String>, Vec<Option<usize>>) {
    let mut p = Printer {
        l,
        out: vec![String::new()],
        targets,
        cells: vec![Vec::new(); l.align_groups],
        pos: vec![(0, 0); l.toks.len()],
        col: 0,
        broken: vec![None; l.groups.len()],
        flat_until: Vec::new(),
        groups_at: {
            let mut v: Vec<Vec<usize>> = vec![Vec::new(); l.toks.len()];
            for (i, g) in l.groups.iter().enumerate() {
                if g.first < v.len() {
                    v[g.first].push(i);
                }
            }
            for gs in &mut v {
                gs.sort_by_key(|&g| std::cmp::Reverse(l.groups[g].last));
            }
            v
        },
        blank_end: false,
        protected: Default::default(),
        raw_base: (0, 0),
    };
    p.run();
    let next = p.next_targets();
    (p.finish(), next)
}

struct Printer<'a> {
    l: &'a Layout,
    out: Vec<String>,
    /// Alignment columns from the previous pass.
    targets: &'a [Option<usize>],
    /// Alignment cells printed in this pass, per group: (line, column the
    /// cell would have without padding).
    cells: Vec<Vec<(usize, usize)>>,
    /// Printed (line, column) of each token.
    pos: Vec<(usize, usize)>,
    /// Current column (chars) on the last line.
    col: usize,
    /// Per group: broken?
    broken: Vec<Option<bool>>,
    /// Last tokens of the flat groups we're inside.
    flat_until: Vec<usize>,
    groups_at: Vec<Vec<usize>>,
    /// The batch ends with a blank line.
    blank_end: bool,
    /// Output lines whose trailing whitespace must stay.
    protected: std::collections::HashSet<usize>,
    /// The last raw region printed: (output line, source line) of its start.
    raw_base: (usize, usize),
}

enum Decision {
    Flat(bool),
    Break(usize),
}

impl Printer<'_> {
    fn run(&mut self) {
        let n = self.l.toks.len();
        for i in 0..n {
            if self.l.raw[i] {
                // Printed whole by the Raw item in the gap before the
                // region's first token; record a position for references.
                if !self.l.trivia[i].items.is_empty() {
                    let pending = usize::from(i == 0);
                    self.trivia(i, None, self.l.toks[i].col, pending);
                }
                let t = &self.l.toks[i];
                self.pos[i] = (
                    self.raw_base.0 + t.line.saturating_sub(self.raw_base.1),
                    t.col,
                );
                continue;
            }
            self.gap(i);
            self.flat_until.retain(|&last| last >= i);
            for gi in 0..self.groups_at[i].len() {
                let g = self.groups_at[i][gi];
                let b = self.flat_until.is_empty() && self.group_broken(g);
                self.broken[g] = Some(b);
                if !b {
                    self.flat_until.push(self.l.groups[g].last);
                }
            }
            self.token(i);
        }
        self.trailing();
    }

    fn line(&self) -> usize {
        self.out.len() - 1
    }

    fn at_line_start(&self) -> bool {
        self.out.last().unwrap().trim().is_empty()
    }

    fn push_str(&mut self, s: &str) {
        self.push_text(s, false);
    }

    /// Appends text that may span lines. Lines that end inside it keep
    /// their trailing whitespace (it's part of a comment or string), and all
    /// of a `verbatim` text's lines do.
    fn push_text(&mut self, s: &str, verbatim: bool) {
        let mut parts = s.split('\n');
        let first = parts.next().unwrap();
        self.out.last_mut().unwrap().push_str(first);
        self.col += first.chars().count();
        for part in parts {
            self.protected.insert(self.out.len() - 1);
            self.out.push(part.to_string());
            self.col = part.chars().count();
        }
        if verbatim {
            self.protected.insert(self.out.len() - 1);
        }
    }

    fn trim_last(&mut self) {
        let n = self.out.len() - 1;
        if !self.protected.contains(&n) {
            let last = &mut self.out[n];
            let t = last.trim_end_matches([' ', '\t']).len();
            last.truncate(t);
        }
    }

    /// Moves to the start of a line indented `indent`: a new line, or the
    /// current one if nothing is on it yet. With `blank`, makes sure a blank
    /// line comes before it (not at the very start of the batch output when
    /// the source had none).
    fn break_to(&mut self, blank: bool, indent: usize) {
        if self.at_line_start() && !self.protected.contains(&(self.out.len() - 1)) {
            self.out.last_mut().unwrap().clear();
        } else {
            self.trim_last();
            self.out.push(String::new());
        }
        let n = self.out.len();
        if blank && (n < 2 || !self.out[n - 2].is_empty()) {
            self.out.insert(n - 1, String::new());
        }
        self.out.last_mut().unwrap().push_str(&" ".repeat(indent));
        self.col = indent;
    }

    fn indent(&self, ind: Ind, next: usize) -> usize {
        let v: isize = match ind {
            Ind::Col(t, off) => self.pos[t].1 as isize + off,
            Ind::LineOf(t, off) => {
                let line = &self.out[self.pos[t].0];
                (line.len() - line.trim_start().len()) as isize + off
            }
            Ind::Src(a) => return self.src_indent(a, self.l.toks[next].col),
            Ind::Abs(c) => c as isize,
        };
        v.max(0) as usize
    }

    /// Source-relative indent for something at source column `col`: as far
    /// right of the anchor as it was in the source; or, if it was left of
    /// the anchor, as far right of the anchor's line's indent.
    fn src_indent(&self, anchor: usize, col: usize) -> usize {
        let a = &self.l.toks[anchor];
        let v = if col >= a.col {
            self.pos[anchor].1 as isize + col as isize - a.col as isize
        } else {
            let line = &self.out[self.pos[anchor].0];
            let out_indent = line.len() - line.trim_start().len();
            let src_indent = self.l.line_indent.get(a.line).copied().unwrap_or(0);
            out_indent as isize + col as isize - src_indent as isize
        };
        v.max(0) as usize
    }

    fn decision(&self, i: usize) -> Decision {
        let l = self.l;
        let gap = &l.gaps[i];
        let nl = l.trivia[i].newlines() > 0;
        let (prev, next) = (&l.toks[i - 1], &l.toks[i]);
        let flat = |sp: Sp| Decision::Flat(space(l, i, sp, prev, next));
        match gap.rule {
            Rule::Src => {
                if nl {
                    Decision::Break(self.indent(Ind::Src(gap.anchor), i))
                } else {
                    flat(Sp::Src)
                }
            }
            Rule::Flat(sp) => flat(sp),
            Rule::Break(ind) => Decision::Break(self.indent(ind, i)),
            Rule::BreakIf(g, ind, sp) => {
                if self.broken[g].unwrap_or(true) {
                    Decision::Break(self.indent(ind, i))
                } else {
                    flat(sp)
                }
            }
            Rule::Keep(ind, sp) => {
                if nl {
                    Decision::Break(self.indent(ind, i))
                } else {
                    flat(sp)
                }
            }
            Rule::FlatIn(_, sp) => flat(sp),
            Rule::SpaceIf(g, b, f) => flat(if self.broken[g].unwrap_or(true) { b } else { f }),
            Rule::Fill(g, ind, sp) => {
                // A kept line break may sit on the other side of a comma:
                // after it for a fill gap before the comma (leading
                // commas), before it for a fill gap after it (trailing).
                let comma = |k: usize| matches!(l.toks[k].token, Token::Comma);
                let kept_at = |k: usize| {
                    matches!(l.gaps.get(k).map(|g| g.rule), Some(Rule::FlatIn(gg, _)) if gg == g)
                        && l.trivia[k].newlines() > 0
                };
                let nl_after = comma(i) && kept_at(i + 1);
                let nl_before = !comma(i) && comma(i - 1) && kept_at(i - 1);
                if self.broken[g] == Some(false) {
                    flat(sp)
                } else if (l.groups[g].keep && (nl || nl_after || nl_before))
                    || !self.chunk_fits(i, g, sp)
                {
                    Decision::Break(self.indent(ind, i))
                } else {
                    flat(sp)
                }
            }
        }
    }

    /// Whether the tokens from `i` up to the next `Fill` gap of group `g`
    /// (or the group's end and what follows it) fit on the current line.
    fn chunk_fits(&self, i: usize, g: usize, sp: Sp) -> bool {
        let l = self.l;
        let mut width = self.col + usize::from(sp == Sp::One);
        let last = l.groups[g].last;
        let mut k = i;
        loop {
            if k >= l.toks.len() || l.raw[k] {
                return true;
            }
            if k > i {
                if matches!(l.gaps[k].rule, Rule::Fill(gg, ..) if gg == g) {
                    return true;
                }
                if k > last
                    && !matches!(l.gaps[k].rule, Rule::Flat(_) | Rule::Src | Rule::FlatIn(..))
                {
                    return true;
                }
                // An element that will be broken anyway (a group the author
                // wrote over several lines, or one that's always broken)
                // starts a line of its own, as it does when it's too long:
                // the same either way, so formatting again changes nothing.
                if let Rule::BreakIf(gg, ..) | Rule::Fill(gg, ..) = l.gaps[k].rule
                    && gg != g
                    && l.groups[gg].first >= i
                    && ((l.groups[gg].keep && l.trivia[k].newlines() > 0) || l.groups[gg].force)
                {
                    return false;
                }
                match self.flat_spacing(k, g) {
                    None => return true,
                    Some(s) => width = self.pad_to_align(k, width + s),
                }
            }
            width += l.toks[k].width();
            if width > l.margin {
                return false;
            }
            if k > last + 8 {
                return true;
            }
            k += 1;
        }
    }

    /// Prints the gap before token `i`: its comments, then a line break or
    /// spacing.
    fn gap(&mut self, i: usize) {
        let next_col = self.l.toks[i].col;
        if i == 0 {
            // Start of the batch: the batch starts on a new line already.
            self.trivia(0, Some(0), next_col, 1);
            let blank = self.blank_before_token(0, 1);
            self.break_to(blank, 0);
            return;
        }
        let d = match self.decision(i) {
            // After a raw region the next token starts a line.
            Decision::Flat(_) if self.l.raw[i - 1] => {
                Decision::Break(self.indent(Ind::Src(self.l.gaps[i].anchor), i))
            }
            d => d,
        };
        match d {
            Decision::Break(ind) => {
                self.trivia(i, Some(ind), next_col, 0);
                let blank = self.blank_before_token(i, 0);
                self.break_to(blank, ind);
            }
            Decision::Flat(space) => {
                let anchor = self.l.gaps[i].anchor;
                if self.trivia(i, None, next_col, 0) {
                    let ind = self
                        .src_indent(anchor, next_col)
                        .max(self.cur_line_indent());
                    let blank = self.blank_before_token(i, 0);
                    self.break_to(blank, ind);
                    return;
                }
                if space && !self.at_line_start() {
                    self.push_str(" ");
                }
                if let Some((a, start)) = self.l.gaps[i].align {
                    let line = self.line();
                    // A cell counts when its item starts on this line, and
                    // only the first cell of a group on a line.
                    let starts_here = start == usize::MAX || self.pos[start].0 == line;
                    if starts_here && self.cells[a].last().is_none_or(|c| c.0 != line) {
                        self.cells[a].push((line, self.col));
                        if let Some(t) = self.targets[a]
                            && t > self.col
                        {
                            let pad = " ".repeat(t - self.col);
                            self.push_str(&pad);
                        }
                    }
                }
            }
        }
    }

    fn cur_line_indent(&self) -> usize {
        let l = self.out.last().unwrap();
        l.len() - l.trim_start().len()
    }

    /// Whether the source had a blank line right before token `i` (after
    /// its gap's last comment). `initial` counts a line break assumed before
    /// the gap (at the start of the batch).
    fn blank_before_token(&self, i: usize, initial: usize) -> bool {
        let items = &self.l.trivia[i].items;
        let n = match items.last() {
            Some(TriviaItem::Newlines(n)) => *n,
            _ => 0,
        };
        let initial = if items.len() <= 1 { initial } else { 0 };
        n + initial >= 2
    }

    /// Prints the comments and raw regions in gap `i`. `brk` is the indent
    /// of the next token if the gap breaks; `next_col` is the next token's
    /// source column. `pending` counts line breaks assumed before the gap.
    /// Returns whether the next token must start a new line.
    fn trivia(&mut self, i: usize, brk: Option<usize>, next_col: usize, pending: usize) -> bool {
        let items = &self.l.trivia[i].items;
        let anchor = if i < self.l.toks.len() {
            self.l.gaps[i].anchor
        } else {
            0
        };
        let mut pending_nl = pending;
        let mut must = false;
        // Source column where the previous token ended (for trailing
        // comments' spacing).
        let mut prev_end = if i > 0 && i <= self.l.toks.len() {
            let t = &self.l.toks[i - 1];
            if t.end.line as usize == t.line + 1 {
                (t.end.column - 1) as usize
            } else {
                usize::MAX
            }
        } else {
            usize::MAX
        };
        for item in items {
            match item {
                TriviaItem::Newlines(n) => pending_nl += n,
                TriviaItem::Comment {
                    text,
                    line_comment,
                    col,
                    ..
                } => {
                    if pending_nl == 0 && !self.at_line_start() {
                        // Trailing or inline comment: stays on this line, as
                        // far from the code before it as in the source (so
                        // aligned comments stay aligned).
                        let pad = if *line_comment {
                            (*col).saturating_sub(prev_end).max(1)
                        } else {
                            1
                        };
                        self.push_str(&" ".repeat(pad));
                    } else {
                        let ind = match brk {
                            // Keep the comment's offset from the next token.
                            Some(b) => {
                                (b as isize + *col as isize - next_col as isize).max(0) as usize
                            }
                            None if i < self.l.toks.len() => self.src_indent(anchor, *col),
                            None => *col,
                        };
                        self.break_to(pending_nl >= 2, ind);
                    }
                    self.push_str(text);
                    prev_end = usize::MAX;
                    pending_nl = 0;
                    must = *line_comment;
                    if must {
                        self.break_to(false, 0);
                    }
                }
                TriviaItem::Raw(text, src_line) => {
                    self.break_to(pending_nl >= 2, 0);
                    self.out.last_mut().unwrap().clear();
                    self.raw_base = (self.line(), *src_line);
                    self.col = 0;
                    self.push_text(text, true);
                    self.break_to(false, 0);
                    pending_nl = 0;
                    must = true;
                }
            }
        }
        must
    }

    fn token(&mut self, i: usize) {
        self.pos[i] = (self.line(), self.col);
        let t = &self.l.toks[i];
        self.push_str(&t.text);
    }

    /// Comments after the last token, and a blank line before the end of the
    /// batch if the source had one.
    fn trailing(&mut self) {
        let n = self.l.toks.len();
        let pending = usize::from(n == 0);
        self.trivia(n, None, 0, pending);
        if let Some(TriviaItem::Newlines(k)) = self.l.trivia[n].items.last() {
            self.blank_end = *k >= 2;
        }
    }

    fn group_broken(&self, g: usize) -> bool {
        let l = self.l;
        let grp = &l.groups[g];
        if grp.force {
            return true;
        }
        if grp.keep {
            for i in grp.first + 1..=(grp.last + 1).min(l.toks.len() - 1) {
                if let Rule::BreakIf(gg, ..) | Rule::FlatIn(gg, _) | Rule::Fill(gg, ..) =
                    l.gaps[i].rule
                    && gg == g
                    && l.trivia[i].newlines() > 0
                {
                    return true;
                }
            }
        }
        let mut width = self.col;
        for i in grp.first..=grp.last {
            if l.raw[i] {
                return true;
            }
            if i > grp.first {
                match self.flat_spacing(i, g) {
                    None => return true,
                    Some(s) => width = self.pad_to_align(i, width + s),
                }
            }
            let t = &l.toks[i];
            if t.multiline() {
                return true;
            }
            width += t.width();
            if width > l.margin {
                return true;
            }
        }
        // What follows the group up to the next possible line break.
        let mut i = grp.last + 1;
        while i < l.toks.len() && !l.raw[i] {
            match l.gaps[i].rule {
                Rule::Flat(_)
                | Rule::Src
                | Rule::Keep(..)
                | Rule::FlatIn(..)
                | Rule::SpaceIf(..) => {}
                _ => break,
            }
            match self.flat_spacing(i, g) {
                None => break,
                Some(s) => width = self.pad_to_align(i, width + s),
            }
            width += l.toks[i].width();
            if width > l.margin {
                return true;
            }
            i += 1;
        }
        false
    }

    /// The column after gap `i` printed at `col`, with its alignment padding.
    fn pad_to_align(&self, i: usize, col: usize) -> usize {
        match self.l.gaps[i].align {
            Some((a, _)) => self.targets[a].map_or(col, |t| t.max(col)),
            None => col,
        }
    }

    /// Width of gap `i` printed on one line inside group `g`, or `None` if
    /// it can't be.
    fn flat_spacing(&self, i: usize, g: usize) -> Option<usize> {
        let l = self.l;
        let tr = &l.trivia[i];
        if tr.forces_break() {
            return None;
        }
        let nl = tr.newlines() > 0;
        let sp = match l.gaps[i].rule {
            Rule::Src => {
                if nl {
                    return None;
                }
                Sp::Src
            }
            Rule::Flat(sp) => sp,
            Rule::Break(_) => return None,
            Rule::Keep(_, sp) => {
                if nl {
                    return None;
                }
                sp
            }
            Rule::BreakIf(gg, _, sp) | Rule::Fill(gg, _, sp) => {
                if gg != g
                    && self.broken[gg] == Some(true)
                    && l.groups[gg].first < l.groups[g].first
                {
                    return None;
                }
                // A group that's always broken (a block the author wrote
                // over several lines, a `wrap = "always"` list) can't be on
                // one line, so neither can a group around it.
                if gg != g && l.groups[gg].force && matches!(l.gaps[i].rule, Rule::BreakIf(..)) {
                    return None;
                }
                if gg != g && l.groups[gg].keep && nl {
                    return None;
                }
                sp
            }
            Rule::FlatIn(_, sp) => sp,
            Rule::SpaceIf(gg, b, f) => {
                if gg == g || self.broken[gg] == Some(false) {
                    f
                } else if self.broken[gg] == Some(true) {
                    b
                } else {
                    f
                }
            }
        };
        let mut w = usize::from(space(l, i, sp, &l.toks[i - 1], &l.toks[i]));
        for item in &tr.items {
            if let TriviaItem::Comment { text, .. } = item {
                w += 1 + text.chars().count();
            }
        }
        Some(w)
    }

    /// Alignment columns for the next pass: per group, the rightmost
    /// unpadded cell, if the group has cells on more than one line.
    fn next_targets(&self) -> Vec<Option<usize>> {
        self.cells
            .iter()
            .map(|cs| {
                if cs.len() >= 2 {
                    cs.iter().map(|c| c.1).max()
                } else {
                    None
                }
            })
            .collect()
    }

    fn finish(mut self) -> Vec<String> {
        for (i, l) in self.out.iter_mut().enumerate() {
            if !self.protected.contains(&i) {
                let t = l.trim_end_matches([' ', '\t']).len();
                l.truncate(t);
            }
        }
        while self.out.len() > 1 && self.out.last().is_some_and(|l| l.is_empty()) {
            self.out.pop();
        }
        if self.blank_end {
            self.out.push(String::new());
        }
        self.out
    }
}

/// Splits whitespace/comment tokens into trivia items.
pub fn trivia_item(ws: &Whitespace, col: usize, line: usize) -> Option<TriviaItem> {
    match ws {
        Whitespace::Newline => Some(TriviaItem::Newlines(1)),
        Whitespace::Space | Whitespace::Tab => None,
        Whitespace::SingleLineComment { comment, prefix } => Some(TriviaItem::Comment {
            text: format!("{prefix}{comment}"),
            line_comment: true,
            col,
            line,
        }),
        Whitespace::MultiLineComment(c) => Some(TriviaItem::Comment {
            text: format!("/*{c}*/"),
            line_comment: false,
            col,
            line,
        }),
    }
}
