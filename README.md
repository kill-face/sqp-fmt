# tsqlfmt

A T-SQL formatter that only changes whitespace and case, built on a fork of
[sqlparser-rs](https://github.com/apache/datafusion-sqlparser-rs). The target
style is in [STYLE.md](STYLE.md).

## Layout

- `../datafusion-sqlparser-rs`: the parser fork (`origin` =
  `kill-face/datafusion-sqlparser-rs`, `upstream` = `apache/...`), worked on
  under the `tsql` bookmark. Each grammar fix is its own change with tests, so
  it can go upstream.
- `src/lib.rs`: splits a file into `GO` batches (SQLCMD `:r` / `:setvar`
  lines are skipped), keeping file line numbers, and parses each batch with
  the MS SQL dialect, keeping comments.
- `src/main.rs`: the `tsqlfmt` command.

## Commands

```sh
nix develop                      # or direnv
cargo run --release -- check corpus/private/           # parse rate + grouped errors
cargo run --release -- check --files corpus/private/   # ... plus every failing file
```

Formatting itself (`tsqlfmt < in.sql > out.sql`, for conform.nvim) comes once
the whole test bed parses.

## Status

| Date | corpus files parsing |
|---|---|
| 2026-11-19 | 263 / 510 (upstream sqlparser-rs v0.64.0) |
| 2026-11-19 | 292 / 510 (fork: `PRIMARY KEY` / `UNIQUE` `CLUSTERED`) |
| 2026-11-19 | 348 / 510 (fork: `CREATE [UNIQUE] [NON]CLUSTERED [COLUMNSTORE] INDEX`) |
| 2026-11-19 | 414 / 510 (fork `tsql`, 20 changes; 476 / 510 with `require_semicolon_stmt_delimiter: false`) |
| 2026-11-19 | 478 / 510 (harness: optional semicolons on, `go;`, `GO` inside comments/strings ignored). The 32 left: 30 placeholder templates that aren't valid SQL (`values ();`, empty `from (values )`, `check ()`), one file of CREATE LOGIN / USER … FOR LOGIN and one of ALTER DATABASE … SET and CREATE DATABASE file lists |

## Decisions

- Block indentation: contents at the `(` column + 2, `)` under the `(`
  (STYLE.md, change 1). Decided 2026-11-19.
- The fork stays local for now; nothing is pushed to GitHub.
