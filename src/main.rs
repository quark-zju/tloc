#[allow(dead_code, unused)]
mod ascii_tree;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use ascii_tree::{AsciiOptions, DescribeTreeSpan, Tree, TreeSpan};
use globset::{GlobBuilder, GlobSet, GlobSetBuilder};
use pico_args::Arguments;
use tokei::{Config, LanguageType};

#[derive(Clone, Copy, Debug, Default)]
struct LineStats {
    files: usize,
    lines: usize,
    code: usize,
    comments: usize,
    blanks: usize,
}

impl std::ops::AddAssign for LineStats {
    fn add_assign(&mut self, rhs: Self) {
        self.files += rhs.files;
        self.lines += rhs.lines;
        self.code += rhs.code;
        self.comments += rhs.comments;
        self.blanks += rhs.blanks;
    }
}

#[derive(Debug, Default)]
struct DirNode {
    stats: LineStats,
    language_lines: BTreeMap<String, usize>,
    children: BTreeMap<String, DirNode>,
}

impl DirNode {
    fn add_file(&mut self, components: &[String], stats: LineStats, language: &str) {
        self.stats += stats;
        *self.language_lines.entry(language.to_string()).or_default() += stats.lines;
        if let Some((head, tail)) = components.split_first() {
            self.children
                .entry(head.clone())
                .or_default()
                .add_file(tail, stats, language);
        }
    }
}

#[derive(Debug)]
struct Cli {
    roots: Vec<PathBuf>,
    language_filter: Option<BTreeSet<LanguageType>>,
    exclude_matcher: GlobSet,
    min_root_code_percentage_to_hide: u8,
}

fn main() {
    let cli = match parse_cli() {
        Ok(cli) => cli,
        Err(err) => {
            eprintln!("{err}");
            std::process::exit(2);
        }
    };

    let aggregate = match collect_directory_stats(&cli) {
        Ok(stats) => stats,
        Err(err) => {
            eprintln!("Failed to collect stats: {err}");
            std::process::exit(1);
        }
    };

    if aggregate.stats.files == 0 {
        println!("No matching source files found.");
        return;
    }

    print!(
        "{}",
        render_ascii_tree(
            &aggregate,
            cli.min_root_code_percentage_to_hide,
            root_render_name(&cli.roots),
        )
    );
}

fn parse_cli() -> Result<Cli, String> {
    let mut pargs = Arguments::from_env();

    if pargs.contains(["-h", "--help"]) {
        print_help();
        std::process::exit(0);
    }

    let mut raw_language_values = Vec::new();
    while let Some(value) = pargs
        .opt_value_from_str::<_, String>("-L")
        .map_err(|e| e.to_string())?
    {
        raw_language_values.push(value);
    }
    while let Some(value) = pargs
        .opt_value_from_str::<_, String>("--languages")
        .map_err(|e| e.to_string())?
    {
        raw_language_values.push(value);
    }

    let mut raw_excluded_paths = Vec::new();
    while let Some(value) = pargs
        .opt_value_from_str::<_, String>("-X")
        .map_err(|e| e.to_string())?
    {
        raw_excluded_paths.push(value);
    }
    while let Some(value) = pargs
        .opt_value_from_str::<_, String>("--exclude")
        .map_err(|e| e.to_string())?
    {
        raw_excluded_paths.push(value);
    }

    let min_root_code_percentage_to_hide = parse_root_code_hide_percentage(
        pargs
            .opt_value_from_str::<_, u8>("-p")
            .map_err(|e| e.to_string())?,
        pargs
            .opt_value_from_str::<_, u8>("--hide-below")
            .map_err(|e| e.to_string())?,
    )?;

    let free = pargs.finish();
    let roots = if free.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        free.into_iter().map(PathBuf::from).collect()
    };

    let language_filter = parse_language_filter(&raw_language_values)?;
    let exclude_matcher = build_exclude_matcher(&raw_excluded_paths)?;

    Ok(Cli {
        roots,
        language_filter,
        exclude_matcher,
        min_root_code_percentage_to_hide,
    })
}

