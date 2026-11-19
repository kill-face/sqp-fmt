//! `tsqlfmt check <path>...`: parse every .sql file under the paths and
//! report which fail, grouped by parser error. This is the progress measure
//! for the grammar work in the sqlparser-rs fork.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn usage() -> ExitCode {
    eprintln!("usage: tsqlfmt check [--files] <path>...");
    ExitCode::from(2)
}

/// Hidden dirs (.git, .direnv, .idea, ...) and build output hold copies of
/// the same files. Only applies below the paths given on the command line.
fn skip_dir(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.starts_with('.') || matches!(name, "bin" | "obj" | "node_modules" | "target")
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
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
    } else if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("sql")) {
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

    println!("{ok}/{} files parse ({} fail)", files.len(), files.len() - ok);
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
    if ok == files.len() { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("check") => check(&args[1..]),
        _ => usage(),
    }
}
