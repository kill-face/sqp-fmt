//! Keyword, built-in function and system type case.
//!
//! A word is an identifier when it starts an `Ident` of the AST: those are
//! left as written (even `Status`, `Name`, `Value`), except the names of
//! built-in functions, system data types and `@@` system functions, which
//! are cased like keywords. Every other unquoted word is a keyword (or a
//! type or function name that the grammar knows) and is cased if it's one of
//! sqlparser's keywords or on the lists below; anything else is left alone.
//! Keywords, functions and types each take their own setting
//! (`keyword_case`, `function_case`, `type_case`; lower case by default).

use std::collections::{HashMap, HashSet};
use std::ops::ControlFlow;

use sqlparser::ast::*;
use sqlparser::keywords::Keyword;
use sqlparser::tokenizer::{Location, Token};

use crate::config::{Config, WordCase};
use crate::layout::Layout;

/// T-SQL built-in functions (scalar, aggregate, ranking, analytic, rowset).
pub const BUILTIN_FUNCTIONS: &[&str] = &[
    // aggregate
    "approx_count_distinct",
    "approx_percentile_cont",
    "approx_percentile_disc",
    "avg",
    "checksum_agg",
    "count",
    "count_big",
    "grouping",
    "grouping_id",
    "max",
    "min",
    "stdev",
    "stdevp",
    "string_agg",
    "sum",
    "var",
    "varp",
    // analytic and ranking
    "cume_dist",
    "dense_rank",
    "first_value",
    "lag",
    "last_value",
    "lead",
    "ntile",
    "percent_rank",
    "percentile_cont",
    "percentile_disc",
    "rank",
    "row_number",
    // conversion
    "cast",
    "convert",
    "parse",
    "try_cast",
    "try_convert",
    "try_parse",
    // date and time
    "current_timestamp",
    "current_timezone",
    "current_timezone_id",
    "date_bucket",
    "dateadd",
    "datediff",
    "datediff_big",
    "datefromparts",
    "datename",
    "datepart",
    "datetime2fromparts",
    "datetimefromparts",
    "datetimeoffsetfromparts",
    "datetrunc",
    "day",
    "eomonth",
    "getdate",
    "getutcdate",
    "isdate",
    "month",
    "smalldatetimefromparts",
    "switchoffset",
    "sysdatetime",
    "sysdatetimeoffset",
    "sysutcdatetime",
    "timefromparts",
    "todatetimeoffset",
    "year",
    // logical
    "choose",
    "greatest",
    "iif",
    "least",
    // math
    "abs",
    "acos",
    "asin",
    "atan",
    "atn2",
    "ceiling",
    "cos",
    "cot",
    "degrees",
    "exp",
    "floor",
    "log",
    "log10",
    "pi",
    "power",
    "radians",
    "rand",
    "round",
    "sign",
    "sin",
    "sqrt",
    "square",
    "tan",
    // string
    "ascii",
    "char",
    "charindex",
    "concat",
    "concat_ws",
    "difference",
    "format",
    "left",
    "len",
    "lower",
    "ltrim",
    "nchar",
    "patindex",
    "quotename",
    "replace",
    "replicate",
    "reverse",
    "right",
    "rtrim",
    "soundex",
    "space",
    "str",
    "string_escape",
    "string_split",
    "stuff",
    "substring",
    "translate",
    "trim",
    "unicode",
    "upper",
    // json and xml
    "isjson",
    "json_array",
    "json_modify",
    "json_object",
    "json_query",
    "json_value",
    "openjson",
    "openxml",
    // system, metadata, security
    "app_name",
    "binary_checksum",
    "checksum",
    "col_length",
    "col_name",
    "columnproperty",
    "columns_updated",
    "compress",
    "connectionproperty",
    "context_info",
    "current_request_id",
    "current_transaction_id",
    "current_user",
    "cursor_status",
    "databasepropertyex",
    "datalength",
    "db_id",
    "db_name",
    "decompress",
    "error_line",
    "error_message",
    "error_number",
    "error_procedure",
    "error_severity",
    "error_state",
    "eventdata",
    "formatmessage",
    "get_filestream_transaction_context",
    "getansinull",
    "has_dbaccess",
    "has_perms_by_name",
    "hashbytes",
    "host_id",
    "host_name",
    "ident_current",
    "ident_incr",
    "ident_seed",
    "identity",
    "index_col",
    "indexkey_property",
    "indexproperty",
    "is_member",
    "is_rolemember",
    "is_srvrolemember",
    "isnull",
    "isnumeric",
    "min_active_rowversion",
    "newid",
    "newsequentialid",
    "nullif",
    "coalesce",
    "object_definition",
    "object_id",
    "object_name",
    "object_schema_name",
    "objectproperty",
    "objectpropertyex",
    "opendatasource",
    "openquery",
    "openrowset",
    "original_login",
    "parsename",
    "permissions",
    "pwdcompare",
    "pwdencrypt",
    "rowcount_big",
    "schema_id",
    "schema_name",
    "scope_identity",
    "serverproperty",
    "session_context",
    "session_user",
    "sessionproperty",
    "sql_variant_property",
    "stats_date",
    "suser_id",
    "suser_name",
    "suser_sid",
    "suser_sname",
    "system_user",
    "trigger_nestlevel",
    "type_id",
    "type_name",
    "typeproperty",
    "user_id",
    "user_name",
    "xact_state",
];

