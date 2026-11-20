//! Keyword, function and type case.
//!
//! What a word is comes from the AST wherever the AST says it:
//!
//! - A word that starts an `Ident` is an identifier, left as written (even
//!   `Status`, `Name`, `Value`).
//! - A function call's name (`Expr::Function`) is a function name
//!   (`function_case`) when it's unqualified; a qualified name
//!   (`dbo.fnTotal`) names a schema's function, and is an identifier. So is
//!   the keyword of the functions the grammar has a syntax of its own for
//!   (`cast`, `try_cast`, `convert`, `extract`, `substring`, `trim`,
//!   `position`, `overlay`, `ceil`, `floor`), and the standard's niladic
//!   functions written without parentheses (`current_timestamp`, ...).
//! - The name of a data type (`DataType`, in a column definition, cast,
//!   convert, declaration, parameter, return type or typed literal) is a
//!   type name (`type_case`); its parameters (`max` in `varchar(max)`)
//!   are not.
//! - Every other word the tokenizer knows as a keyword is a keyword
//!   (`keyword_case`); anything else is left alone.
//!
//! Lists remain, per dialect, only for what the AST can't tell (see each):
//! built-in types it has no `DataType` for, T-SQL date parts and `@@`
//! functions, and T-SQL keywords sqlparser's keyword list lacks.

use std::collections::{HashMap, HashSet};
use std::ops::ControlFlow;

use sqlparser::ast::*;
use sqlparser::dialect::GenericDialect;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Location, Span, Token, Tokenizer};

use crate::config::{Config, WordCase};
use crate::layout::{Layout, Tok};

/// T-SQL system data types ("Data types (Transact-SQL)") that sqlparser
/// has no `DataType` for: it parses them as `DataType::Custom`, a name like
/// any user-defined type's, so only the name says they're built in.
const MSSQL_CUSTOM_TYPES: &[&str] = &[
    "cursor",
    "datetime2",
    "datetimeoffset",
    "geography",
    "geometry",
    "hierarchyid",
    "image",
    "money",
    "ntext",
    "rowversion",
    "smalldatetime",
    "smallmoney",
    "sql_variant",
    "sysname",
    "uniqueidentifier",
    "xml",
];

/// PostgreSQL built-in types ("Data Types", Table 8.1; range types, 8.17;
/// object identifier types, 8.19) that sqlparser parses as
/// `DataType::Custom`, for the same reason.
const POSTGRESQL_CUSTOM_TYPES: &[&str] = &[
    "bigserial",
    "box",
    "cidr",
    "circle",
    "daterange",
    "inet",
    "int4range",
    "int8range",
    "line",
    "lseg",
    "macaddr",
    "macaddr8",
    "money",
    "name",
    "numrange",
    "oid",
    "path",
    "pg_lsn",
    "pg_snapshot",
    "point",
    "polygon",
    "regtype",
    "serial",
    "smallserial",
    "tsrange",
    "tstzrange",
    "txid_snapshot",
    "xml",
];

/// The SQL standard's niladic functions, written without parentheses: to
/// the AST some are column references (`Expr::Identifier`).
const NILADIC_FUNCTIONS: &[&str] = &[
    "current_date",
    "current_time",
    "current_timestamp",
    "current_user",
    "localtime",
    "localtimestamp",
    "session_user",
    "system_user",
];

/// T-SQL date parts ("DATEADD (Transact-SQL)": datepart), as the first
/// argument of the functions below: keywords to T-SQL, but parsed as a
/// column reference.
const DATE_PARTS: &[&str] = &[
    "year",
    "yy",
    "yyyy",
    "quarter",
    "qq",
    "q",
    "month",
    "mm",
    "m",
    "dayofyear",
    "dy",
    "y",
    "day",
    "dd",
    "d",
    "week",
    "wk",
    "ww",
    "weekday",
    "dw",
    "w",
    "hour",
    "hh",
    "minute",
    "mi",
    "n",
    "second",
    "ss",
    "s",
    "millisecond",
    "ms",
    "microsecond",
    "mcs",
    "nanosecond",
    "ns",
    "tzoffset",
    "tz",
    "iso_week",
    "isowk",
    "isoww",
];

const DATE_FUNCTIONS: &[&str] = &[
    "dateadd",
    "datediff",
    "datediff_big",
    "datename",
    "datepart",
    "datetrunc",
    "date_bucket",
];

/// Which case setting applies to a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Keyword,
    Function,
    Type,
}