fn parse_language_filter(raw_values: &[String]) -> Result<Option<BTreeSet<LanguageType>>, String> {
    if raw_values.is_empty() {
        return Ok(None);
    }

    let mut filter = BTreeSet::new();
    let mut unknown = Vec::new();

    for token in raw_values
        .iter()
        .flat_map(|v| v.split(','))
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        if let Some(lang) = LanguageType::from_name(token).or_else(|| token.parse().ok()) {
            filter.insert(lang);
        } else {
            unknown.push(token.to_string());
        }
    }

    if !unknown.is_empty() {
        return Err(format!("Unknown language(s): {}", unknown.join(", ")));
    }

    if filter.is_empty() {
        return Err("-L/--languages was set but no language names were provided".to_string());
    }

    Ok(Some(filter))
}

fn build_exclude_matcher(raw_patterns: &[String]) -> Result<GlobSet, String> {
    let mut builder = GlobSetBuilder::new();

    for pattern in raw_patterns {
        if pattern.is_empty() {
            return Err("-X/--exclude was set but no glob was provided".to_string());
        }

        if Path::new(pattern).is_absolute() {
            return Err(format!(
                "-X/--exclude expects a glob relative to the current directory, got {pattern}"
            ));
        }

        let pattern = normalize_relative_pattern(pattern);
        add_exclude_glob(&mut builder, &pattern)?;
        add_exclude_glob(&mut builder, &format!("{pattern}/**"))?;
    }

    builder
        .build()
        .map_err(|err| format!("Invalid -X/--exclude glob: {err}"))
}

fn add_exclude_glob(builder: &mut GlobSetBuilder, pattern: &str) -> Result<(), String> {
    let glob = GlobBuilder::new(pattern)
        .literal_separator(true)
        .build()
        .map_err(|err| format!("Invalid -X/--exclude glob {pattern}: {err}"))?;
    builder.add(glob);
    Ok(())
}

fn parse_root_code_hide_percentage(
    short: Option<u8>,
    hide_below: Option<u8>,
) -> Result<u8, String> {
    if short.is_some() && hide_below.is_some() {
        return Err("Use only one of -p or --hide-below, not both".to_string());
    }

    let value = hide_below.or(short).unwrap_or(10);
    if value > 100 {
        return Err(format!(
            "min root code percentage to hide must be in 0..=100, got {value}"
        ));
    }

    Ok(value)
}

fn collect_directory_stats(cli: &Cli) -> io::Result<DirNode> {
    let mut root = DirNode::default();
    let config = Config::default();
    let root_labels = compute_root_labels(&cli.roots);

    for (root_path, root_label) in cli.roots.iter().zip(root_labels.into_iter()) {
        let root_path = if root_path.as_os_str().is_empty() {
            Path::new(".")
        } else {
            root_path.as_path()
        };
        let root_metadata = match fs::metadata(root_path) {
            Ok(meta) => meta,
            Err(err) => {
                eprintln!("Skipping {}: {err}", root_path.display());
                continue;
            }
        };

        if root_metadata.is_file() {
            if is_excluded_path(root_path, &cli.exclude_matcher) {
                continue;
            }

            process_file(
                root_path,
                root_path.parent().unwrap_or_else(|| Path::new(".")),
                &root_label,
                &config,
                &cli.language_filter,
                &mut root,
            );
            continue;
        }

        walk_dir(
            root_path,
            root_path,
            &root_label,
            &config,
            &cli.language_filter,
            &cli.exclude_matcher,
            &mut root,
        )?;
    }

    Ok(root)
}

fn compute_root_labels(roots: &[PathBuf]) -> Vec<Option<String>> {
    if roots.len() == 1 {
        return vec![None];
    }

    roots
        .iter()
        .map(|root| Some(path_display_label(root)))
        .collect()
}

