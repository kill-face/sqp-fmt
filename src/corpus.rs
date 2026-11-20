//! `sqp-fmt corpus [--config FILE] [--diff FILE] [--top N] <path>...`: format
//! every .sql file in memory and report how much changed, the safety-net
//! fallbacks and idempotency failures. This is the progress measure for the
//! formatter. The configuration is `--config FILE` or the defaults (no
//! `sqp-fmt.toml` discovery).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{Duration, Instant};

use crate::{collect, usage};

pub fn run(args: &[String]) -> ExitCode {
    let mut diff_file: Option<String> = None;
    let mut top = 15;
    let mut roots = Vec::new();
    let mut verbose = false;
    let mut cfg = sqp_fmt::Config::default();
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
            "--diff" => diff_file = it.next().cloned(),
            "--top" => top = it.next().and_then(|n| n.parse().ok()).unwrap_or(top),
            "--problems" => verbose = true,
            "--list" => top = usize::MAX,
            _ if a.starts_with("--") => return usage(),
            _ => roots.push(a.clone()),
        }
    }
    std::panic::set_hook(Box::new(|_| {}));
    if let Some(f) = diff_file {
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

    let mut total = sqp_fmt::Stats::default();
    let (mut lines, mut unchanged_lines, mut unchanged_files) = (0usize, 0usize, 0usize);
    let mut changed: Vec<(usize, PathBuf)> = Vec::new();
    let mut not_idempotent = Vec::new();
    let mut problems = Vec::new();
    let mut slowest = (Duration::ZERO, PathBuf::new());
    let started = Instant::now();
    for file in &files {
        let Ok(src) = std::fs::read_to_string(file) else {
            eprintln!("{}: unreadable", file.display());
            continue;
        };
        let t = Instant::now();
        let (out, stats) = sqp_fmt::format_with(&src, &cfg);
        let took = t.elapsed();
        if took > slowest.0 {
            slowest = (took, file.clone());
        }
        let a: Vec<&str> = src.lines().collect();
        let b: Vec<&str> = out.lines().collect();
        let same = common_lines(&a, &b);
        lines += a.len();
        unchanged_lines += same;
        if out == src {
            unchanged_files += 1;
        } else {
            changed.push((a.len().max(b.len()) - same, file.clone()));
        }
        let (again, _) = sqp_fmt::format_with(&out, &cfg);
        if again != out {
            not_idempotent.push(file.clone());
        }
        for (line, msg) in &stats.problems {
            let msg: String = msg.chars().take(300).collect();
            problems.push(format!("{}:{line}: {msg}", file.display()));
        }
        total.batches += stats.batches;
        total.formatted += stats.formatted;
        total.unparsed += stats.unparsed;
        total.mismatches += stats.mismatches;
        total.errors += stats.errors;
        total.off += stats.off;
    }
    let pct = |n: usize, d: usize| {
        if d == 0 {
            0.0
        } else {
            100.0 * n as f64 / d as f64
        }
    };
    println!(
        "{unchanged_files}/{} files unchanged ({:.1}%), {unchanged_lines}/{lines} lines unchanged ({:.2}%)",
        files.len(),
        pct(unchanged_files, files.len()),
        pct(unchanged_lines, lines)
    );
    println!(
        "{} batches: {} formatted, {} left as written (don't parse), {} in @formatter:off regions",
        total.batches, total.formatted, total.unparsed, total.off
    );
    println!(
        "safety fallbacks: {} token mismatches, {} internal errors; {} files not idempotent",
        total.mismatches,
        total.errors,
        not_idempotent.len()
    );
    println!(
        "time: {:.0?} total, slowest {:.1?} ({})",
        started.elapsed(),
        slowest.0,
        slowest.1.display()
    );
    for p in problems.iter().take(if verbose { usize::MAX } else { 10 }) {
        println!("  problem: {p}");
    }
    for f in not_idempotent
        .iter()
        .take(if verbose { usize::MAX } else { 10 })
    {
        println!("  not idempotent: {}", f.display());
    }
    changed.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    println!("biggest diffs (changed lines):");
    for (n, f) in changed.iter().take(top) {
        println!("{n:6}  {}", f.display());
    }
    if problems.is_empty() && not_idempotent.is_empty() {
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
