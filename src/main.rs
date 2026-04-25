#[allow(dead_code, unused)]
mod ascii_tree;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use ascii_tree::{AsciiOptions, DescribeTreeSpan, Tree, TreeSpan};
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
    children: BTreeMap<String, DirNode>,
}

impl DirNode {
    fn add_file(&mut self, components: &[String], stats: LineStats) {
        self.stats += stats;
        if let Some((head, tail)) = components.split_first() {
            self.children
                .entry(head.clone())
                .or_default()
                .add_file(tail, stats);
        }
    }
}

#[derive(Debug)]
struct Cli {
    roots: Vec<PathBuf>,
    language_filter: Option<BTreeSet<LanguageType>>,
    min_parent_percentage_to_hide: u8,
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
        render_ascii_tree(&aggregate, cli.min_parent_percentage_to_hide)
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

    let min_parent_percentage_to_hide = parse_parent_hide_percentage(
        pargs
            .opt_value_from_str::<_, u8>("-p")
            .map_err(|e| e.to_string())?,
        pargs
            .opt_value_from_str::<_, u8>("--min-parent-percentage-to-hide")
            .map_err(|e| e.to_string())?,
    )?;

    let free = pargs.finish();
    let roots = if free.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        free.into_iter().map(PathBuf::from).collect()
    };

    let language_filter = parse_language_filter(&raw_language_values)?;

    Ok(Cli {
        roots,
        language_filter,
        min_parent_percentage_to_hide,
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
        .flat_map(|v| v.split(|ch: char| ch == ',' || ch.is_whitespace()))
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

fn parse_parent_hide_percentage(short: Option<u8>, long: Option<u8>) -> Result<u8, String> {
    if short.is_some() && long.is_some() {
        return Err("Use only one of -p or --min-parent-percentage-to-hide, not both".to_string());
    }

    let value = long.or(short).unwrap_or(90);
    if value > 100 {
        return Err(format!(
            "min parent percentage to hide must be in 0..=100, got {value}"
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
            &mut root,
        )?;
    }

    Ok(root)
}

fn compute_root_labels(roots: &[PathBuf]) -> Vec<Option<String>> {
    if roots.len() == 1 {
        let single = roots[0].as_path();
        if single == Path::new(".") {
            return vec![None];
        }
    }

    roots
        .iter()
        .map(|root| Some(path_display_label(root)))
        .collect()
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
            walk_dir(base, &path, root_label, config, language_filter, root)?;
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

    let mut components = Vec::new();
    if let Some(label) = root_label {
        components.push(label.clone());
    }

    let relative = file_path.strip_prefix(base).unwrap_or(file_path);
    if let Some(parent) = relative.parent() {
        for component in parent.components() {
            if let Component::Normal(value) = component {
                components.push(value.to_string_lossy().to_string());
            }
        }
    }

    root.add_file(&components, file_stats);
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
        "tloc - directory-based code line counter\n\nUSAGE:\n    tloc [OPTIONS] [PATH ...]\n\nOPTIONS:\n    -L, --languages <LANGS>               Comma- or space-separated languages to include\n    -p, --min-parent-percentage-to-hide   Hide child nodes smaller than this % of parent (default: 90)\n    -h, --help                            Print help\n\nOUTPUT:\n    ASCII directory tree with files/code summary and line breakdown\n"
    );
}

#[derive(Clone, Debug, Default)]
struct RenderNode {
    name: String,
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

    fn source(&self, span: &TreeSpan<RenderNode>) -> String {
        span.extra
            .as_ref()
            .map(|n| {
                format!(
                    "code:{} comment:{} blank:{}",
                    n.stats.code, n.stats.comments, n.stats.blanks
                )
            })
            .unwrap_or_default()
    }
    fn source_title(&self) -> String {
        "Details".to_string()
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

fn render_ascii_tree(aggregate: &DirNode, min_parent_percentage_to_hide: u8) -> String {
    let mut tree: Tree<RenderNode> = Tree::default();
    let root_id = tree.push(
        0,
        TreeSpan {
            start_time: aggregate.stats.files as u64,
            duration: aggregate.stats.lines as u64,
            extra: Some(RenderNode {
                name: ".".to_string(),
                stats: aggregate.stats,
            }),
            ..Default::default()
        },
    );

    append_children(&mut tree, root_id, &aggregate.children);

    let options = AsciiOptions {
        min_duration_parent_percentage_to_hide: min_parent_percentage_to_hide,
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
                duration: node.stats.lines as u64,
                extra: Some(RenderNode {
                    name: name.clone(),
                    stats: node.stats,
                }),
                ..Default::default()
            },
        );
        append_children(tree, node_id, &node.children);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parse_languages_supports_commas_and_spaces() {
        let parsed =
            parse_language_filter(&["Rust,Python".to_string(), "Markdown JavaScript".to_string()])
                .unwrap()
                .unwrap();

        assert!(parsed.contains(&LanguageType::Rust));
        assert!(parsed.contains(&LanguageType::Python));
        assert!(parsed.contains(&LanguageType::Markdown));
        assert!(parsed.contains(&LanguageType::JavaScript));
    }

    #[test]
    fn parse_languages_rejects_unknowns() {
        let err = parse_language_filter(&["Rust,NotALanguage".to_string()]).unwrap_err();
        assert!(err.contains("NotALanguage"));
    }

    #[test]
    fn parse_parent_hide_percentage_defaults_to_ninety() {
        assert_eq!(parse_parent_hide_percentage(None, None).unwrap(), 90);
    }

    #[test]
    fn parse_parent_hide_percentage_validates_range() {
        let err = parse_parent_hide_percentage(Some(120), None).unwrap_err();
        assert!(err.contains("0..=100"));
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
}
