//! `tsqlfmt check <path>...`: parse every .sql file under the paths and
//! report which fail, grouped by parser error. This is the progress measure
//! for the grammar work in the sqlparser-rs fork.
//!
//! `tsqlfmt spans [--details] <path>...`: audit the source spans of the parsed
//! AST (see `tsqlfmt::spans`), the progress measure for the span work.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod corpus;

pub(crate) fn usage() -> ExitCode {
    eprintln!(
        "usage: tsqlfmt [FILE]                      format FILE (or stdin) to stdout\n       \
         tsqlfmt corpus [--diff FILE] [--top N] <path>...\n       \
         tsqlfmt check [--files] <path>...\n       \
         tsqlfmt spans [--details] <path>..."
    );
    ExitCode::from(2)
}

/// Hidden dirs (.git, .direnv, .idea, ...) and build output hold copies of
/// the same files. Only applies below the paths given on the command line.
fn skip_dir(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.starts_with('.') || matches!(name, "bin" | "obj" | "node_modules" | "target")
}

pub(crate) fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_dir() {
        let mut entries: Vec<_> = std::fs::read_dir(path)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .collect();
        entries.sort();
        for p in entries {
            if p.is_dir() && skip_dir(&p) {
                continue;
            }
            collect(&p, out);
        }
    } else if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("sql"))
    {
        out.push(path.to_path_buf());
    }
}

fn check(args: &[String]) -> ExitCode {
    let list_files = args.iter().any(|a| a == "--files");
    let roots: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    if roots.is_empty() {
        return usage();
    }
    let mut files = Vec::new();
    for r in roots {
        collect(Path::new(r), &mut files);
    }

    let mut ok = 0;
    // error message (without position) -> (count, example "file:line:col")
    let mut groups: BTreeMap<String, (usize, String)> = BTreeMap::new();
    let mut failed = Vec::new();
    for file in &files {
        let src = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}: {e}", file.display());
                continue;
            }
        };
        let err = tsqlfmt::split_batches(&src)
            .iter()
            .find_map(|b| tsqlfmt::parse_batch(b).err());
        match err {
            None => ok += 1,
            Some(e) => {
                let msg = e.to_string();
                let (key, pos) = match msg.split_once(" at Line: ") {
                    Some((k, p)) => (k.to_string(), p.replace(", Column: ", ":")),
                    None => (msg.clone(), String::new()),
                };
                let example = format!("{}:{pos}", file.display());
                groups.entry(key).or_insert((0, example.clone())).0 += 1;
                failed.push(example);
            }
        }
    }

    println!(
        "{ok}/{} files parse ({} fail)",
        files.len(),
        files.len() - ok
    );
    let mut groups: Vec<_> = groups.into_iter().collect();
    groups.sort_by(|a, b| b.1.0.cmp(&a.1.0).then_with(|| a.0.cmp(&b.0)));
    for (msg, (n, example)) in &groups {
        println!("{n:5}  {msg}\n       {example}");
    }
    if list_files {
        println!("\nfailing files:");
        for f in &failed {
            println!("  {f}");
        }
    }
    if ok == files.len() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// `tsqlfmt spans [--details] <path>...`: audit the AST source spans of every
/// batch that parses (see `tsqlfmt::spans`), grouped by problem and node kind.
fn spans(args: &[String]) -> ExitCode {
    let details = args.iter().any(|a| a == "--details");
    let roots: Vec<&String> = args.iter().filter(|a| !a.starts_with("--")).collect();
    if roots.is_empty() {
        return usage();
    }
    let mut files = Vec::new();
    for r in roots {
        collect(Path::new(r), &mut files);
    }

    let mut audit = tsqlfmt::spans::Audit::default();
    let (mut batches, mut skipped) = (0, 0);
    // (problem, node) -> locations "file:line:col"
    let mut groups: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for file in &files {
        let src = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}: {e}", file.display());
                continue;
            }
        };
        for batch in tsqlfmt::split_batches(&src) {
            if tsqlfmt::spans::audit_batch(&batch, &mut audit).is_err() {
                skipped += 1;
                continue;
            }
            batches += 1;
            for f in audit.findings.drain(..) {
                let at = format!("{}:{}:{}", file.display(), f.at.line, f.at.column);
                groups.entry((f.problem, f.node)).or_default().push(at);
            }
        }
    }

    let a = &audit;
    let pct = |n: usize, d: usize| {
        if d == 0 {
            0.0
        } else {
            100.0 * n as f64 / d as f64
        }
    };
    println!(
        "{batches} batches audited in {} files ({skipped} batches don't parse, skipped)",
        files.len()
    );
    println!(
        "{} top-level statements, {} nested nodes checked",
        a.statements, a.nodes
    );
    println!(
        "{} tokens: {} ({:.2}%) outside every top-level statement span, {} in more than one",
        a.tokens,
        a.uncovered,
        pct(a.uncovered, a.tokens),
        a.overlapping
    );
    println!(
        "{} empty spans, {} short spans, {} unlike their Display, {} outside parent, {} off token boundaries, {} stray tokens in lists",
        a.empty, a.short, a.unlike_display, a.outside_parent, a.off_boundary, a.gap_tokens
    );
    let mut groups: Vec<_> = groups.into_iter().collect();
    let total: usize = groups.iter().map(|(_, locs)| locs.len()).sum();
    println!("{total} findings in {} groups:", groups.len());
    groups.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
    for ((problem, node), locs) in &groups {
        println!("{:5}  {node}: {problem}\n       {}", locs.len(), locs[0]);
        if details {
            for l in &locs[1..] {
                println!("       {l}");
            }
        }
    }
    if groups.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// `tsqlfmt [FILE]`: format FILE (or stdin) to stdout. Batches that can't
/// be formatted safely come out exactly as written.
fn format_cmd(path: Option<&str>) -> ExitCode {
    use std::io::{Read, Write};
    // A panic is caught per batch and that batch left as written; keep the
    // panic message off stderr, which editors show to the user.
    std::panic::set_hook(Box::new(|_| {}));
    let mut src = String::new();
    let read = match path {
        Some(p) => std::fs::read_to_string(p).map(|s| src = s),
        None => std::io::stdin().read_to_string(&mut src).map(|_| ()),
    };
    if let Err(e) = read {
        eprintln!("tsqlfmt: {e}");
        return ExitCode::FAILURE;
    }
    let (out, _) = tsqlfmt::format(&src);
    let mut stdout = std::io::stdout().lock();
    if stdout
        .write_all(out.as_bytes())
        .and_then(|_| stdout.flush())
        .is_err()
    {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("check") => check(&args[1..]),
        Some("spans") => spans(&args[1..]),
        Some("corpus") => corpus::run(&args[1..]),
        Some("-h" | "--help" | "help") => usage(),
        Some(p) if !p.starts_with('-') => format_cmd(Some(p)),
        Some("-") | None => format_cmd(None),
        _ => usage(),
    }
}
