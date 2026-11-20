# sqp-fmt

A SQL formatter for every dialect
[sqlparser-rs](https://github.com/apache/datafusion-sqlparser-rs) parses
(through a fork, sqp-rs). It only changes whitespace and keyword case:
a safety net tokenizes each batch's output again and compares it with the
input, and leaves the batch exactly as written on any difference. A file
is read the way its dialect's command-line client reads it (sqlcmd's `GO`,
the mysql client's `DELIMITER`, psql's meta-commands, the sqlite3 shell's
dot-commands, ...), and those are kept as written. The style, a general
one with a section per dialect, is in [STYLE.md](STYLE.md); every setting
is in [docs/config.md](docs/config.md).

## Layout

- `../sqp-rs`: the parser fork (`origin` =
  [`kill-face/sqp-rs`](https://github.com/kill-face/sqp-rs), `upstream` =
  `apache/datafusion-sqlparser-rs`), integrating on its `main` bookmark.
  Each grammar or span fix is its own change with tests, so it can go
  upstream. `Cargo.toml` depends on it by path; the Nix package
  (`flake.nix`) builds against its GitHub `main` instead.
- `src/script.rs`: the script layer. A file split into SQL, client
  commands and client data for its dialect's client, from each client's
  documentation (sqlcmd, mysql, psql, sqlite3 / DuckDB, SnowSQL, BTEQ;
  `generic` has none), and the lexing of SQL with sqlparser's tokenizer for
  the dialect (with the mysql client's delimiter and psql's `\g`-style
  terminators): the one source of lexical truth.
- `src/lib.rs`: the batches of a file and their parse (`check`, `spans`).
- `src/format.rs`: formats a file: line endings and BOM, the script's
  parts, `@formatter:off` regions, one batch at a time; parsing (a sqlcmd
  batch as a whole, else statement by statement); and the safety net
  (below).
- `src/layout.rs`: the layout model and printer. A batch is its tokens, the
  trivia (comments, line breaks) between them, and a rule per gap between two
  tokens (keep the source's line break / one line / break / break with a
  group / fill to the margin), with indents relative to tokens printed
  earlier and alignment columns. The printer only ever writes whitespace
  between the tokens, so it can't drop or reorder anything, and it keeps
  two tokens apart that written together would read as other tokens.
- `src/build/`: sets the rules from the AST (`stmt.rs` statements and
  blocks, `query.rs` queries, `expr.rs` expressions, `dml.rs`, `ddl.rs`).
  Every gap starts as "keep the source layout, re-indented with its
  surroundings"; handlers replace that for the constructs they know, so
  anything unknown keeps its layout. Shared constructs get the same rules
  in every dialect; T-SQL's procedural syntax has rules of its own.
- `src/case.rs`: keyword / function / type case, each word classed by the
  AST (identifiers, function calls, data types), with per-dialect lists
  only for what the AST can't tell.
- `src/config.rs`: the configuration file (`sqp-fmt.toml`): the schema (one
  struct per section, defaults = STYLE.md), `[dialects.<name>]` layering,
  `[[dialect_paths]]`, discovery (upward search, user config), loading,
  and the table of implemented values (anything else is rejected);
  `docs/config.md` is its reference.
- `src/corpus.rs`: the `corpus` progress measure, also over sqlfluff's
  dialect fixtures.
- `src/main.rs`: the `sqp-fmt` command.
- `docs/sqlparser-field-guide.html`: the parser's architecture and how
  sqp-fmt uses it.

## Formatting

`sqp-fmt [--config FILE] [--dialect NAME] [--stdin-filepath PATH] [FILE | -]`
formats FILE (or stdin) to stdout. The layout comes from the first
`sqp-fmt.toml` in FILE's directory or above it (for stdin: above
`--stdin-filepath`'s directory, else the current one), or `--config FILE`,
else `$XDG_CONFIG_HOME/sqp-fmt/sqp-fmt.toml`; with none, the house style
of [STYLE.md](STYLE.md). The dialect is `--dialect`, else the
configuration's `[[dialect_paths]]` entry for the file, else its
`dialect`, else `generic`. [docs/config.md](docs/config.md) has every
key. A bad configuration is an error (exit code 2, the message on stderr
points at the key, nothing on stdout).

Files in place, for scripts, CI and agents:

```sh
sqp-fmt --write PATH...   # format files in place; directories: their .sql files
sqp-fmt --check PATH...   # change nothing; exit 1 if any file would change
```

Both print the files that changed (or would change) on stdout, one per
line, and on stderr a summary plus the files whose batches were left as
written because they don't parse. Each file gets its own `sqp-fmt.toml` and
dialect. Exit codes: 0 done (`--check`: nothing to change), 1 `--check`
found files to change, 2 a file couldn't be found, read, written or
configured. A batch that doesn't parse is not an error: it's left as written.
`--write` replaces a file atomically (a temporary file renamed over it),
keeps its permissions, follows symlinks, and leaves unchanged files
untouched.

For conform.nvim, pass the buffer's path so the right `sqp-fmt.toml` is
found:

```lua
require("conform").setup({
  formatters_by_ft = { sql = { "sqp_fmt" } },
  formatters = {
    sqp_fmt = {
      command = "sqp-fmt",
      args = { "--stdin-filepath", "$FILENAME" },
      stdin = true,
    },
  },
})
```

Safety: each batch's output is tokenized again, with the dialect's
tokenizer and client, and compared with the input's tokens, comments
included; only whitespace may differ, and case only for the words the
formatter re-cased, and no output line may read as a client command. If
they differ, or formatting panics, that batch is emitted exactly as
written. Statements that don't parse (sqlcmd: batches),
`@formatter:off` … `@formatter:on` lines and client commands and data are
left as written too. CRLF / LF, a BOM and a missing final newline are kept
(a file with mixed line endings is only formatted if every `\r` is inside a
comment or string, else left alone).

## Commands

```sh
nix develop                      # or direnv
nix build                        # the package, tests included
cargo run --release -- corpus --dialect mssql corpus/private/     # format the test bed in memory: lines unchanged, fallbacks, idempotency
cargo run --release -- corpus --list corpus/private/              # ... with every changed file
cargo run --release -- corpus --diff FILE                         # unified diff of what formatting FILE changes
cargo run --release -- corpus --sqlfluff corpus/sqlfluff/         # sqlfluff's fixtures, a table per dialect
cargo run --release -- corpus --sqlfluff --errors 5 corpus/sqlfluff/  # ... with each dialect's most common parser errors
cargo test                                                        # golden tests (tests/golden), token and idempotency checks
cargo run --release -- check --dialect mssql corpus/private/      # parse rate + grouped errors
cargo run --release -- check --files corpus/private/              # ... plus every failing statement
cargo run --release -- spans --dialect mssql corpus/private/      # AST span audit, grouped findings
cargo run --release -- spans --details corpus/private/            # ... with every location
```

`corpus/sqlfluff/` is sqlfluff's `test/fixtures/dialects` (`*.sql` only,
MIT, see `corpus/sqlfluff/SOURCE`), git-ignored like all of `corpus/`:
never commit corpus SQL. `cargo test` needs nothing outside the repo.

