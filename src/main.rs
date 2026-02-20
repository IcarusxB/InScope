use clap::{error::ErrorKind, Args, Parser, Subcommand};
use glob::{MatchOptions, glob_with};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Parser, Debug)]
#[command(name = "inscope")]
#[command(about = "Audit scope manager for in-scope/out-of-scope paths")]
#[command(
    after_help = "Examples:\n  inscope init\n  inscope in --add 'contracts/**/*.sol'\n  inscope out --add contracts/protocols/dex/Bad.sol\n  inscope sync --dry-run"
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    #[command(about = "Create audit-scope.yml in the current directory")]
    Init,
    #[command(about = "List both IN and OUT entries, conflicts, and overrides")]
    List,
    #[command(about = "Show manifest status summary")]
    Status,
    #[command(about = "Apply or remove scope markers for IN entries")]
    Annotate {
        #[arg(long, help = "Apply scope markers")]
        apply: bool,
        #[arg(long, help = "Remove scope markers")]
        remove: bool,
        #[arg(long, help = "Show planned changes without writing files")]
        dry_run: bool,
    },
    #[command(about = "Reconcile files to match scope config (recursive, OUT wins)")]
    Sync {
        #[arg(long, help = "Show planned changes without writing files")]
        dry_run: bool,
    },
    #[command(about = "Manage IN scope entries")]
    In(ScopeActionArgs),
    #[command(about = "Manage OUT scope entries")]
    Out(ScopeActionArgs),
    #[command(hide = true)]
    Add { path: String },
    #[command(hide = true)]
    Remove { path: String },
}

#[derive(Args, Debug)]
struct ScopeActionArgs {
    #[arg(
        long,
        value_name = "PATH",
        help = "Add a path (file, directory, or glob)"
    )]
    add: Option<String>,
    #[arg(long, value_name = "PATH", help = "Remove a path")]
    remove: Option<String>,
    #[arg(long, help = "List current entries")]
    list: bool,
}

#[derive(Clone, Copy)]
enum ScopeTarget {
    In,
    Out,
}

#[derive(Debug, Serialize, Deserialize)]
struct ScopeConfig {
    version: u8,
    include: Vec<String>,
    exclude: Vec<String>,
    files: BTreeMap<String, String>,
}

fn main() {
    print_box_header();
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(err) => {
            let code = match err.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => 0,
                _ => 2,
            };

            match err.kind() {
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion => print!("{err}"),
                _ => eprint!("{err}"),
            }

            print_box_footer();
            std::process::exit(code);
        }
    };

    match cli.command {
        Commands::Init => init(),
        Commands::List => list(),
        Commands::Status => status(),
        Commands::Annotate {
            apply,
            remove,
            dry_run,
        } => annotate(apply, remove, dry_run),
        Commands::Sync { dry_run } => sync(dry_run),
        Commands::In(args) => handle_scope_action(ScopeTarget::In, args),
        Commands::Out(args) => handle_scope_action(ScopeTarget::Out, args),
        Commands::Add { path } => add_scope_entry(ScopeTarget::In, path),
        Commands::Remove { path } => remove_scope_entry(ScopeTarget::In, path),
    }

    print_box_footer();
}

fn handle_scope_action(target: ScopeTarget, args: ScopeActionArgs) {
    if let Some(path) = args.add {
        add_scope_entry(target, path);
        return;
    }
    if let Some(path) = args.remove {
        remove_scope_entry(target, path);
        return;
    }
    if args.list {
        list_scope_entries(target);
        return;
    }
    fail("choose one of: --add <PATH>, --remove <PATH>, --list");
}

fn print_box_header() {
    println!("--------InScope---------");
}

fn print_box_footer() {
    println!("-------------------------");
}

fn fail(message: impl AsRef<str>) -> ! {
    println!("{}", message.as_ref());
    print_box_footer();
    std::process::exit(1);
}

fn scopeconfig_path() -> &'static Path {
    Path::new("audit-scope.yml")
}