fn root_render_name(roots: &[PathBuf]) -> String {
    if roots.len() > 1 {
        return ".".to_string();
    }

    let root = roots
        .first()
        .map(PathBuf::as_path)
        .unwrap_or_else(|| Path::new("."));

    if root == Path::new(".") {
        if let Ok(current) = std::env::current_dir()
            && let Some(name) = current.file_name()
        {
            let label = name.to_string_lossy().to_string();
            if !label.is_empty() {
                return label;
            }
        }
        return ".".to_string();
    }

    root.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .filter(|label| !label.is_empty())
        .unwrap_or_else(|| path_display_label(root))
}

fn path_display_label(path: &Path) -> String {
    if path.as_os_str().is_empty() {
        return ".".to_string();
    }

    let text = path.to_string_lossy().to_string();
    if text.is_empty() {
        ".".to_string()
    } else {
        text
    }
}

fn walk_dir(
    base: &Path,
    dir: &Path,
    root_label: &Option<String>,
    config: &Config,
    language_filter: &Option<BTreeSet<LanguageType>>,
    exclude_matcher: &GlobSet,
    root: &mut DirNode,
) -> io::Result<()> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) => {
            eprintln!("Skipping {}: {err}", dir.display());
            return Ok(());
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(err) => {
                eprintln!("Skipping directory entry in {}: {err}", dir.display());
                continue;
            }
        };

        let path = entry.path();
        if is_excluded_path(&path, exclude_matcher) {
            continue;
        }

        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(err) => {
                eprintln!("Skipping {}: {err}", path.display());
                continue;
            }
        };

        if file_type.is_symlink() {
            continue;
        }

        if file_type.is_dir() {
            if is_dot_prefixed_dir(&entry) {
                continue;
            }
            walk_dir(
                base,
                &path,
                root_label,
                config,
                language_filter,
                exclude_matcher,
                root,
            )?;
        } else if file_type.is_file() {
            process_file(&path, base, root_label, config, language_filter, root);
        }
    }

    Ok(())
}

fn is_dot_prefixed_dir(entry: &fs::DirEntry) -> bool {
    entry
        .file_name()
        .to_str()
        .map(|name| name.starts_with('.'))
        .unwrap_or(false)
}

fn is_excluded_path(path: &Path, exclude_matcher: &GlobSet) -> bool {
    if exclude_matcher.is_empty() {
        return false;
    }

    let relative_path = path_relative_to_current_dir(path);
    exclude_matcher.is_match(relative_path)
}

fn path_relative_to_current_dir(path: &Path) -> PathBuf {
    let current_dir = std::env::current_dir().ok();
    let relative = current_dir
        .as_deref()
        .and_then(|current_dir| path.strip_prefix(current_dir).ok())
        .unwrap_or(path);

    normalize_relative_path(relative)
}

fn normalize_relative_pattern(pattern: &str) -> String {
    normalize_relative_path(Path::new(pattern))
        .to_string_lossy()
        .replace('\\', "/")
}

fn normalize_relative_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(value) => normalized.push(value),
            Component::ParentDir => {
                if !normalized.pop() {
                    normalized.push("..");
                }
            }
            Component::RootDir | Component::Prefix(_) => {}
        }
    }
    normalized
}

fn process_file(
    file_path: &Path,
    base: &Path,
    root_label: &Option<String>,
    config: &Config,
    language_filter: &Option<BTreeSet<LanguageType>>,
    root: &mut DirNode,
) {
    if is_probably_binary(file_path) {
        return;
    }

    let language = match LanguageType::from_path(file_path, config) {
        Some(language) => language,
        None => return,
    };

    if let Some(filter) = language_filter {
        if !filter.contains(&language) {
            return;
        }
    }

    let report = match language.parse(file_path.to_path_buf(), config) {
        Ok(report) => report,
        Err((err, path)) => {
            eprintln!("Skipping {}: {err}", path.display());
            return;
        }
    };

    let stats = report.stats;
    let file_stats = LineStats {
        files: 1,
        lines: stats.lines(),
        code: stats.code,
        comments: stats.comments,
        blanks: stats.blanks,
    };
    let language_name = language.name().to_string();

    let mut components = Vec::new();
    if let Some(label) = root_label {
        components.push(label.clone());
    }

    let relative = file_path.strip_prefix(base).unwrap_or(file_path);
    for component in relative.components() {
        if let Component::Normal(value) = component {
            components.push(value.to_string_lossy().to_string());
        }
    }

    root.add_file(&components, file_stats, &language_name);
}

