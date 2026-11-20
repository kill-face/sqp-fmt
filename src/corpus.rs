//! `sqp-fmt corpus [--config FILE] [--dialect NAME] [--diff FILE] [--top N] <path>...`:
//! format every .sql file in memory and report how much changed, the
//! safety-net fallbacks and idempotency failures. This is the progress
//! measure for the formatter. The configuration is `--config FILE` or the
//! defaults (no `sqp-fmt.toml` discovery); the dialect is `--dialect`, else
//! the configuration's `[[dialect_paths]]` entry for the file, else its
//! `dialect`.
//!
//! `sqp-fmt corpus --sqlfluff [--config FILE] [--errors N] <dir>`: the same
//! for sqlfluff's dialect test fixtures (`test/fixtures/dialects/<dialect>/`
//! of a sqlfluff checkout, or a copy), one sqlfluff dialect directory at a
//! time with its sqlparser dialect (`SQLFLUFF_DIALECTS`), as a table; with
//! `--errors N`, each dialect's N most common parser errors.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use crate::{collect, usage};

/// sqlfluff's dialects (4.5.0) and the sqlparser dialect their fixtures are
/// formatted as: the same dialect (by name, or renamed: `postgres`,
/// `sparksql`, `tsql`); for a dialect sqlparser doesn't have, the one it
/// derives from (MariaDB from MySQL; Greenplum and Materialize from
/// PostgreSQL, with PostgreSQL's wire protocol and psql); for the rest
/// `generic`, the union of sqlparser's grammars: they derive from no
/// sqlparser dialect (Athena and Trino are Presto; Db2, Exasol, Flink,
/// Vertica have grammars of their own; Doris, Impala and StarRocks only
/// borrow their parent's syntax in part), and SOQL isn't SQL but is
/// measured anyway (the safety net must hold on any input).
pub const SQLFLUFF_DIALECTS: &[(&str, &str)] = &[
    ("ansi", "ansi"),
    ("athena", "generic"),
    ("bigquery", "bigquery"),
    ("clickhouse", "clickhouse"),
    ("databricks", "databricks"),
    ("db2", "generic"),
    ("doris", "generic"),
    ("duckdb", "duckdb"),
    ("exasol", "generic"),
    ("flink", "generic"),
    ("greenplum", "postgresql"),
    ("hive", "hive"),
    ("impala", "generic"),
    ("mariadb", "mysql"),
    ("materialize", "postgresql"),
    ("mysql", "mysql"),
    ("oracle", "oracle"),
    ("postgres", "postgresql"),
    ("redshift", "redshift"),
    ("snowflake", "snowflake"),
    ("soql", "generic"),
    ("sparksql", "spark"),
    ("sqlite", "sqlite"),
    ("starrocks", "generic"),
    ("teradata", "teradata"),
    ("trino", "generic"),
    ("tsql", "mssql"),
    ("vertica", "generic"),
];

/// What formatting a set of files did.
#[derive(Default)]
struct Measure {
    files: usize,
    /// Files whose every batch (statement) parses.
    files_parsing: usize,
    unchanged_files: usize,
    lines: usize,
    unchanged_lines: usize,
    total: sqp_fmt::Stats,
    /// (changed lines, file).
    changed: Vec<(usize, PathBuf)>,
    not_idempotent: Vec<PathBuf>,
    problems: Vec<String>,
    /// Parser error (without its position) -> (count, first "file:line").
    errors: BTreeMap<String, (usize, String)>,
    slowest: (Duration, PathBuf),
}

