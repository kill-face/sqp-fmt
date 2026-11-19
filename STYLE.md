# Target style

The baseline is JetBrains' SQL formatter (Rider / DataGrip) with the code style
in `style/jetbrains-codestyle.xml`, as applied to T-SQL (its
`MSSQLCodeStyleSettings` block). `style/datagrip-reference.sql` is DataGrip's
output for `style/datagrip-reference.input.sql` with that scheme.

On top of that baseline there are two deliberate changes, both about making a
multi-line construct read as one indented block.

The formatter only changes whitespace and keyword/function/type case. It never
adds, removes or reorders tokens: no `as` inserted, no `inner` / `outer` added,
no `;` added, comments kept where they are.

## Baseline highlights

- Keywords, built-in functions and types lowercase; identifiers as written.
- 4-space indent, 120-column right margin.
- Clauses (`from`, `join`, `where`, `group by`, `having`, `order by`) start a
  line; a short clause body stays on the clause's line
  (`select a, b, c`, `group by o.Id, c.[Name]`).
- `join` at the clause level, its `on` on the next line indented 4.
- `where` predicates: one per line, `and` / `or` indented 2 under `where`.
- Leading commas when a list is chopped (`set`, `values`, procedure
  parameters, long select lists).
- `create table` column definitions aligned in columns.

## Change 1: logical blocks in boolean expressions

A parenthesised group inside a long boolean expression is laid out as a block:
`(` ends its line, the contents are indented, and `)` sits on its own line.

```sql
select *
from dbo.ShipmentEvents shpEvt
where shpEvt.EventKindCode = 6005
  and shpEvt.IsShipped = 'L'
  and fnInZone.Result = 'true'
  and (
        (
          shpEvt.RecordedAtUtcTime >= @recordedAtUtcTimeStart
          and shpEvt.RecordedAtUtcTime < @recordedAtUtcTimeEnd
        )
        or (
            other condition
            and other condition
        )
      )
```

The author decides what is a block: a group written with a line break after
its `(` is kept as a block; a group written on one line stays inline unless it
no longer fits in the margin, in which case it becomes a block.

**Indentation:** a block's contents go at the column of its `(` + 2 and its
`)` sits under the `(`, the same everywhere (including `and (` / `or (`).
The hand-written example above drifts from that in the `or (` group; the rule
wins:

```sql
  and (
        (
          shpEvt.RecordedAtUtcTime >= @recordedAtUtcTimeStart
          and shpEvt.RecordedAtUtcTime < @recordedAtUtcTimeEnd
        )
        or (
             other condition
             and other condition
           )
      )
```

## Change 2: window clauses as blocks

A long `over (...)` is laid out as a block: `(` ends the line, each of
`partition by`, `order by` and the frame clause starts its own line indented
2 past the start of the select item, and `)` lines up with the start of the
item.

```sql
select row_number() over (
         partition by col_a, col_b, col_c
         order by col_x
       ) as rn
     , new.Code
     , new.Name
     , new.ShortName
     , new.RegionId
     , new.IsVerified
     , new.ImportBatchId
from NewCarrier new
```

## Test bed

The SQL in a private T-SQL corpus (`corpus/private/`, about 510 files) is mostly in this
style already. It is used read-only, pointed at by path: never copy those
files into this repo. Formatting a file from it should change nothing except
where the two changes above apply or the file was simply out of style.

## Formatter control

- `-- @formatter:off` / `-- @formatter:on` (JetBrains' markers, already used
  in the test bed) leave the lines between them exactly as written.
- A batch that doesn't parse is left exactly as written; the rest of the
  file is still formatted. Placeholder templates (`values ();`, `check ()`)
  stay usable.

## Rules

What `tsqlfmt` implements, rule by rule. Each has a golden test in
`tests/golden/` (input `NN-name.sql`, output `NN-name.expected.sql`). Where a
rule comes from the test bed rather than the JetBrains scheme, the counts are
from `corpus/private/` (house style); "keeps" means the author's line break is
kept, and anything not covered here keeps its source layout, re-indented with
its surroundings. (So far that's everything but the rules below.)

Formatting is per `GO` batch. Batches that don't parse, `@formatter:off`
regions, `GO` lines and SQLCMD lines (`:r`, `:setvar`) come out exactly as
written; so does any batch whose output wouldn't tokenize back to the same
tokens (the safety net). Line endings (LF / CRLF), a BOM and a missing final
newline are kept.

### R1 Case (`01-case`)

Keywords, built-in functions, system types and `@@` functions lower-case.
Identifiers exactly as written, decided by the AST (a word that starts an
identifier node is one), so `Status`, `Name`, `Value` keep their case; so do
user-defined functions and types (`dbo.fnCustom`, `dbo.MyType`). Date parts
in `dateadd`-style functions count as keywords.

```sql
select [Name], Status, count(*) as Cnt, getdate() as Now, @@rowcount as N
declare @x nvarchar(max) = convert(varchar(10), 5), @y dbo.MyType, @z sysname
```

### R3 Statements (`03-statements`)

Each statement starts a line, at the indent of the block it's in; `;` stays
on the statement's last line; `;with` stays together.

### R4 Blank lines (`04-blank-lines`)

A blank line the author wrote between two lines is kept; runs of blank lines
become one. No blank lines are added.

### R5 Comments (`05-comments`)

Comments never move relative to the tokens around them. A trailing comment
stays on its line, as far from the code before it as in the source (so
aligned comments stay aligned). A comment on its own line keeps its offset
from the line after it. A comment right after a comma keeps the comma at the
end of its line (a chopped list can't move that comma to the next line).

```sql
select a,       -- trailing
       b        -- another
from t -- after from
```

### R27 Formatter control (`26-formatter-off`, `27-unparsed`)

`@formatter:off` … `@formatter:on` lines (both included) are verbatim, also
inside a statement or across `GO`; the code after them continues at the
indent it would have.
