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