/// Where a data type starts: right after the end of a node (skipping a
/// token, like the `as` of a cast), or before a token.
#[derive(Debug, Clone, Copy)]
enum From {
    After(Location, Option<&'static str>),
    /// Right before a token (all of the type).
    Before(Location),
    /// After the first `word` at or after a location.
    AfterWord(Location, &'static str),
}

/// What the AST says about the words of a batch.
#[derive(Default)]
struct Words {
    /// Starts of identifiers.
    idents: HashSet<Location>,
    /// Identifiers cased all the same: function names, built-in type names
    /// the AST only knows as names.
    marked: HashMap<Location, Class>,
    /// Keyword-functions (`cast`, `extract`, ...): their first token.
    functions: Vec<Location>,
    /// Data types and where they start.
    types: Vec<(From, DataType)>,
    /// The dialect's built-in types that parse as `DataType::Custom`.
    custom_types: &'static [&'static str],
    mssql: bool,
}

impl Words {
    fn data_type(&mut self, from: From, dt: &DataType) {
        if let DataType::Custom(name, _) = dt
            && let [ObjectNamePart::Identifier(id)] = name.0.as_slice()
            && id.quote_style.is_none()
            && self
                .custom_types
                .contains(&id.value.to_ascii_lowercase().as_str())
        {
            self.marked.insert(id.span.start, Class::Type);
        }
        self.types.push((from, dt.clone()));
        // A table type's columns (`declare @t table (...)`, `returns @t
        // table (...)`).
        if let DataType::Table(Some(cols)) = dt {
            self.columns(cols);
        }
    }

    fn columns(&mut self, cols: &[ColumnDef]) {
        for c in cols {
            self.data_type(after(c.name.span, None), &c.data_type);
        }
    }
}

fn after(span: Span, skip: Option<&'static str>) -> From {
    From::After(span.end, skip)
}

impl Visitor for Words {
    type Break = ();