fn load_scope_config(path: &Path) -> ScopeConfig {
    let raw = fs::read_to_string(path)
        .unwrap_or_else(|e| fail(format!("failed to read {}: {}", path.display(), e)));
    serde_yaml::from_str(&raw)
        .unwrap_or_else(|e| fail(format!("invalid {}: {}", path.display(), e)))
}

fn save_scope_config(path: &Path, scopeconfig: &ScopeConfig) {
    let out = serde_yaml::to_string(scopeconfig)
        .unwrap_or_else(|e| fail(format!("failed to serialize manifest: {}", e)));
    fs::write(path, out)
        .unwrap_or_else(|e| fail(format!("failed to write {}: {}", path.display(), e)));
}

fn require_scopeconfig() -> ScopeConfig {
    let path = scopeconfig_path();
    if !path.exists() {
        fail("audit-scope.yml not found. Run `inscope init` first.");
    }
    load_scope_config(path)
}

fn target_label(target: ScopeTarget) -> &'static str {
    match target {
        ScopeTarget::In => "in",
        ScopeTarget::Out => "out",
    }
}

fn entry_list(scopeconfig: &ScopeConfig, target: ScopeTarget) -> &Vec<String> {
    match target {
        ScopeTarget::In => &scopeconfig.include,
        ScopeTarget::Out => &scopeconfig.exclude,
    }
}

fn entry_list_mut(scopeconfig: &mut ScopeConfig, target: ScopeTarget) -> &mut Vec<String> {
    match target {
        ScopeTarget::In => &mut scopeconfig.include,
        ScopeTarget::Out => &mut scopeconfig.exclude,
    }
}

fn normalize_entry(entry: &str) -> String {
    let trimmed = entry.trim();
    if trimmed == "/" {
        return "/".to_string();
    }
    let normalized = trimmed.trim_end_matches('/');
    if normalized.is_empty() {
        trimmed.to_string()
    } else {
        normalized.to_string()
    }
}

fn init() {
    let path = scopeconfig_path();
    if path.exists() {
        fail("audit-scope.yml already exists");
    }

    fs::write(path, "version: 1\ninclude: []\nexclude: []\nfiles: {}\n")
        .unwrap_or_else(|e| fail(format!("failed to create {}: {}", path.display(), e)));
    println!("created {}", path.display());
}

fn add_scope_entry(target: ScopeTarget, path: String) {
    let path = path.trim();
    if path.is_empty() {
        fail("path cannot be empty");
    }

    let mut scopeconfig = require_scopeconfig();
    let normalized = normalize_entry(path);
    let label = target_label(target);
    let entries = entry_list_mut(&mut scopeconfig, target);

    if entries
        .iter()
        .any(|existing| normalize_entry(existing) == normalized)
    {
        println!("already in {} list: {}", label, path);
        return;
    }

    entries.push(path.to_string());
    entries.sort();
    save_scope_config(scopeconfig_path(), &scopeconfig);
    println!("added to {} list: {}", label, path);
}

fn remove_scope_entry(target: ScopeTarget, path: String) {
    let path = path.trim();
    if path.is_empty() {
        fail("path cannot be empty");
    }

    let mut scopeconfig = require_scopeconfig();
    let normalized = normalize_entry(path);
    let label = target_label(target);
    let entries = entry_list_mut(&mut scopeconfig, target);
    let before = entries.len();

    entries.retain(|entry| normalize_entry(entry) != normalized);

    if entries.len() == before {
        println!("not in {} list: {}", label, path);
        return;
    }

    save_scope_config(scopeconfig_path(), &scopeconfig);
    println!("removed from {} list: {}", label, path);
}

fn list_scope_entries(target: ScopeTarget) {
    let scopeconfig = require_scopeconfig();
    let entries = entry_list(&scopeconfig, target);
    let label = target_label(target);

    println!("{} entries ({}):", label, entries.len());
    if entries.is_empty() {
        println!("- none");
        return;
    }

    for entry in entries {
        println!("- {}", entry);
    }
}