fn is_probably_binary(path: &Path) -> bool {
    const CHUNK_SIZE: usize = 8 * 1024;

    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return true,
    };

    let mut buffer = [0_u8; CHUNK_SIZE];
    let read = match file.read(&mut buffer) {
        Ok(read) => read,
        Err(_) => return true,
    };

    buffer[..read].contains(&0)
}

fn print_help() {
    println!(
        r#"tloc - directory-based code line counter

USAGE:
    tloc [OPTIONS] [PATH ...]

OPTIONS:
    -L, --languages <LANGS>    Comma-separated languages to include (repeatable)
    -X, --exclude <GLOB>       Skip a relative glob such as target or **/node_modules (repeatable)
    -p, --hide-below <PCT>     Hide nodes below this % of total code (default: 10)
    -h, --help                 Print help

OUTPUT:
    ASCII directory tree with files/code summary and line breakdown
"#
    );
}

#[derive(Clone, Debug, Default)]
struct RenderNode {
    name: String,
    languages: String,
    stats: LineStats,
}

struct DirTreeDescriptor;

impl DescribeTreeSpan<RenderNode> for DirTreeDescriptor {
    fn name(&self, span: &TreeSpan<RenderNode>) -> String {
        span.extra
            .as_ref()
            .map(|n| n.name.clone())
            .unwrap_or_default()
    }

    fn source_title(&self) -> String {
        "Language".to_string()
    }

    fn source(&self, span: &TreeSpan<RenderNode>) -> String {
        span.extra
            .as_ref()
            .map(|n| n.languages.clone())
            .unwrap_or_default()
    }

    fn code(&self, span: &TreeSpan<RenderNode>) -> String {
        span.extra
            .as_ref()
            .map(|n| n.stats.code.to_string())
            .unwrap_or_default()
    }
    fn code_title(&self) -> String {
        "Code".to_string()
    }

    fn comment(&self, span: &TreeSpan<RenderNode>) -> String {
        span.extra
            .as_ref()
            .map(|n| n.stats.comments.to_string())
            .unwrap_or_default()
    }
    fn comment_title(&self) -> String {
        "Comment".to_string()
    }

    fn blank(&self, span: &TreeSpan<RenderNode>) -> String {
        span.extra
            .as_ref()
            .map(|n| n.stats.blanks.to_string())
            .unwrap_or_default()
    }
    fn blank_title(&self) -> String {
        "Blank".to_string()
    }

    fn start(&self, span: &TreeSpan<RenderNode>) -> String {
        span.extra
            .as_ref()
            .map(|n| n.stats.files.to_string())
            .unwrap_or_default()
    }
    fn start_title(&self) -> String {
        "Files".to_string()
    }

    fn duration(&self, span: &TreeSpan<RenderNode>) -> String {
        span.extra
            .as_ref()
            .map(|n| n.stats.lines.to_string())
            .unwrap_or_default()
    }

    fn duration_title(&self) -> String {
        "LOC".to_string()
    }
}

fn render_ascii_tree(
    aggregate: &DirNode,
    min_root_code_percentage_to_hide: u8,
    root_name: String,
) -> String {
    let mut tree: Tree<RenderNode> = Tree::default();
    // ascii-tree was originally built for profiler output. In tloc, fields like
    // duration carry code-line values for tree filtering rather than time.
    let root_id = tree.push(
        0,
        TreeSpan {
            start_time: aggregate.stats.files as u64,
            duration: aggregate.stats.code as u64,
            extra: Some(RenderNode {
                name: root_name,
                languages: render_language_summary(&aggregate.language_lines),
                stats: aggregate.stats,
            }),
            ..Default::default()
        },
    );

    append_children(&mut tree, root_id, &aggregate.children);

    let min_code_to_hide =
        (aggregate.stats.code as u64 * min_root_code_percentage_to_hide as u64) / 100;
    let options = AsciiOptions {
        min_duration_to_hide: min_code_to_hide,
        ..Default::default()
    };
    let descriptor = DirTreeDescriptor;
    let rows = tree.render_ascii_rows(&options, &descriptor);
    rows.to_string()
}