/// Formats each file twice in memory (`config_for` gives its
/// configuration; `None`: stop, exit code 2).
fn measure(
    files: &[PathBuf],
    config_for: &mut dyn FnMut(&Path) -> Option<sqp_fmt::Config>,
) -> Option<Measure> {
    let mut m = Measure::default();
    for file in files {
        let Ok(src) = std::fs::read_to_string(file) else {
            eprintln!("{}: unreadable", file.display());
            continue;
        };
        let cfg = config_for(file)?;
        m.files += 1;
        let t = Instant::now();
        let (out, stats) = sqp_fmt::format_with(&src, &cfg);
        let took = t.elapsed();
        if took > m.slowest.0 {
            m.slowest = (took, file.clone());
        }
        let a: Vec<&str> = src.lines().collect();
        let b: Vec<&str> = out.lines().collect();
        let same = common_lines(&a, &b);
        m.lines += a.len();
        m.unchanged_lines += same;
        if out == src {
            m.unchanged_files += 1;
        } else {
            m.changed.push((a.len().max(b.len()) - same, file.clone()));
        }
        let (again, _) = sqp_fmt::format_with(&out, &cfg);
        if again != out {
            m.not_idempotent.push(file.clone());
        }
        for (line, msg) in &stats.problems {
            let msg: String = msg.chars().take(300).collect();
            m.problems.push(format!("{}:{line}: {msg}", file.display()));
        }
        if stats.unparsed == 0 && stats.partial == 0 {
            m.files_parsing += 1;
        }
        for (line, msg) in &stats.parse_errors {
            let key = msg
                .split_once(" at Line: ")
                .map_or(msg.as_str(), |(k, _)| k);
            // Long "Expected: a, b, c, ..." lists cut short, keeping what
            // was found.
            let key = match key.rsplit_once(", found: ") {
                Some((exp, found)) if exp.chars().count() > 100 => {
                    let exp: String = exp.chars().take(100).collect();
                    format!("{exp}..., found: {found}")
                }
                _ => key.to_string(),
            };
            m.errors
                .entry(key)
                .or_insert((0, format!("{}:{line}", file.display())))
                .0 += 1;
        }
        let t = &mut m.total;
        t.batches += stats.batches;
        t.formatted += stats.formatted;
        t.partial += stats.partial;
        t.unparsed_statements += stats.unparsed_statements;
        t.unparsed += stats.unparsed;
        t.mismatches += stats.mismatches;
        t.errors += stats.errors;
        t.off += stats.off;
    }
    Some(m)
}

impl Measure {
    fn fallbacks(&self) -> usize {
        self.total.mismatches + self.total.errors
    }

    fn ok(&self) -> bool {
        self.problems.is_empty() && self.not_idempotent.is_empty()
    }

    fn changed_lines(&self) -> usize {
        self.changed.iter().map(|c| c.0).sum()
    }

    /// The most common parser errors, most common first.
    fn top_errors(&self, n: usize) -> Vec<(&str, usize, &str)> {
        let mut v: Vec<_> = self
            .errors
            .iter()
            .map(|(k, (c, at))| (k.as_str(), *c, at.as_str()))
            .collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
        v.truncate(n);
        v
    }
}

fn pct(n: usize, d: usize) -> f64 {
    if d == 0 {
        0.0
    } else {
        100.0 * n as f64 / d as f64
    }
}