fn list() {
    let scopeconfig = require_scopeconfig();

    println!("IN entries ({}):", scopeconfig.include.len());
    if scopeconfig.include.is_empty() {
        println!("- none");
    } else {
        for entry in &scopeconfig.include {
            println!("- {}", entry);
        }
    }

    println!();
    println!("OUT entries ({}):", scopeconfig.exclude.len());
    if scopeconfig.exclude.is_empty() {
        println!("- none");
    } else {
        for entry in &scopeconfig.exclude {
            println!("- {}", entry);
        }
    }

    println!();
    let conflicts = find_exact_conflicts(&scopeconfig.include, &scopeconfig.exclude);
    if conflicts.is_empty() {
        println!("conflicts: none");
    } else {
        println!("conflicts (exact same path in both):");
        for conflict in conflicts {
            println!("- {}", conflict);
        }
    }

    println!();
    let overrides = find_overrides(&scopeconfig.include, &scopeconfig.exclude);
    if overrides.is_empty() {
        println!("overrides: none");
    } else {
        println!("overrides (out wins over in):");
        for note in overrides {
            println!("- {}", note);
        }
    }
}

fn find_exact_conflicts(in_entries: &[String], out_entries: &[String]) -> BTreeSet<String> {
    let in_set: HashSet<String> = in_entries
        .iter()
        .map(|entry| normalize_entry(entry))
        .collect();
    let mut conflicts = BTreeSet::new();

    for out_entry in out_entries {
        let normalized = normalize_entry(out_entry);
        if in_set.contains(&normalized) {
            conflicts.insert(normalized);
        }
    }

    conflicts
}

fn find_overrides(in_entries: &[String], out_entries: &[String]) -> BTreeSet<String> {
    let conflicts = find_exact_conflicts(in_entries, out_entries);
    let mut overrides = BTreeSet::new();

    for out_entry in out_entries {
        if looks_like_glob(out_entry) {
            continue;
        }

        let out_norm = normalize_entry(out_entry);
        if conflicts.contains(&out_norm) {
            continue;
        }

        let out_path = PathBuf::from(&out_norm);

        for in_entry in in_entries {
            if looks_like_glob(in_entry) || !is_directory_entry(in_entry) {
                continue;
            }

            let in_norm = normalize_entry(in_entry);
            if in_norm == out_norm {
                continue;
            }

            let in_path = PathBuf::from(&in_norm);
            if out_path.starts_with(&in_path) {
                overrides.insert(format!("OUT {} overrides IN {}", out_entry, in_entry));
            }
        }
    }

    overrides
}

fn is_directory_entry(entry: &str) -> bool {
    entry.ends_with('/') || Path::new(entry).is_dir()
}

fn status() {
    let scopeconfig = require_scopeconfig();
    let path = scopeconfig_path();

    println!("InScope status");
    println!("manifest: {}", path.display());
    println!("version: {}", scopeconfig.version);
    println!("include entries: {}", scopeconfig.include.len());
    println!("exclude entries: {}", scopeconfig.exclude.len());
    println!("file overrides: {}", scopeconfig.files.len());

    if scopeconfig.include.is_empty() {
        println!("warning: include is empty (nothing currently in scope)");
    } else {
        println!("ok: scope has entries");
    }
}

