//! `sqp-fmt check <path>...`: parse every .sql file under the paths and
//! report which fail, grouped by parser error. This is the progress measure
//! for the grammar work in the sqlparser-rs fork.
//!
//! `sqp-fmt spans [--details] <path>...`: audit the source spans of the parsed
//! AST (see `sqp_fmt::spans`), the progress measure for the span work.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod corpus;

pub(crate) fn usage() -> ExitCode {
    eprintln!(
        "usage: sqp-fmt [--config FILE] [--dialect NAME] [--stdin-filepath PATH] [FILE | -]\n         \
         format FILE (or stdin) to stdout, with FILE's (PATH's) sqp-fmt.toml\n       \
         sqp-fmt --write [--config FILE] [--dialect NAME] <path>...\n         \
         format files in place (directories: their .sql files); prints the files changed\n       \
         sqp-fmt --check [--config FILE] [--dialect NAME] <path>...\n         \
         print the files formatting would change; exit 1 if any, 2 on errors\n       \
         sqp-fmt corpus [--config FILE] [--dialect NAME] [--diff FILE] [--top N] <path>...\n       \
         sqp-fmt corpus --sqlfluff [--config FILE] <dir>\n       \
         sqp-fmt check [--dialect NAME] [--files] <path>...\n       \
         sqp-fmt spans [--dialect NAME] [--details] <path>..."
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
        let err = sqp_fmt::split_batches(&src)
            .iter()
            .find_map(|b| sqp_fmt::parse_batch(b).err());
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

/// `sqp-fmt spans [--details] <path>...`: audit the AST source spans of every
/// batch that parses (see `sqp_fmt::spans`), grouped by problem and node kind.
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

    let mut audit = sqp_fmt::spans::Audit::default();
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
        for batch in sqp_fmt::split_batches(&src) {
            if sqp_fmt::spans::audit_batch(&batch, &mut audit).is_err() {
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

/// `sqp-fmt [--config FILE] [--dialect NAME] [--stdin-filepath PATH] [FILE | -]`:
/// format FILE (or stdin) to stdout. Batches that can't be formatted safely
/// come out exactly as written.
///
/// The configuration is `--config FILE`, else the first `sqp-fmt.toml` in
/// the formatted file's directory or one above it (for stdin: from
/// `--stdin-filepath`'s directory, else the current directory), else the
/// user's `$XDG_CONFIG_HOME/sqp-fmt/sqp-fmt.toml`, else the defaults. The
/// dialect is `--dialect`, else the configuration's `[[dialect_paths]]`
/// entry for the file, else its `dialect`. A bad configuration is an error:
/// nothing is written to stdout.
fn format_cmd(args: &[String]) -> ExitCode {
    use std::io::{Read, Write};
    let mut config: Option<PathBuf> = None;
    let mut dialect: Option<String> = None;
    let mut stdin_path: Option<PathBuf> = None;
    let mut mode = Mode::Stdout;
    let mut paths: Vec<PathBuf> = Vec::new();
    let mut stdin_dash = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let (flag, inline) = match a.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f, Some(v.to_string())),
            _ => (a.as_str(), None),
        };
        let mut value = || inline.clone().or_else(|| it.next().cloned());
        match flag {
            "--config" => match value() {
                Some(v) => config = Some(PathBuf::from(v)),
                None => return usage(),
            },
            "--dialect" => match value() {
                Some(v) => dialect = Some(v),
                None => return usage(),
            },
            "--stdin-filepath" => match value() {
                Some(v) => stdin_path = Some(PathBuf::from(v)),
                None => return usage(),
            },
            "--write" | "--check" if mode == Mode::Stdout && inline.is_none() => {
                mode = if flag == "--write" {
                    Mode::Write
                } else {
                    Mode::Check
                };
            }
            "-" => stdin_dash = true,
            f if !f.starts_with('-') => paths.push(PathBuf::from(a)),
            _ => return usage(),
        }
    }
    if mode != Mode::Stdout {
        if paths.is_empty() || stdin_dash || stdin_path.is_some() {
            eprintln!(
                "sqp-fmt: --write and --check take files or directories (not stdin), and only one of them"
            );
            return ExitCode::from(2);
        }
        return format_files(&paths, mode, config.as_deref(), dialect.as_deref());
    }
    if paths.len() + usize::from(stdin_dash) > 1 {
        eprintln!("sqp-fmt: formats one file to stdout; use --write or --check for several");
        return ExitCode::from(2);
    }
    let file = paths.pop();
    if file.is_some() && stdin_path.is_some() {
        eprintln!("sqp-fmt: --stdin-filepath only applies when reading stdin");
        return ExitCode::from(2);
    }
    let named = file.as_deref().or(stdin_path.as_deref());
    let Ok(cfg) = file_config(config.as_deref(), dialect.as_deref(), named) else {
        return ExitCode::from(2);
    };
    // A panic is caught per batch and that batch left as written; keep the
    // panic message off stderr, which editors show to the user.
    std::panic::set_hook(Box::new(|_| {}));
    let mut src = String::new();
    let read = match &file {
        Some(p) => std::fs::read_to_string(p).map(|s| src = s),
        None => std::io::stdin().read_to_string(&mut src).map(|_| ()),
    };
    if let Err(e) = read {
        eprintln!("sqp-fmt: {e}");
        return ExitCode::FAILURE;
    }
    let (out, _) = sqp_fmt::format_with(&src, &cfg);
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// Format one file (or stdin) to stdout.
    Stdout,
    /// `--write`: format files in place.
    Write,
    /// `--check`: report the files formatting would change.
    Check,
}

/// The configuration for a file (or for stdin named by `--stdin-filepath`):
/// `--config`, else found from the file's directory, then specialised for
/// its dialect. Errors are printed.
fn file_config(
    config: Option<&Path>,
    dialect: Option<&str>,
    named: Option<&Path>,
) -> Result<sqp_fmt::Config, ()> {
    let cfg = match config {
        Some(c) => sqp_fmt::Config::load(c),
        None => {
            let dir = named
                .and_then(Path::parent)
                .map(Path::to_path_buf)
                .unwrap_or_default();
            let user = sqp_fmt::config::user_config();
            sqp_fmt::Config::discover(&dir, user.as_deref()).map(|(c, _)| c)
        }
    };
    let cfg = cfg.map_err(|e| eprintln!("sqp-fmt: {e}"))?;
    let dialect = resolve_dialect(dialect, &cfg, named).ok_or(())?;
    Ok(cfg.for_dialect(dialect))
}

/// `--write` / `--check` over files and directories. Directories are
/// searched for `.sql` files as `corpus` does; a file named explicitly is
/// formatted whatever its extension. Each file gets its own configuration
/// and dialect, found as for a single file.
///
/// Prints the files that changed (`--write`) or would change (`--check`) on
/// stdout, one per line; on stderr, a summary and the files with batches
/// left as written. Exit status: 2 if a file couldn't be found, read,
/// written or configured; else, for `--check`, 1 if any file would change;
/// else 0. Batches that don't parse are left as written and aren't errors.
fn format_files(
    args: &[PathBuf],
    mode: Mode,
    config: Option<&Path>,
    dialect: Option<&str>,
) -> ExitCode {
    // As for one file: a panic leaves its batch as written, silently.
    std::panic::set_hook(Box::new(|_| {}));
    let mut files = Vec::new();
    let mut failed = 0;
    for a in args {
        if a.is_dir() {
            collect(a, &mut files);
        } else if a.exists() {
            files.push(a.clone());
        } else {
            eprintln!("sqp-fmt: {}: no such file or directory", a.display());
            failed += 1;
        }
    }
    let (mut changed, mut unchanged) = (0, 0);
    for file in &files {
        let Ok(cfg) = file_config(config, dialect, Some(file)) else {
            failed += 1;
            continue;
        };
        let src = match std::fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("sqp-fmt: {}: {e}", file.display());
                failed += 1;
                continue;
            }
        };
        let (out, stats) = sqp_fmt::format_with(&src, &cfg);
        let left = stats.unparsed + stats.mismatches + stats.errors;
        if left > 0 {
            eprintln!(
                "sqp-fmt: {}: {left} of {} batches left as written (they don't parse, or formatting them wasn't safe)",
                file.display(),
                stats.batches
            );
        }
        if out == src {
            unchanged += 1;
            continue;
        }
        if mode == Mode::Write
            && let Err(e) = write_in_place(file, &out)
        {
            eprintln!("sqp-fmt: {}: {e}", file.display());
            failed += 1;
            continue;
        }
        changed += 1;
        println!("{}", file.display());
    }
    let verb = if mode == Mode::Write {
        "formatted"
    } else {
        "would be formatted"
    };
    let failures = if failed > 0 {
        format!(", {failed} failed")
    } else {
        String::new()
    };
    eprintln!(
        "sqp-fmt: {changed} file{} {verb}, {unchanged} unchanged{failures}",
        if changed == 1 { "" } else { "s" }
    );
    if failed > 0 {
        ExitCode::from(2)
    } else if mode == Mode::Check && changed > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Replaces `path`'s contents with `text` atomically: a temporary file in