pub fn run(args: &[String]) -> ExitCode {
    let mut diff_file: Option<String> = None;
    let mut top = 15;
    let mut roots = Vec::new();
    let mut verbose = false;
    let mut sqlfluff = false;
    let mut errors = 0;
    let mut cfg = sqp_fmt::Config::default();
    let mut dialect: Option<String> = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--config" => match it.next().map(|p| sqp_fmt::Config::load(Path::new(p))) {
                Some(Ok(c)) => cfg = c,
                Some(Err(e)) => {
                    eprintln!("sqp-fmt: {e}");
                    return ExitCode::from(2);
                }
                None => return usage(),
            },
            "--dialect" => match it.next() {
                Some(d) => dialect = Some(d.clone()),
                None => return usage(),
            },
            "--diff" => diff_file = it.next().cloned(),
            "--top" => top = it.next().and_then(|n| n.parse().ok()).unwrap_or(top),
            "--errors" => match it.next().and_then(|n| n.parse().ok()) {
                Some(n) => errors = n,
                None => return usage(),
            },
            "--problems" => verbose = true,
            "--list" => top = usize::MAX,
            "--sqlfluff" => sqlfluff = true,
            _ if a.starts_with("--") => return usage(),
            _ => roots.push(a.clone()),
        }
    }
    std::panic::set_hook(Box::new(|_| {}));
    if sqlfluff {
        return match (roots.as_slice(), &dialect, &diff_file) {
            ([dir], None, None) => run_sqlfluff(Path::new(dir), &cfg, errors, verbose),
            _ => usage(),
        };
    }
    let base = cfg;
    let mut per_dialect: HashMap<&'static str, sqp_fmt::Config> = HashMap::new();
    let mut config_for = |path: &Path| -> Option<sqp_fmt::Config> {
        let d = crate::resolve_dialect(dialect.as_deref(), &base, Some(path))?;
        Some(
            per_dialect
                .entry(d)
                .or_insert_with(|| base.for_dialect(d))
                .clone(),
        )
    };
    if let Some(f) = diff_file {
        let Some(cfg) = config_for(Path::new(&f)) else {
            return ExitCode::from(2);
        };
        let src = match std::fs::read_to_string(&f) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{f}: {e}");
                return ExitCode::FAILURE;
            }
        };
        let (out, stats) = sqp_fmt::format_with(&src, &cfg);
        print!("{}", unified(&f, &src, &out));
        for (line, msg) in &stats.problems {
            eprintln!("{f}:{line}: {msg}");
        }
        let (again, _) = sqp_fmt::format_with(&out, &cfg);
        if again != out {
            eprintln!("{f}: not idempotent; second pass:");
            eprint!("{}", unified(&f, &out, &again));
        }
        return ExitCode::SUCCESS;
    }
    if roots.is_empty() {
        return usage();
    }
    let mut files = Vec::new();
    for r in &roots {
        collect(Path::new(r), &mut files);
    }
    let started = Instant::now();
    let Some(mut m) = measure(&files, &mut config_for) else {
        return ExitCode::from(2);
    };
    let total = &m.total;
    println!(
        "{}/{} files unchanged ({:.1}%), {}/{} lines unchanged ({:.2}%)",
        m.unchanged_files,
        m.files,
        pct(m.unchanged_files, m.files),
        m.unchanged_lines,
        m.lines,
        pct(m.unchanged_lines, m.lines)
    );
    println!(
        "{} batches: {} formatted, {} partly formatted ({} statements left as written), {} left as written (don't parse), {} in @formatter:off regions",
        total.batches,
        total.formatted,
        total.partial,
        total.unparsed_statements,
        total.unparsed,
        total.off
    );
    println!(
        "safety fallbacks: {} token mismatches, {} internal errors; {} files not idempotent",
        total.mismatches,
        total.errors,
        m.not_idempotent.len()
    );
    println!(
        "time: {:.0?} total, slowest {:.1?} ({})",
        started.elapsed(),
        m.slowest.0,
        m.slowest.1.display()
    );
    for p in m
        .problems
        .iter()
        .take(if verbose { usize::MAX } else { 10 })
    {
        println!("  problem: {p}");
    }
    for f in m
        .not_idempotent
        .iter()
        .take(if verbose { usize::MAX } else { 10 })
    {
        println!("  not idempotent: {}", f.display());
    }
    m.changed
        .sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    println!("biggest diffs (changed lines):");
    for (n, f) in m.changed.iter().take(top) {
        println!("{n:6}  {}", f.display());
    }
    if errors > 0 {
        println!("most common parser errors:");
        for (msg, n, at) in m.top_errors(errors) {
            println!("{n:6}  {msg}\n        {at}");
        }
    }
    if m.ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// `corpus --sqlfluff`: each sqlfluff dialect directory under `dir` with its