fn annotate(apply: bool, remove: bool, dry_run: bool) {
    match (apply, remove) {
        (true, false) => {}
        (false, true) => {}
        (false, false) => fail("choose one: --apply or --remove"),
        (true, true) => fail("use only one flag: --apply or --remove"),
    }

    let scopeconfig = require_scopeconfig();
    let mut changed = 0usize;
    let mut skipped = 0usize;
    let mut errors = 0usize;

    let include_files = collect_files_from_entries(&scopeconfig.include, &mut skipped);
    let exclude_files = collect_files_from_entries(&scopeconfig.exclude, &mut skipped);

    for file in include_files {
        if apply && exclude_files.contains(&file) {
            println!("skip (out wins over in): {}", file.display());
            skipped += 1;
            continue;
        }

        let file_display = file.display().to_string();
        let original = match read_scope_text_file(&file) {
            Ok(Some(s)) => s,
            Ok(None) => {
                println!("skip (non-text or system file): {}", file_display);
                skipped += 1;
                continue;
            }
            Err(e) => {
                println!("error reading {}: {}", file_display, e);
                errors += 1;
                continue;
            }
        };

        if apply {
            let marker = match marker_for_file(&file, &original) {
                Some(marker) => marker,
                None => {
                    println!("skip (unsupported file type): {}", file_display);
                    skipped += 1;
                    continue;
                }
            };

            let (updated, did_change) = apply_scope_marker(&original, marker);
            if !did_change {
                println!("already annotated: {}", file_display);
                continue;
            }

            if dry_run {
                println!("would annotate: {}", file_display);
                changed += 1;
                continue;
            }

            match fs::write(&file, updated) {
                Ok(_) => {
                    println!("annotated: {}", file_display);
                    changed += 1;
                }
                Err(e) => {
                    println!("error writing {}: {}", file_display, e);
                    errors += 1;
                }
            }
        } else {
            let (updated, did_change) = remove_scope_marker(&original);
            if !did_change {
                println!("no scope marker: {}", file_display);
                continue;
            }

            if dry_run {
                println!("would remove marker: {}", file_display);
                changed += 1;
                continue;
            }

            match fs::write(&file, updated) {
                Ok(_) => {
                    println!("removed marker: {}", file_display);
                    changed += 1;
                }
                Err(e) => {
                    println!("error writing {}: {}", file_display, e);
                    errors += 1;
                }
            }
        }
    }

    if dry_run {
        println!(
            "annotate dry-run done: would_change={}, skipped={}, errors={}",
            changed, skipped, errors
        );
    } else {
        println!(
            "annotate done: changed={}, skipped={}, errors={}",
            changed, skipped, errors
        );
    }

    if errors > 0 {
        fail("annotate finished with errors");
    }
}

fn sync(dry_run: bool) {
    let scopeconfig = require_scopeconfig();
    let mut changed = 0usize;
    let mut skipped = 0usize;
    let mut errors = 0usize;

    let include_files = collect_files_from_entries(&scopeconfig.include, &mut skipped);
    let exclude_files = collect_files_from_entries(&scopeconfig.exclude, &mut skipped);

    println!("IN pass:");
    for file in &include_files {
        if exclude_files.contains(file) {
            println!("skip (out wins over in): {}", file.display());
            skipped += 1;
            continue;
        }

        let file_display = file.display().to_string();
        let original = match read_scope_text_file(file) {
            Ok(Some(s)) => s,
            Ok(None) => {
                println!("skip (non-text or system file): {}", file_display);
                skipped += 1;
                continue;
            }
            Err(e) => {
                println!("error reading {}: {}", file_display, e);
                errors += 1;
                continue;
            }
        };

        let marker = match marker_for_file(file, &original) {
            Some(marker) => marker,
            None => {
                println!("skip (unsupported file type): {}", file_display);
                skipped += 1;
                continue;
            }
        };

        let (updated, did_change) = apply_scope_marker(&original, marker);
        if !did_change {
            continue;
        }

        if dry_run {
            println!("would sync +annotate: {}", file_display);
            changed += 1;
            continue;
        }

        match fs::write(file, updated) {
            Ok(_) => {
                println!("synced +annotated: {}", file_display);
                changed += 1;
            }
            Err(e) => {
                println!("error writing {}: {}", file_display, e);
                errors += 1;
            }
        }
    }

    println!();
    println!("OUT pass:");
    for file in &exclude_files {
        let file_display = file.display().to_string();
        let original = match read_scope_text_file(file) {
            Ok(Some(s)) => s,
            Ok(None) => {
                println!("skip (non-text or system file): {}", file_display);
                skipped += 1;
                continue;
            }
            Err(e) => {
                println!("error reading {}: {}", file_display, e);
                errors += 1;
                continue;
            }
        };

        let (updated, did_change) = remove_scope_marker(&original);
        if !did_change {
            continue;
        }

        if dry_run {
            println!("would sync -remove marker: {}", file_display);
            changed += 1;
            continue;
        }

        match fs::write(file, updated) {
            Ok(_) => {
                println!("synced -removed marker: {}", file_display);
                changed += 1;
            }
            Err(e) => {
                println!("error writing {}: {}", file_display, e);
                errors += 1;
            }
        }
    }

    println!();
    if dry_run {
        println!(
            "sync dry-run done: would_change={}, skipped={}, errors={}",
            changed, skipped, errors
        );
    } else {
        println!(
            "sync done: changed={}, skipped={}, errors={}",
            changed, skipped, errors
        );
    }

    if errors > 0 {
        fail("sync finished with errors");
    }
}