    fn pre_visit_ident(&mut self, ident: &Ident) -> ControlFlow<()> {
        if ident.span.start.line > 0 {
            self.idents.insert(ident.span.start);
            // T-SQL reserves `@@` names for its system functions
            // ("Database identifiers": "names that start with @@").
            if self.mssql && ident.quote_style.is_none() && ident.value.starts_with("@@") {
                self.marked.insert(ident.span.start, Class::Function);
            }
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<()> {
        match expr {
            Expr::Function(f) => {
                if let [ObjectNamePart::Identifier(id)] = f.name.0.as_slice()
                    && id.quote_style.is_none()
                {
                    self.marked.insert(id.span.start, Class::Function);
                    let name = id.value.to_ascii_lowercase();
                    if self.mssql
                        && DATE_FUNCTIONS.contains(&name.as_str())
                        && let FunctionArguments::List(list) = &f.args
                        && let Some(FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Identifier(
                            part,
                        )))) = list.args.first()
                        && part.quote_style.is_none()
                        && DATE_PARTS.contains(&part.value.to_ascii_lowercase().as_str())
                    {
                        self.marked.insert(part.span.start, Class::Keyword);
                    }
                }
            }
            Expr::Identifier(id)
                if id.quote_style.is_none()
                    && NILADIC_FUNCTIONS.contains(&id.value.to_ascii_lowercase().as_str()) =>
            {
                self.marked.insert(id.span.start, Class::Function);
            }
            Expr::Cast {
                kind,
                expr: inner,
                data_type,
                ..
            } => {
                let skip = match kind {
                    CastKind::DoubleColon => "::",
                    _ => {
                        self.functions.push(expr.span().start);
                        "as"
                    }
                };
                self.data_type(after(inner.span(), Some(skip)), data_type);
            }
            Expr::Convert {
                expr: inner,
                data_type,
                target_before_value,
                ..
            } => {
                self.functions.push(expr.span().start);
                if let Some(dt) = data_type {
                    let from = if *target_before_value {
                        // `convert(type, value)`: after the `(`.
                        From::AfterWord(expr.span().start, "(")
                    } else {
                        after(inner.span(), Some(","))
                    };
                    self.data_type(from, dt);
                }
            }
            Expr::Extract { .. }
            | Expr::Ceil { .. }
            | Expr::Floor { .. }
            | Expr::Position { .. }
            | Expr::Substring { .. }
            | Expr::Trim { .. }
            | Expr::Overlay { .. } => self.functions.push(expr.span().start),
            Expr::TypedString(ts) => {
                // The span is the literal's; the type is right before it.
                self.data_type(From::Before(ts.value.span.start), &ts.data_type)
            }
            _ => {}
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_statement(&mut self, stmt: &Statement) -> ControlFlow<()> {
        match stmt {
            Statement::CreateTable(ct) => self.columns(&ct.columns),
            Statement::CreateType {
                representation: Some(r),
                ..
            } => match r {
                UserDefinedTypeRepresentation::Table { columns, .. } => self.columns(columns),
                UserDefinedTypeRepresentation::Composite { attributes } => {
                    for a in attributes {
                        self.data_type(after(a.name.span, None), &a.data_type);
                    }
                }
                _ => {}
            },
            Statement::CreateSequence {
                name,
                sequence_options,
                ..
            } => {
                for o in sequence_options {
                    if let SequenceOptions::DataType(dt) = o {
                        self.data_type(From::AfterWord(name.span().end, "as"), dt);
                    }
                }
            }
            Statement::AlterTable(at) => {
                for op in &at.operations {
                    if let AlterTableOperation::AddColumn { column_def, .. } = op {
                        self.columns(std::slice::from_ref(column_def));
                    }
                }
            }
            Statement::Declare { stmts, .. } => {
                for d in stmts {
                    if let (Some(dt), Some(name)) = (&d.data_type, d.names.last()) {
                        self.data_type(after(name.span, Some("as")), dt);
                    }
                }
            }
            Statement::CreateProcedure {
                params: Some(params),
                ..
            } => {
                for p in params {
                    self.data_type(after(p.name.span, Some("as")), &p.data_type);
                }
            }
            Statement::CreateFunction(f) => {
                for a in f.args.iter().flatten() {
                    if let Some(name) = &a.name {
                        self.data_type(after(name.span, None), &a.data_type);
                    }
                }
                if let Some(FunctionReturnType::DataType(dt) | FunctionReturnType::SetOf(dt)) =
                    &f.return_type
                {
                    // After `returns` (`returns setof`).
                    let end = f.args.iter().flatten().last().map_or(f.name.span(), |a| {
                        a.name.as_ref().map_or(Span::empty(), |n| n.span)
                    });
                    self.types
                        .push((From::AfterWord(end.end, "returns"), dt.clone()));
                }
            }
            _ => {}
        }
        ControlFlow::Continue(())
    }
}

/// T-SQL keywords sqlparser's keyword list lacks: the grammar reads them as
/// words, so the tokenizer doesn't mark them as keywords: SET options,
/// table hints, index and column options, `goto`, the trigger tables.
const MSSQL_EXTRA_KEYWORDS: &[&str] = &[
    "allow_page_locks",
    "allow_row_locks",
    "ansi_nulls",
    "ansi_padding",
    "ansi_warnings",
    "arithabort",
    "concat_null_yields_null",
    "data_compression",
    "datefirst",
    "deadlock_priority",
    "deleted",
    "filestream",
    "fillfactor",
    "goto",
    "holdlock",
    "ignore_dup_key",
    "inserted",
    "lock_timeout",
    "maxdop",
    "nocount",
    "nolock",
    "numeric_roundabort",
    "online",
    "pad_index",
    "paglock",
    "quoted_identifier",
    "readcommitted",
    "readpast",
    "readuncommitted",
    "recompile",
    "repeatableread",
    "rowcount",
    "rowguidcol",
    "rowlock",
    "schemabinding",
    "sort_in_tempdb",
    "sparse",
    "statistics_norecompute",
    "tablock",
    "tablockx",
    "textsize",
    "updlock",
    "xact_abort",
    "xlock",
];

/// The token range of a data type that starts at `from`: as many tokens as
/// match the type's `Display`, case aside (at least one).
fn type_range(
    l: &Layout,
    starts: &[Location],
    from: From,
    dt: &DataType,
) -> Option<(usize, usize)> {
    let n = l.toks.len();
    let first_at = |loc: Location| starts.partition_point(|s| crate::build::loc_lt(*s, loc));
    let is = |i: usize, s: &str| i < n && l.toks[i].text.eq_ignore_ascii_case(s);
    let want: Vec<Token> = Tokenizer::new(&GenericDialect {}, &dt.to_string())
        .tokenize()
        .ok()?
        .into_iter()
        .filter(|t| !matches!(t, Token::Whitespace(_) | Token::EOF))
        .collect();
    let start = match from {
        // The whole type must match then.
        From::Before(loc) => first_at(loc).checked_sub(want.len())?,
        From::After(loc, skip) => {
            let i = first_at(loc);
            if skip.is_some_and(|s| is(i, s)) {
                i + 1
            } else {
                i
            }
        }
        From::AfterWord(loc, word) => {
            let i = (first_at(loc)..n).find(|&i| is(i, word))? + 1;
            if is(i, "setof") { i + 1 } else { i }
        }
    };
    let same = |t: &Tok, w: &Token| match (&t.token, w) {
        (Token::Word(a), Token::Word(b)) => {
            a.quote_style == b.quote_style && a.value.eq_ignore_ascii_case(&b.value)
        }
        (a, b) => a == b,
    };
    let len = want
        .iter()
        .zip(l.toks.get(start..)?)
        .take_while(|(w, t)| same(t, w))
        .count();
    (len > 0).then(|| (start, start + len - 1))
}

/// Sets the case of keywords, function names and type names in `l.toks`
/// (`keyword_case`, `function_case`, `type_case`).
pub fn apply(l: &mut Layout, stmts: &[Statement], cfg: &Config) {
    let dialect = cfg.dialect_name();
    let mut w = Words {
        custom_types: match dialect {
            "mssql" => MSSQL_CUSTOM_TYPES,
            "postgresql" => POSTGRESQL_CUSTOM_TYPES,
            _ => &[],
        },
        mssql: dialect == "mssql",
        ..Words::default()
    };
    for s in stmts {
        let _ = s.visit(&mut w);
    }
    let n = l.toks.len();
    let starts: Vec<Location> = l.toks.iter().map(|t| t.start).collect();
    let mut class: Vec<Option<Class>> = vec![None; n];
    let paren = |i: usize| matches!(l.toks.get(i).map(|t| &t.token), Some(Token::LParen));
    for loc in &w.functions {
        // The keyword starts the expression's span; but the spans of some
        // (`extract`, `ceil`, `position`, ...) cover only what's inside the
        // parentheses: then it's the word before the `(` around them.
        let i = starts.partition_point(|s| crate::build::loc_lt(*s, *loc));
        let k = if paren(i + 1) {
            Some(i)
        } else {
            let mut depth = 0usize;
            (0..i.min(n))
                .rev()
                .find(|&j| match l.toks[j].token {
                    Token::RParen => {
                        depth += 1;
                        false
                    }
                    Token::LParen if depth == 0 => true,
                    Token::LParen => {
                        depth -= 1;
                        false
                    }
                    _ => false,
                })
                .and_then(|j| j.checked_sub(1))
        };
        if let Some(k) = k
            && k < n
            && matches!(&l.toks[k].token, Token::Word(w) if w.quote_style.is_none())
            && paren(k + 1)
        {
            class[k] = Some(Class::Function);
        }
    }
    for (from, dt) in &w.types {
        let Some((a, b)) = type_range(l, &starts, *from, dt) else {
            continue;
        };
        let mut depth = 0usize;
        for (t, c) in l.toks[a..=b].iter().zip(&mut class[a..=b]) {
            match t.token {
                Token::LParen => depth += 1,
                Token::RParen => depth = depth.saturating_sub(1),
                Token::Word(_) if depth == 0 && c.is_none() => *c = Some(Class::Type),
                _ => {}
            }
        }
    }
    for i in 0..n {
        let t = &l.toks[i];
        let Token::Word(word) = &t.token else {
            continue;
        };
        if word.quote_style.is_some() {
            continue;
        }
        let class = if w.idents.contains(&t.start) {
            w.marked.get(&t.start).copied()
        } else if let Some(c) = class[i] {
            Some(c)
        } else if w.marked.get(&t.start) == Some(&Class::Keyword)
            || word.keyword != Keyword::NoKeyword
            || (w.mssql && MSSQL_EXTRA_KEYWORDS.contains(&word.value.to_ascii_lowercase().as_str()))
        {
            Some(Class::Keyword)
        } else {
            None
        };
        let case = match class {
            Some(Class::Keyword) => cfg.keyword_case,
            Some(Class::Function) => cfg.function_case,
            Some(Class::Type) => cfg.type_case,
            None => continue,
        };
        let t = &mut l.toks[i];
        let new = match case {
            WordCase::Lower => t.text.to_ascii_lowercase(),
            WordCase::Upper => t.text.to_ascii_uppercase(),
            WordCase::AsIs => continue,
        };
        if new != t.text {
            t.text = new;
            t.case_changed = true;
        }
    }
}