`spans` checks, for every batch that parses, that the AST's source spans are
complete enough to map every token back to its node (see `src/spans.rs`):
every token but `;` lies in exactly one top-level statement span, which runs
from the statement's first token to its last; statements, queries, selects,
expressions, table factors, joins, functions (with `OVER`), column
definitions and options, constraints and parameters have non-empty spans
inside their parent's, on token boundaries; list items (select items,
columns, arguments, statements in a block) have only separators between
them; and a node's span is not shorter than its `Display`, nor starts or
ends with a different token.

`corpus` formats every file in memory and reports the files and lines left
unchanged, the batches the safety net left as written (should be 0), the
files where formatting twice differs from formatting once (should be 0), the
slowest file and the files with the biggest diffs. Golden tests: each
`tests/golden/DIALECT/NAME.sql` must format to `NAME.expected.sql` as
DIALECT (the directory maps its files to the dialect), with
`NAME.toml` as its configuration if there is one, else the defaults
(`SQP_FMT_BLESS=1 cargo test` rewrites the expected files; review the
diff). `tests/cli.rs` tests the command: finding `sqp-fmt.toml`, `--config`,
`--stdin-filepath`, the user config, dialects, `--write` / `--check`,
`corpus --sqlfluff`, configuration errors.

## Status

The private T-SQL corpus (`corpus/private/`, 510 files; `--dialect mssql`):