fn append_children(
    tree: &mut Tree<RenderNode>,
    parent_id: usize,
    children: &BTreeMap<String, DirNode>,
) {
    let mut items: Vec<(&String, &DirNode)> = children.iter().collect();
    items.sort_by(|(name_a, node_a), (name_b, node_b)| {
        node_b
            .stats
            .lines
            .cmp(&node_a.stats.lines)
            .then_with(|| name_a.cmp(name_b))
    });

    for (name, node) in items {
        let node_id = tree.push(
            parent_id,
            TreeSpan {
                start_time: node.stats.files as u64,
                duration: node.stats.code as u64,
                extra: Some(RenderNode {
                    name: name.clone(),
                    languages: render_language_summary(&node.language_lines),
                    stats: node.stats,
                }),
                ..Default::default()
            },
        );
        append_children(tree, node_id, &node.children);
    }
}

fn render_language_summary(language_lines: &BTreeMap<String, usize>) -> String {
    let mut languages: Vec<(&String, &usize)> = language_lines.iter().collect();
    languages.sort_by(|(name_a, lines_a), (name_b, lines_b)| {
        lines_b.cmp(lines_a).then_with(|| name_a.cmp(name_b))
    });
    let total = languages.len();
    let mut parts: Vec<&str> = languages
        .into_iter()
        .take(6)
        .map(|(name, _)| name.as_str())
        .collect();
    if total > 6 {
        parts.push("...");
    }
    parts.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parse_languages_supports_commas_and_repeatable_flags() {
        let parsed =
            parse_language_filter(&["Rust,Plain Text".to_string(), "JavaScript".to_string()])
                .unwrap()
                .unwrap();

        assert!(parsed.contains(&LanguageType::Rust));
        assert!(parsed.contains(&LanguageType::Text));
        assert!(parsed.contains(&LanguageType::JavaScript));
    }

    #[test]
    fn parse_languages_rejects_unknowns() {
        let err = parse_language_filter(&["Rust,NotALanguage".to_string()]).unwrap_err();
        assert!(err.contains("NotALanguage"));
    }

    #[test]
    fn parse_languages_requires_commas_between_multiple_values() {
        let err = parse_language_filter(&["Rust Python".to_string()]).unwrap_err();
        assert!(err.contains("Rust Python"));
    }

    #[test]
    fn build_exclude_matcher_normalizes_relative_paths() {
        let matcher =
            build_exclude_matcher(&["./target".to_string(), "src/../generated".to_string()])
                .unwrap();

        assert!(matcher.is_match("target"));
        assert!(matcher.is_match("target/debug/build.rs"));
        assert!(matcher.is_match("generated"));
        assert!(matcher.is_match("generated/output.rs"));
    }

    #[test]
    fn build_exclude_matcher_rejects_absolute_paths() {
        let err = build_exclude_matcher(&["/tmp/build".to_string()]).unwrap_err();
        assert!(err.contains("relative to the current directory"));
    }

    #[test]
    fn build_exclude_matcher_rejects_invalid_globs() {
        let err = build_exclude_matcher(&["src/[".to_string()]).unwrap_err();
        assert!(err.contains("Invalid -X/--exclude glob"));
    }

    #[test]
    fn parse_root_code_hide_percentage_defaults_to_ten() {
        assert_eq!(parse_root_code_hide_percentage(None, None).unwrap(), 10);
    }

    #[test]
    fn parse_root_code_hide_percentage_validates_range() {
        let err = parse_root_code_hide_percentage(Some(120), None).unwrap_err();
        assert!(err.contains("0..=100"));
    }

    #[test]
    fn render_ascii_tree_hides_by_root_code_percentage() {
        let mut root = DirNode::default();
        root.stats = LineStats {
            files: 3,
            lines: 130,
            code: 100,
            comments: 20,
            blanks: 10,
        };

        let mut large = DirNode::default();
        large.stats = LineStats {
            files: 2,
            lines: 80,
            code: 60,
            comments: 15,
            blanks: 5,
        };

        let mut small_child = DirNode::default();
        small_child.stats = LineStats {
            files: 1,
            lines: 15,
            code: 10,
            comments: 4,
            blanks: 1,
        };
        large
            .children
            .insert("small_child.rs".to_string(), small_child);

        let mut medium = DirNode::default();
        medium.stats = LineStats {
            files: 1,
            lines: 35,
            code: 30,
            comments: 1,
            blanks: 4,
        };

        root.children.insert("large".to_string(), large);
        root.children.insert("medium.rs".to_string(), medium);

        let output = render_ascii_tree(&root, 16, "root".to_string());

        assert!(output.contains("large"));
        assert!(output.contains("medium.rs"));
        assert!(!output.contains("small_child.rs"));
    }

    #[test]
    fn render_language_summary_sorts_by_lines_desc() {
        let mut input = BTreeMap::new();
        input.insert("Rust".to_string(), 120);
        input.insert("TypeScript".to_string(), 240);
        input.insert("Python".to_string(), 120);
        assert_eq!(render_language_summary(&input), "TypeScript,Python,Rust");
    }

    #[test]
    fn render_language_summary_limits_to_six_with_ellipsis() {
        let mut input = BTreeMap::new();
        input.insert("A".to_string(), 70);
        input.insert("B".to_string(), 60);
        input.insert("C".to_string(), 50);
        input.insert("D".to_string(), 40);
        input.insert("E".to_string(), 30);
        input.insert("F".to_string(), 20);
        input.insert("G".to_string(), 10);
        assert_eq!(render_language_summary(&input), "A,B,C,D,E,F,...");
    }

    #[test]
    fn dot_prefixed_directory_detection() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let base = std::env::temp_dir().join(format!("tloc-dotdir-test-{unique}"));
        fs::create_dir_all(base.join(".git")).unwrap();
        fs::create_dir_all(base.join("src")).unwrap();

        let mut dot = None;
        let mut normal = None;
        for entry in fs::read_dir(&base).unwrap() {
            let entry = entry.unwrap();
            let name = entry.file_name();
            if name == ".git" {
                dot = Some(entry);
            } else if name == "src" {
                normal = Some(entry);
            }
        }

        let dot = dot.expect("missing .git dir entry");
        let normal = normal.expect("missing src dir entry");
        assert!(is_dot_prefixed_dir(&dot));
        assert!(!is_dot_prefixed_dir(&normal));

        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn excluded_path_matches_path_and_descendants() {
        let matcher = build_exclude_matcher(&["target".to_string()]).unwrap();

        assert!(is_excluded_path(Path::new("target"), &matcher));
        assert!(is_excluded_path(
            Path::new("target/debug/build.rs"),
            &matcher
        ));
        assert!(!is_excluded_path(
            Path::new("src/target/debug/build.rs"),
            &matcher
        ));
        assert!(!is_excluded_path(Path::new("targeted/file.rs"), &matcher));
    }

    #[test]
    fn excluded_path_supports_recursive_globs() {
        let matcher = build_exclude_matcher(&["**/node_modules".to_string()]).unwrap();

        assert!(is_excluded_path(Path::new("node_modules"), &matcher));
        assert!(is_excluded_path(
            Path::new("frontend/node_modules"),
            &matcher
        ));
        assert!(is_excluded_path(
            Path::new("frontend/node_modules/package/index.js"),
            &matcher
        ));
        assert!(!is_excluded_path(
            Path::new("frontend/not_node_modules/package/index.js"),
            &matcher
        ));
    }
}