fn collect_files_from_entries(entries: &[String], skipped: &mut usize) -> BTreeSet<PathBuf> {
    let mut files = BTreeSet::new();

    for entry in entries {
        if looks_like_glob(entry) {
            let expanded = expand_glob_entry(entry);
            if expanded.is_empty() {
                println!("skip (glob matched no files): {}", entry);
                *skipped += 1;
                continue;
            }

            for file in expanded {
                files.insert(file);
            }
            continue;
        }

        let entry_path = Path::new(entry);
        if entry_path.is_dir() && is_ignored_path(entry_path) {
            println!("skip (ignored directory): {}", entry);
            *skipped += 1;
            continue;
        }
        let entry_files = collect_entry_files(entry_path);

        if entry_files.is_empty() {
            println!("skip (not found / no files): {}", entry);
            *skipped += 1;
            continue;
        }

        for file in entry_files {
            let rel = relativize_to_cwd(&file);
            files.insert(rel);
        }
    }

    files
}

fn expand_glob_entry(pattern: &str) -> Vec<PathBuf> {
    let mut files = BTreeSet::new();
    let opts = MatchOptions {
        case_sensitive: true,
        require_literal_separator: false,
        require_literal_leading_dot: false,
    };

    let paths = match glob_with(pattern, opts) {
        Ok(paths) => paths,
        Err(e) => {
            println!("skip (invalid glob '{}'): {}", pattern, e);
            return Vec::new();
        }
    };

    for entry in paths {
        let path = match entry {
            Ok(path) => path,
            Err(e) => {
                println!("skip (glob access error '{}'): {}", pattern, e);
                continue;
            }
        };

        if is_ignored_path(&path) {
            continue;
        }

        if path.is_file() {
            files.insert(relativize_to_cwd(&path));
            continue;
        }

        if path.is_dir() {
            for file in collect_entry_files(&path) {
                files.insert(relativize_to_cwd(&file));
            }
        }
    }

    files.into_iter().collect()
}

fn collect_entry_files(entry: &Path) -> Vec<PathBuf> {
    if entry.is_file() {
        return vec![entry.to_path_buf()];
    }

    if !entry.is_dir() {
        return Vec::new();
    }

    let mut stack = vec![entry.to_path_buf()];
    let mut files = Vec::new();

    while let Some(dir) = stack.pop() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for item in entries.flatten() {
            let file_type = match item.file_type() {
                Ok(file_type) => file_type,
                Err(_) => continue,
            };
            let path = item.path();

            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_file() {
                files.push(path);
            } else if file_type.is_dir() {
                if is_ignored_path(&path) {
                    continue;
                }
                stack.push(path);
            }
        }
    }

    files.sort();
    files
}

fn looks_like_glob(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[') || s.contains('{')
}

fn is_ignored_path(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component.as_os_str().to_str(),
            Some(".git") | Some("node_modules") | Some("target")
        )
    })
}