/// the same directory, given the original's permissions, renamed over it. A
/// symlink is followed, so it stays a symlink.
fn write_in_place(path: &Path, text: &str) -> std::io::Result<()> {
    let target = std::fs::canonicalize(path)?;
    let dir = target.parent().unwrap_or(Path::new("."));
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp = dir.join(format!(".{name}.sqp-fmt-{}.tmp", std::process::id()));
    let result = std::fs::metadata(&target).and_then(|meta| {
        std::fs::write(&tmp, text)?;
        std::fs::set_permissions(&tmp, meta.permissions())?;
        std::fs::rename(&tmp, &target)
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

/// The dialect for a file: `flag` (`--dialect`), else the configuration's
/// mapping for `path`, else its `dialect`. Prints an error for an unknown
/// `--dialect`.
pub(crate) fn resolve_dialect(
    flag: Option<&str>,
    cfg: &sqp_fmt::Config,
    path: Option<&Path>,
) -> Option<&'static str> {
    match flag {
        Some(f) => {
            let d = sqp_fmt::config::canonical_dialect(f);
            if d.is_none() {
                eprintln!(
                    "sqp-fmt: --dialect: unknown dialect \"{f}\" (one of: {})",
                    sqp_fmt::config::DIALECTS.join(", ")
                );
            }
            d
        }
        None => {
            Some(sqp_fmt::config::canonical_dialect(cfg.dialect_for(path)).unwrap_or("generic"))
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("check") => check(&args[1..]),
        Some("spans") => spans(&args[1..]),
        Some("corpus") => corpus::run(&args[1..]),
        Some("-h" | "--help" | "help") => usage(),
        _ => format_cmd(&args),
    }
}