| Date | corpus files parsing |
|---|---|
| 2026-11-19 | 263 / 510 (upstream sqlparser-rs v0.64.0) |
| 2026-11-19 | 292 / 510 (fork: `PRIMARY KEY` / `UNIQUE` `CLUSTERED`) |
| 2026-11-19 | 348 / 510 (fork: `CREATE [UNIQUE] [NON]CLUSTERED [COLUMNSTORE] INDEX`) |
| 2026-11-19 | 414 / 510 (fork `tsql`, 20 changes; 476 / 510 with `require_semicolon_stmt_delimiter: false`) |
| 2026-11-19 | 478 / 510 (harness: optional semicolons on, `go;`, `GO` inside comments/strings ignored). The 32 left: 30 placeholder templates that aren't valid SQL (`values ();`, empty `from (values )`, `check ()`), one file of CREATE LOGIN / USER … FOR LOGIN and one of ALTER DATABASE … SET and CREATE DATABASE file lists |

Span audit (`sqp-fmt spans`, same corpus, 1343 parsed batches, 1777
top-level statements, 298605 nested nodes, 687211 tokens):

| Date | Tokens outside statement spans | Empty spans | Findings |
|---|---|---|---|
| 2026-11-19 | 153712 (22.37%) | 3364 | 15166 in 117 groups (fork `tsql` before the span work) |
| 2026-11-19 | 41 (0.01%) | 0 | 96 in 5 groups (fork `tsql` + 18 span changes): the `USE` keyword (41 statements, 2 checks each), the `)` of table hints (12), `WITHIN GROUP (...)` (1, 2 checks) |

Formatter (`sqp-fmt corpus`, same corpus: 510 files, 65365 lines; the
largest file, 15539 lines / 1.4 MB, formats in about
0.33 s). One generated seed file (12712 lines of `values` rows written with
upper-case `NULL` and trailing commas) accounts for most changed lines; the
last column leaves it out.

| Date | Files unchanged | Lines unchanged | Safety fallbacks | Not idempotent | Lines unchanged without the seed file | Step |
|---|---|---|---|---|---|---|
| 2026-11-19 | 317 / 510 | 49825 / 65365 (76.23%) | 0 | 0 | 49787 / 52653 (94.56%) | Core: printer, safety net, case; source layout kept, statements at column 0 |
| 2026-11-19 | 443 / 510 | 52292 / 65365 (80.00%) | 0 | 0 | 52253 / 52653 (99.24%) | Statement families: select / from / join / where, CTEs, subqueries, insert / values / update / delete / merge, case, create table / index / alter table, procedures / functions / views / triggers, declare / set, if / while / try blocks (STYLE.md rules R1-R24, R27) |
| 2026-11-19 | 417 / 510 | 51995 / 65365 (79.55%) | 0 | 0 | 51956 / 52653 (98.68%) | STYLE.md changes 1 and 2 (boolean blocks, window blocks; R25, R26). The changed lines left: the two changes (~320 lines), blank-line runs collapsed (~100), corpus inconsistencies where the majority rule wins (inline named constraints, trailing-comma lists, misaligned columns, `merge … on` indented), and the generated seed file |
| 2026-11-19 | 417 / 510 | 51995 / 65365 (79.55%) | 0 | 0 | 51956 / 52653 (98.68%) | Configuration file (`sqp-fmt.toml`, docs/config.md); no file = the same output as before, byte for byte. A window or group written over several lines now breaks the list or `case` around it (none in the test bed) |
| 2026-11-20 | 416 / 510 | 51503 / 65365 (78.79%) | 0 | 0 | 51464 / 52653 (97.74%) | A CTE's `(` 4 in from `with` also when there's one CTE (was: from its name, as DataGrip does); `where.indent` / `having.indent` and `join_on.indent`. The ~490 lines more that change are single-CTE queries moving left |
| 2026-11-20 | 407 / 510 | 51138 / 65365 (78.23%) | 0 | 0 | 51099 / 52653 (97.05%) | Every offset defaults to `indent` (4); `and exists (`: the `(` 4 in from its `and`: `where` / `having` `and` (was 2), logical-block and window contents (was 2), procedure parameters (was 2), chopped parenthesised lists (was 2); each has its own `indent` key (`<condition>.groups.indent`, `window.indent`, `routine_params.indent`). The corpus mostly uses +2, hence the drop |
| 2026-11-20 | 407 / 510 | 51138 / 65365 (78.23%) | 0 | 0 | 51099 / 52653 (97.05%) | Every dialect: the script layer (each client's syntax kept as written, sqlparser's tokenizer as the one lexer, statement-by-statement parsing outside sqlcmd), case from the AST, T-SQL's procedural rules only for T-SQL, `corpus --sqlfluff`. The output is byte for byte the same as before |