fn relativize_to_cwd(path: &Path) -> PathBuf {
    if path.is_absolute() {
        if let Ok(cwd) = std::env::current_dir() {
            if let Ok(rel) = path.strip_prefix(cwd) {
                return normalize_rel_path(rel);
            }
        }
    }
    normalize_rel_path(path)
}

fn normalize_rel_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        if component == std::path::Component::CurDir {
            continue;
        }
        out.push(component.as_os_str());
    }
    out
}

fn marker_for_file(path: &Path, content: &str) -> Option<&'static str> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase());

    match ext.as_deref() {
        Some("rs") | Some("js") | Some("ts") | Some("jsx") | Some("tsx") | Some("sol")
        | Some("c") | Some("cpp") | Some("h") | Some("hpp") | Some("java") | Some("go")
        | Some("swift") | Some("kt") => Some("// @scope"),
        Some("py") | Some("sh") | Some("bash") | Some("zsh") | Some("rb") | Some("yml")
        | Some("yaml") | Some("toml") => Some("# @scope"),
        Some("sql") | Some("lua") => Some("-- @scope"),
        Some("css") => Some("/* @scope */"),
        Some("html") | Some("xml") | Some("md") => Some("<!-- @scope -->"),
        Some("json") => None,
        _ => {
            if content.starts_with("#!") {
                Some("# @scope")
            } else {
                None
            }
        }
    }
}

fn apply_scope_marker(content: &str, marker: &str) -> (String, bool) {
    if has_scope_marker(content) {
        return (content.to_string(), false);
    }

    let marker_line = format!("{marker}\n");
    if content.starts_with("#!") {
        if let Some(pos) = content.find('\n') {
            let mut out = String::with_capacity(content.len() + marker_line.len());
            out.push_str(&content[..pos + 1]);
            out.push_str(&marker_line);
            out.push_str(&content[pos + 1..]);
            (out, true)
        } else {
            (format!("{content}\n{marker}\n"), true)
        }
    } else {
        (format!("{marker_line}{content}"), true)
    }
}

fn remove_scope_marker(content: &str) -> (String, bool) {
    let had_trailing_newline = content.ends_with('\n');
    let mut lines: Vec<&str> = content.lines().collect();

    if lines.is_empty() {
        return (content.to_string(), false);
    }

    let removed = if line_is_scope_marker(lines[0]) {
        lines.remove(0);
        true
    } else if lines[0].starts_with("#!") && lines.len() > 1 && line_is_scope_marker(lines[1]) {
        lines.remove(1);
        true
    } else {
        false
    };

    if !removed {
        return (content.to_string(), false);
    }

    let mut out = lines.join("\n");
    if had_trailing_newline && !out.is_empty() {
        out.push('\n');
    }
    (out, true)
}

fn has_scope_marker(content: &str) -> bool {
    let mut lines = content.lines();

    if let Some(first) = lines.next() {
        if line_is_scope_marker(first) {
            return true;
        }
        if first.starts_with("#!") {
            if let Some(second) = lines.next() {
                return line_is_scope_marker(second);
            }
        }
    }

    false
}

fn line_is_scope_marker(line: &str) -> bool {
    matches!(
        line.trim(),
        "// @scope"
            | "//@scope"
            | "# @scope"
            | "#@scope"
            | "-- @scope"
            | "--@scope"
            | "/* @scope */"
            | "/*@scope*/"
            | "<!-- @scope -->"
            | "<!--@scope-->"
    )
}

fn read_scope_text_file(path: &Path) -> Result<Option<String>, std::io::Error> {
    if matches!(
        path.file_name().and_then(|n| n.to_str()),
        Some(".DS_Store")
    ) {
        return Ok(None);
    }

    match fs::read_to_string(path) {
        Ok(contents) => Ok(Some(contents)),
        Err(err) if err.kind() == std::io::ErrorKind::InvalidData => Ok(None),
        Err(err) => Err(err),
    }
}