/// sqlparser dialect, as a Markdown table.
fn run_sqlfluff(dir: &Path, base: &sqp_fmt::Config, errors: usize, verbose: bool) -> ExitCode {
    let mut known = Vec::new();
    let mut rows = Vec::new();
    let mut all_ok = true;
    for &(fluff, ours) in SQLFLUFF_DIALECTS {
        let d = dir.join(fluff);
        known.push(fluff);
        if !d.is_dir() {
            continue;
        }
        let mut files = Vec::new();
        collect(&d, &mut files);
        let cfg = base.for_dialect(ours);
        let Some(m) = measure(&files, &mut |_| Some(cfg.clone())) else {
            return ExitCode::from(2);
        };
        all_ok &= m.ok();
        rows.push((fluff, ours, m));
    }
    if let Ok(entries) = std::fs::read_dir(dir) {
        let mut unknown: Vec<String> = entries
            .flatten()
            .filter(|e| e.path().is_dir())
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|n| !known.contains(&n.as_str()))
            .collect();
        unknown.sort();
        for n in unknown {
            eprintln!(
                "sqp-fmt: {}: not a sqlfluff dialect sqp-fmt knows, skipped",
                dir.join(n).display()
            );
        }
    }
    if rows.is_empty() {
        eprintln!(
            "sqp-fmt: {}: no sqlfluff dialect directories",
            dir.display()
        );
        return ExitCode::from(2);
    }
    println!(
        "| sqlfluff | sqlparser | files | files parsing | batches formatted / partly / not | statements left as written | safety fallbacks | not idempotent | lines changed |"
    );
    println!("|---|---|---|---|---|---|---|---|---|");
    let mut sum = Measure::default();
    for (fluff, ours, m) in &rows {
        let t = &m.total;
        println!(
            "| {fluff} | {ours} | {} | {} | {} / {} / {} | {} | {} | {} | {} / {} |",
            m.files,
            m.files_parsing,
            t.formatted,
            t.partial,
            t.unparsed,
            t.unparsed_statements,
            m.fallbacks(),
            m.not_idempotent.len(),
            m.changed_lines(),
            m.lines
        );
        sum.files += m.files;
        sum.files_parsing += m.files_parsing;
        sum.lines += m.lines;
        sum.total.formatted += t.formatted;
        sum.total.partial += t.partial;
        sum.total.unparsed += t.unparsed;
        sum.total.unparsed_statements += t.unparsed_statements;
        sum.total.mismatches += m.fallbacks();
        sum.not_idempotent.extend(m.not_idempotent.iter().cloned());
        sum.changed.push((m.changed_lines(), PathBuf::new()));
    }
    let t = &sum.total;
    println!(
        "| all | | {} | {} | {} / {} / {} | {} | {} | {} | {} / {} |",
        sum.files,
        sum.files_parsing,
        t.formatted,
        t.partial,
        t.unparsed,
        t.unparsed_statements,
        t.mismatches,
        sum.not_idempotent.len(),
        sum.changed_lines(),
        sum.lines
    );
    for (fluff, _, m) in &rows {
        for p in m
            .problems
            .iter()
            .take(if verbose { usize::MAX } else { 10 })
        {
            println!("  {fluff}: problem: {p}");
        }
        for f in m
            .not_idempotent
            .iter()
            .take(if verbose { usize::MAX } else { 10 })
        {
            println!("  {fluff}: not idempotent: {}", f.display());
        }
    }
    if errors > 0 {
        for (fluff, ours, m) in &rows {
            println!("\n{fluff} ({ours}): most common parser errors");
            for (msg, n, at) in m.top_errors(errors) {
                println!("{n:6}  {msg}\n        {at}");
            }
        }
    }
    if all_ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Op {
    Same(usize, usize),
    Del(usize),
    Ins(usize),
}

/// Line diff: patience-style anchoring on lines unique to both sides, LCS
/// between anchors when small, else delete-all/insert-all.
fn diff(a: &[&str], b: &[&str]) -> Vec<Op> {
    let mut ops = Vec::new();
    diff_rec(a, b, 0, a.len(), 0, b.len(), &mut ops);
    ops
}

fn diff_rec(
    a: &[&str],
    b: &[&str],
    mut a0: usize,
    mut a1: usize,
    mut b0: usize,
    mut b1: usize,
    ops: &mut Vec<Op>,
) {
    // Common prefix and suffix.
    while a0 < a1 && b0 < b1 && a[a0] == b[b0] {
        ops.push(Op::Same(a0, b0));
        a0 += 1;
        b0 += 1;
    }
    let mut tail = Vec::new();
    while a0 < a1 && b0 < b1 && a[a1 - 1] == b[b1 - 1] {
        a1 -= 1;
        b1 -= 1;
        tail.push(Op::Same(a1, b1));
    }
    if a0 == a1 || b0 == b1 {
        ops.extend((a0..a1).map(Op::Del));
        ops.extend((b0..b1).map(Op::Ins));
        ops.extend(tail.into_iter().rev());
        return;
    }
    // Unique lines on both sides.
    let mut count: HashMap<&str, (usize, usize, usize)> = HashMap::new();
    for (i, l) in a.iter().enumerate().take(a1).skip(a0) {
        let e = count.entry(l).or_insert((0, 0, 0));
        e.0 += 1;
        e.2 = i;
    }
    let mut pairs = Vec::new();
    let mut bcount: HashMap<&str, (usize, usize)> = HashMap::new();
    for (j, l) in b.iter().enumerate().take(b1).skip(b0) {
        let e = bcount.entry(l).or_insert((0, 0));
        e.0 += 1;
        e.1 = j;
    }
    for (l, (na, _, i)) in &count {
        if *na == 1
            && let Some((1, j)) = bcount.get(l)
        {
            pairs.push((*i, *j));
        }
    }
    pairs.sort();
    let anchors = lis(&pairs);
    if anchors.is_empty() {
        lcs_small(a, b, a0, a1, b0, b1, ops);
    } else {
        let (mut pa, mut pb) = (a0, b0);
        for (i, j) in anchors {
            diff_rec(a, b, pa, i, pb, j, ops);
            ops.push(Op::Same(i, j));
            pa = i + 1;
            pb = j + 1;
        }
        diff_rec(a, b, pa, a1, pb, b1, ops);
    }
    ops.extend(tail.into_iter().rev());
}