/// System data types.
pub const SYSTEM_TYPES: &[&str] = &[
    "bigint",
    "binary",
    "bit",
    "char",
    "cursor",
    "date",
    "datetime",
    "datetime2",
    "datetimeoffset",
    "decimal",
    "float",
    "geography",
    "geometry",
    "hierarchyid",
    "image",
    "int",
    "money",
    "nchar",
    "ntext",
    "numeric",
    "nvarchar",
    "real",
    "rowversion",
    "smalldatetime",
    "smallint",
    "smallmoney",
    "sql_variant",
    "sysname",
    "table",
    "text",
    "time",
    "timestamp",
    "tinyint",
    "uniqueidentifier",
    "varbinary",
    "varchar",
    "xml",
];

/// `@@` system functions.
pub const SYSTEM_VARIABLES: &[&str] = &[
    "@@connections",
    "@@cpu_busy",
    "@@cursor_rows",
    "@@datefirst",
    "@@dbts",
    "@@error",
    "@@fetch_status",
    "@@identity",
    "@@idle",
    "@@io_busy",
    "@@langid",
    "@@language",
    "@@lock_timeout",
    "@@max_connections",
    "@@max_precision",
    "@@microsoftversion",
    "@@nestlevel",
    "@@options",
    "@@pack_received",
    "@@pack_sent",
    "@@packet_errors",
    "@@procid",
    "@@remserver",
    "@@rowcount",
    "@@servername",
    "@@servicename",
    "@@spid",
    "@@textsize",
    "@@timeticks",
    "@@total_errors",
    "@@total_read",
    "@@total_write",
    "@@trancount",
    "@@version",
];

/// Date parts, as the first argument of `dateadd` and friends.
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

/// Words outside identifiers that are T-SQL keywords sqlparser doesn't list.
const EXTRA_KEYWORDS: &[&str] = &[
    "nocount",
    "xact_abort",
    "ansi_nulls",
    "quoted_identifier",
    "ansi_padding",
    "ansi_warnings",
    "arithabort",
    "concat_null_yields_null",
    "numeric_roundabort",
    "identity_insert",
    "datefirst",
    "dateformat",
    "deadlock_priority",
    "lock_timeout",
    "rowcount",
    "textsize",
    "language",
    "nonclustered",
    "clustered",
    "columnstore",
    "readonly",
    "schemabinding",
    "recompile",
    "encryption",
    "nolock",
    "readuncommitted",
    "readcommitted",
    "repeatableread",
    "serializable",
    "snapshot",
    "updlock",
    "rowlock",
    "paglock",
    "tablock",
    "tablockx",
    "holdlock",
    "xlock",
    "readpast",
    "nowait",
    "maxdop",
    "optimize",
    "unknown",
    "fillfactor",
    "pad_index",
    "sort_in_tempdb",
    "ignore_dup_key",
    "statistics_norecompute",
    "allow_row_locks",
    "allow_page_locks",
    "online",
    "data_compression",
    "nocheck",
    "persisted",
    "sparse",
    "rowguidcol",
    "filestream",
    "output",
    "out",
    "inserted",
    "deleted",
    "try",
    "catch",
    "tran",
    "raiserror",
    "waitfor",
    "delay",
    "nowait",
    "goto",
    "openjson",
    "cross",
    "apply",
    "pivot",
    "unpivot",
    "merge",
    "matched",
    "target",
    "source",
    "throw",
    "print",
    "exec",
    "execute",
    "proc",
    "procedure",
    "returns",
    "return",
    "begin",
    "end",
];

fn set(list: &[&str]) -> HashSet<String> {
    list.iter().map(|s| s.to_string()).collect()
}

struct Lists {
    functions: HashSet<String>,
    types: HashSet<String>,
    vars: HashSet<String>,
    extra: HashSet<String>,
}

fn lists() -> &'static Lists {
    static L: std::sync::OnceLock<Lists> = std::sync::OnceLock::new();
    L.get_or_init(|| Lists {
        functions: set(BUILTIN_FUNCTIONS),
        types: set(SYSTEM_TYPES),
        vars: set(SYSTEM_VARIABLES),
        extra: set(EXTRA_KEYWORDS),
    })
}

/// Identifier starts, and the identifiers that are nevertheless lower-cased
/// (built-in function names, system types).
#[derive(Default)]
struct Idents {
    idents: HashSet<Location>,
    lower: HashMap<Location, Class>,
}

/// Which case setting applies to a word.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Keyword,
    Function,
    Type,
}

impl Idents {
    fn data_type(&mut self, dt: &DataType) {
        if let DataType::Custom(name, _) = dt
            && let [ObjectNamePart::Identifier(id)] = name.0.as_slice()
            && id.quote_style.is_none()
            && lists().types.contains(&id.value.to_ascii_lowercase())
        {
            self.lower.insert(id.span.start, Class::Type);
        }
    }