sqlfluff's dialect fixtures (`sqp-fmt corpus --sqlfluff corpus/sqlfluff`,
sqlfluff 4.5.0, 2026-11-20; each sqlfluff dialect formatted as the
sqlparser dialect beside it, the mapping and its reasons in
`SQLFLUFF_DIALECTS`, `src/corpus.rs`). A file parses when every batch or
statement in it does; for the clients that send one statement at a time a
batch is "partly" formatted when some of its statements don't parse (they
are left as written); lines changed = lines not in the longest common
subsequence of input and output. The fixtures aren't in any house style,
so the lines changed only say how far that is from STYLE.md:

| sqlfluff | sqlparser | files | files parsing | batches formatted / partly / not | statements left as written | safety fallbacks | not idempotent | lines changed |
|---|---|---|---|---|---|---|---|---|
| ansi | ansi | 208 | 177 | 176 / 10 / 21 | 21 | 0 | 0 | 1252 / 1644 |
| athena | generic | 50 | 37 | 37 / 11 / 2 | 29 | 0 | 0 | 390 / 676 |
| bigquery | bigquery | 146 | 99 | 99 / 24 / 23 | 83 | 0 | 0 | 1605 / 2742 |
| clickhouse | clickhouse | 64 | 24 | 24 / 24 / 16 | 443 | 0 | 0 | 1004 / 2703 |
| databricks | databricks | 83 | 21 | 21 / 30 / 32 | 235 | 0 | 0 | 760 / 2694 |
| db2 | generic | 23 | 10 | 10 / 7 / 6 | 85 | 0 | 0 | 185 / 902 |
| doris | generic | 47 | 16 | 16 / 0 / 31 | 0 | 0 | 0 | 32 / 337 |
| duckdb | duckdb | 47 | 20 | 20 / 18 / 9 | 106 | 0 | 0 | 554 / 1288 |
| exasol | generic | 60 | 6 | 6 / 20 / 34 | 128 | 0 | 0 | 427 / 938 |
| flink | generic | 25 | 11 | 11 / 6 / 8 | 7 | 0 | 0 | 137 / 294 |
| greenplum | postgresql | 11 | 4 | 4 / 3 / 4 | 4 | 0 | 0 | 53 / 334 |
| hive | hive | 48 | 29 | 29 / 9 / 10 | 21 | 0 | 0 | 225 / 449 |
| impala | generic | 9 | 2 | 2 / 4 / 3 | 12 | 0 | 0 | 56 / 83 |
| mariadb | mysql | 239 | 103 | 111 / 53 / 88 | 277 | 0 | 0 | 1117 / 2469 |
| materialize | postgresql | 16 | 3 | 3 / 9 / 4 | 60 | 0 | 0 | 149 / 417 |
| mysql | mysql | 255 | 105 | 114 / 60 / 94 | 251 | 0 | 0 | 1175 / 2458 |
| oracle | oracle | 106 | 21 | 21 / 54 / 31 | 1328 | 0 | 0 | 1329 / 6823 |
| postgres | postgresql | 175 | 93 | 106 / 49 / 33 | 321 | 0 | 0 | 3192 / 5575 |
| redshift | redshift | 89 | 40 | 40 / 23 / 26 | 174 | 0 | 0 | 940 / 2510 |
| snowflake | snowflake | 271 | 87 | 87 / 66 / 118 | 837 | 0 | 0 | 2409 / 8775 |
| soql | generic | 3 | 1 | 1 / 0 / 2 | 0 | 0 | 0 | 3 / 7 |
| sparksql | spark | 143 | 57 | 57 / 47 / 39 | 216 | 0 | 0 | 2292 / 4227 |
| sqlite | sqlite | 42 | 30 | 30 / 9 / 3 | 34 | 0 | 0 | 669 / 849 |
| starrocks | generic | 18 | 2 | 2 / 1 / 15 | 3 | 0 | 0 | 105 / 358 |
| teradata | teradata | 18 | 7 | 6 / 6 / 5 | 44 | 0 | 0 | 103 / 412 |
| trino | generic | 32 | 21 | 21 / 10 / 1 | 38 | 0 | 0 | 446 / 717 |
| tsql | mssql | 191 | 89 | 256 / 0 / 194 | 0 | 0 | 0 | 2359 / 7149 |
| vertica | generic | 43 | 19 | 19 / 16 / 8 | 79 | 0 | 0 | 773 / 1360 |
| all | | 2462 | 1134 | 1329 / 569 / 860 | 4836 | 0 | 0 | 23741 / 59190 |