/// Longest increasing subsequence of `pairs` (sorted by first) by second.
fn lis(pairs: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let mut tails: Vec<usize> = Vec::new();
    let mut prev = vec![usize::MAX; pairs.len()];
    for (k, &(_, j)) in pairs.iter().enumerate() {
        let pos = tails.partition_point(|&t| pairs[t].1 < j);
        if pos > 0 {
            prev[k] = tails[pos - 1];
        }
        if pos == tails.len() {
            tails.push(k);
        } else {
            tails[pos] = k;
        }
    }
    let mut out = Vec::new();
    let mut k = tails.last().copied().unwrap_or(usize::MAX);
    while k != usize::MAX {
        out.push(pairs[k]);
        k = prev[k];
    }
    out.reverse();
    out
}

fn lcs_small(
    a: &[&str],
    b: &[&str],
    a0: usize,
    a1: usize,
    b0: usize,
    b1: usize,
    ops: &mut Vec<Op>,
) {
    let (n, m) = (a1 - a0, b1 - b0);
    if n * m > 4_000_000 {
        ops.extend((a0..a1).map(Op::Del));
        ops.extend((b0..b1).map(Op::Ins));
        return;
    }
    let mut dp = vec![0u32; (n + 1) * (m + 1)];
    let w = m + 1;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i * w + j] = if a[a0 + i] == b[b0 + j] {
                dp[(i + 1) * w + j + 1] + 1
            } else {
                dp[(i + 1) * w + j].max(dp[i * w + j + 1])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if a[a0 + i] == b[b0 + j] {
            ops.push(Op::Same(a0 + i, b0 + j));
            i += 1;
            j += 1;
        } else if dp[(i + 1) * w + j] >= dp[i * w + j + 1] {
            ops.push(Op::Del(a0 + i));
            i += 1;
        } else {
            ops.push(Op::Ins(b0 + j));
            j += 1;
        }
    }
    ops.extend((a0 + i..a1).map(Op::Del));
    ops.extend((b0 + j..b1).map(Op::Ins));
}

fn common_lines(a: &[&str], b: &[&str]) -> usize {
    if a == b {
        return a.len();
    }
    diff(a, b)
        .iter()
        .filter(|o| matches!(o, Op::Same(..)))
        .count()
}

/// Unified diff with 3 lines of context.
pub fn unified(name: &str, src: &str, out: &str) -> String {
    let a: Vec<&str> = src.lines().collect();
    let b: Vec<&str> = out.lines().collect();
    let ops = diff(&a, &b);
    let mut s = String::new();
    if ops.iter().all(|o| matches!(o, Op::Same(..))) {
        return s;
    }
    s.push_str(&format!("--- {name}\n+++ {name} (formatted)\n"));
    let ctx = 3;
    let changed: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, o)| !matches!(o, Op::Same(..)))
        .map(|(k, _)| k)
        .collect();
    let mut k = 0;
    while k < changed.len() {
        let start = changed[k].saturating_sub(ctx);
        let mut end = changed[k];
        while k < changed.len() && changed[k] <= end + 2 * ctx {
            end = changed[k];
            k += 1;
        }
        let end = (end + ctx + 1).min(ops.len());
        let (mut la, mut lb) = (None, None);
        let (mut na, mut nb) = (0, 0);
        for o in &ops[start..end] {
            match o {
                Op::Same(i, j) => {
                    la.get_or_insert(*i);
                    lb.get_or_insert(*j);
                    na += 1;
                    nb += 1;
                }
                Op::Del(i) => {
                    la.get_or_insert(*i);
                    na += 1;
                }
                Op::Ins(j) => {
                    lb.get_or_insert(*j);
                    nb += 1;
                }
            }
        }
        s.push_str(&format!(
            "@@ -{},{na} +{},{nb} @@\n",
            la.map_or(0, |x| x + 1),
            lb.map_or(0, |x| x + 1)
        ));
        for o in &ops[start..end] {
            match o {
                Op::Same(i, _) => s.push_str(&format!(" {}\n", a[*i])),
                Op::Del(i) => s.push_str(&format!("-{}\n", a[*i])),
                Op::Ins(j) => s.push_str(&format!("+{}\n", b[*j])),
            }
        }
    }
    s
}