    fn function_name(&mut self, name: &ObjectName) {
        if let [ObjectNamePart::Identifier(id)] = name.0.as_slice()
            && id.quote_style.is_none()
            && lists().functions.contains(&id.value.to_ascii_lowercase())
        {
            self.lower.insert(id.span.start, Class::Function);
        }
    }
}

impl Visitor for Idents {
    type Break = ();

    fn pre_visit_ident(&mut self, ident: &Ident) -> ControlFlow<()> {
        if ident.span.start.line > 0 {
            self.idents.insert(ident.span.start);
            if ident.quote_style.is_none()
                && lists().vars.contains(&ident.value.to_ascii_lowercase())
            {
                self.lower.insert(ident.span.start, Class::Function);
            }
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_expr(&mut self, expr: &Expr) -> ControlFlow<()> {
        match expr {
            Expr::Function(f) => {
                self.function_name(&f.name);
                let name = f.name.to_string().to_ascii_lowercase();
                if DATE_FUNCTIONS.contains(&name.as_str())
                    && let FunctionArguments::List(list) = &f.args
                    && let Some(FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Identifier(id)))) =
                        list.args.first()
                    && id.quote_style.is_none()
                    && DATE_PARTS.contains(&id.value.to_ascii_lowercase().as_str())
                {
                    self.lower.insert(id.span.start, Class::Keyword);
                }
            }
            Expr::Identifier(id) => {
                // Niladic built-ins written without parentheses.
                let v = id.value.to_ascii_lowercase();
                if id.quote_style.is_none()
                    && matches!(
                        v.as_str(),
                        "current_timestamp" | "current_user" | "session_user" | "system_user"
                    )
                {
                    self.lower.insert(id.span.start, Class::Function);
                }
            }
            Expr::Cast { data_type, .. } => self.data_type(data_type),
            Expr::Convert {
                data_type: Some(dt),
                ..
            } => self.data_type(dt),
            _ => {}
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_statement(&mut self, stmt: &Statement) -> ControlFlow<()> {
        match stmt {
            Statement::CreateTable(ct) => {
                for c in &ct.columns {
                    self.data_type(&c.data_type);
                }
            }
            Statement::AlterTable(at) => {
                for op in &at.operations {
                    if let AlterTableOperation::AddColumn { column_def, .. } = op {
                        self.data_type(&column_def.data_type);
                    }
                }
            }
            Statement::Declare { stmts, .. } => {
                for d in stmts {
                    if let Some(dt) = &d.data_type {
                        self.data_type(dt);
                    }
                }
            }
            Statement::CreateProcedure {
                params: Some(params),
                ..
            } => {
                for p in params {
                    self.data_type(&p.data_type);
                }
            }
            Statement::CreateFunction(f) => {
                for a in f.args.iter().flatten() {
                    self.data_type(&a.data_type);
                }
                if let Some(FunctionReturnType::DataType(dt)) = &f.return_type {
                    self.data_type(dt);
                }
            }
            _ => {}
        }
        ControlFlow::Continue(())
    }
}

/// Sets the case of keywords, built-in functions and system types in
/// `l.toks` (`keyword_case`, `function_case`, `type_case`).
///
/// Identifiers are classed by the AST (a function name, a type name, a date
/// part, an `@@` function). Other words are classed by name and position: a
/// system type name is a type (but `table` only after a `@variable`, `xml`
/// not after `for`, `time` not after `at`); a built-in function name right
/// before `(` is a function (`cast(`, `convert(`; not `identity (1,1)`);
/// every other keyword is a keyword.
pub fn apply(l: &mut Layout, stmts: &[Statement], cfg: &Config) {
    let mut ids = Idents::default();
    for s in stmts {
        let _ = s.visit(&mut ids);
    }
    let lists = lists();
    for i in 0..l.toks.len() {
        let t = &l.toks[i];
        let Token::Word(w) = &t.token else { continue };
        if w.quote_style.is_some() {
            continue;
        }
        let class = if ids.idents.contains(&t.start) {
            ids.lower.get(&t.start).copied()
        } else {
            let v = w.value.to_ascii_lowercase();
            let prev = i.checked_sub(1).map(|p| &l.toks[p]);
            let prev_is = |kw: &str| prev.is_some_and(|p| p.is_word(kw));
            let prev_var = prev.is_some_and(
                |p| matches!(&p.token, Token::Word(pw) if pw.quote_style.is_none() && pw.value.starts_with('@')),
            );
            let next_paren = matches!(l.toks.get(i + 1).map(|n| &n.token), Some(Token::LParen));
            if lists.types.contains(&v)
                && (v != "table" || prev_var)
                && !(v == "xml" && prev_is("for"))
                && !(v == "time" && prev_is("at"))
            {
                Some(Class::Type)
            } else if (lists.functions.contains(&v) && next_paren && v != "identity")
                || lists.vars.contains(&v)
            {
                Some(Class::Function)
            } else if w.keyword != Keyword::NoKeyword
                || lists.types.contains(&v)
                || lists.functions.contains(&v)
                || lists.extra.contains(&v)
            {
                Some(Class::Keyword)
            } else {
                None
            }
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