The three fallbacks the fixtures had before (`- - 5` printed as `--5`, a
comment, in ansi and sqlite; the data of `COPY ... FROM stdin` read as
SQL in postgres) went with `Layout::apart` and the script layer.

## Parser gaps (next work)

The most common parser errors in sqlfluff's fixtures
(`sqp-fmt corpus --sqlfluff --errors 5 corpus/sqlfluff`, 2026-11-20),
per dialect: statements left as written, the construct, and the documented
grammar it needs. Counts are statements; a compound statement the parser
can't read is split at its `;`s for recovery, so its pieces count too
(PL/SQL, MySQL and BigQuery bodies). The parser isn't changed in this
series; each is a change for the fork, with tests, in the shape upstream
takes.

- postgresql: `analyze (verbose, skip_locked) t` (25; "ANALYZE":
  parenthesised option list); `unique nulls [not] distinct ... with
  (storage_parameter)` in column constraints (23; "CREATE TABLE",
  `index_parameters`); `create` / `alter` / `drop publication` (18 + 15;
  "CREATE PUBLICATION", "ALTER PUBLICATION"); `create` / `drop cast` (16;
  "CREATE CAST").
- mysql (mariadb alike): account names `'user'@'host'` in `create user`
  (29, mariadb 58; "Specifying Account Names"); `create function` /
  `create procedure` with a compound `begin ... end` body (26 + 18;
  "CREATE PROCEDURE and CREATE FUNCTION Statements", "Compound Statement
  Syntax"); `load data infile` (14; "LOAD DATA Statement"); `check table`
  (13; "CHECK TABLE Statement"); MariaDB's `add index if not exists`
  (12; MariaDB "ALTER TABLE").
- mssql: `delete` without `from` (8; "DELETE (Transact-SQL)": `DELETE
  [ FROM ] table_or_view_name`); procedure options `with encryption,
  recompile, execute as ...` (7; "CREATE PROCEDURE", `procedure_option`);
  index options `with (pad_index = off, ...)` (6; "CREATE INDEX",
  `relational_index_option`); `grant` / `deny ... on object::name` (5;
  "GRANT Object Permissions", `class ::`); `create fulltext catalog` /
  `index` (5; "CREATE FULLTEXT CATALOG").
- snowflake: `define <object>` (36; Snowflake DCM project definitions);
  `alter warehouse` (33; "ALTER WAREHOUSE"); `alter task` (32; "ALTER
  TASK"); `alter database ... set` (28; "ALTER DATABASE"); `alter dynamic
  table ... set` (27; "ALTER DYNAMIC TABLE").
- bigquery: `create [or replace] procedure` (9; "CREATE PROCEDURE
  statement"); the `with(a as ..., expr)` expression (8; "WITH
  expression"); `load data into` (8; "LOAD DATA statement"); `for ... in
  ... do ... end for` (6; procedural language "FOR...IN"); `alter ... set
  options (...)` (5; "ALTER TABLE SET OPTIONS statement").
- sqlite: `insert ... default values` (5; "INSERT" syntax diagram);
  `create virtual table ... using fts5(...)` module arguments (4 + 2;
  "CREATE VIRTUAL TABLE": module arguments are any tokens); `reindex` (4;
  "REINDEX"); `on conflict` after `insert ... select` (3; "UPSERT",
  parsing ambiguity).
- duckdb: `using sample` (17; "Samples"); `pivot` / `unpivot` statements
  (16 + 7; "PIVOT Statement", "UNPIVOT Statement"); `create` / `drop
  macro` (7; "CREATE MACRO"); `copy ... (per_thread_output ...)` (7;
  "COPY Statement").
- clickhouse: `system ...` statements (67; "SYSTEM Statements"); `create
  database ... engine = ...` and `on cluster` (36 + 22; "CREATE
  DATABASE"); `truncate all tables from` (36 + 36; "TRUNCATE Statements").
- databricks: `create or refresh [private] streaming table` / `materialized
  view` (25; Lakeflow Declarative Pipelines SQL); `create [or replace]
  [temporary] function` with a SQL body (18; "CREATE FUNCTION (SQL and
  Python)"); `create flow` (15; "CREATE FLOW"); `comment on catalog` and
  `alter catalog` (12 + 12; "COMMENT ON", "ALTER CATALOG").
- spark: `X"..."` binary literals (21; "Literals", binary literal); `add` /
  `list file | jar` (19 + 14; "ADD FILE", "LIST FILE"); `create [or
  replace] temporary function ... using jar` (15; "CREATE FUNCTION");
  `create or refresh live table` (11; Delta Live Tables SQL).
- hive: `alter database ... set dbproperties` (5; LanguageManual DDL,
  Alter Database); `from t insert ...` multi-insert (5; LanguageManual
  DML, "Multiple Inserts"); constraint `disable novalidate` (4;
  LanguageManual DDL, Constraints); `as (k, v)` aliases of a table-
  generating function (4; LanguageManual UDF, explode); `partition (...)
  if not exists` in `insert overwrite` (3; LanguageManual DML).
- redshift: `create database ... from integration` (14; "CREATE
  DATABASE"); `create user ... password 'md5...'` (13; "CREATE USER");
  `grant ... for schemas in database` (12; "GRANT", scoped permissions);
  `create schema ... quota` (12; "CREATE SCHEMA").
- oracle: SQL*Plus's `/` line, which ends a PL/SQL block or runs the
  buffer (200; "SQL*Plus User's Guide", Running SQL and PL/SQL): a client
  gap, not a parser one: the script layer has no SQL*Plus client yet, so
  PL/SQL blocks are split at their `;`s (`dbms_output.put_line(...)`
  128, `end loop` 83, `end if` 52); `alter session set` (78; "ALTER
  SESSION").
- teradata: `collect statistics` (11; "COLLECT STATISTICS"); `locking ...
  for access` / `lock row for write` request modifiers (6 + 3; "LOCKING
  Request Modifier"); column attributes in expressions, `'x'
  (casespecific)` (5; "Data Type Attributes"); `del` for `delete` (3).
- ansi: `create` / `drop cast` (13 + 5; SQL:2016 Part 2, 11.63
  <user-defined cast definition>); `create trigger ... execute procedure`
  (5; 11.39 <trigger definition>); `create function` with unnamed
  parameters (4; 11.60 <SQL-invoked routine>); a parenthesised table name
  `from (t)` (4; 7.6 <table reference>, <parenthesized joined table>).

The dialects sqlparser doesn't have (athena, db2, doris, exasol, flink,
impala, soql, starrocks, trino, vertica; formatted as `generic`) fail on
their own syntax (Db2 `declare global temporary table`, Doris /
StarRocks `duplicate key` and `properties`, Exasol `import`, Vertica
`::!` casts, Trino `with (properties)`), which is a dialect
for the fork to add, not a gap in an existing one.

## Decisions

- Block indentation: contents one indent (4) in from the `(` column, `)`
  under the `(` (STYLE.md, change 1; 2 in until 2026-11-20, when every
  offset became `indent`).
- The fork is pushed to `kill-face/sqp-rs`; its `main` bookmark is what
  sqp-fmt integrates on, and what the Nix package builds against.
- 2026-11-20, every dialect:
  - The dialect defaults to `generic`, which recognises no client syntax:
    the clients conflict (`GO` and `DELIMITER` are words elsewhere, `\`,
    `.` and `!` mean different things to different clients), as sqlfluff's
    ansi and JetBrains' Generic SQL recognise none.
  - Client syntax per dialect from the client's documentation: sqlcmd
    (mssql), mysql, psql (postgresql), the sqlite3 shell (sqlite; and
    duckdb, whose CLI forks it), SnowSQL (snowflake), BTEQ (teradata).
  - sqlcmd sends a `GO` batch whole, so a T-SQL batch parses as a whole
    or is left as written; every other client sends one statement at a
    time, so there a statement must end at the client's terminator, and
    one that doesn't parse leaves only itself as written.
  - Case: function and type names from the AST; an unqualified function
    call is a function name even if user-defined (as sqlfluff's
    capitalisation.functions does), a qualified one stays as written.
  - The general rules apply to every dialect's shared constructs
    unchanged (a PostgreSQL function's parameters get R22's block);
    procedural code outside T-SQL keeps its source layout.
