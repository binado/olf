//! `olf init`: create or repair a workspace, cloning if needed.

use crate::cli::InitArgs;
use crate::config::{self, Config, Workspace};
use crate::error::{OlfError, Result, bail};
use crate::git::{self, CredentialStatus, HookStatus};
use crate::index::{self, RegisterStatus};
use crate::overleaf::{self, ProjectId};
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

/// LaTeX build artifacts kept out of the checkout's `git status`.
const LATEX_ARTIFACTS: &[&str] = &[
    "*.aux",
    "*.log",
    "*.out",
    "*.toc",
    "*.lof",
    "*.lot",
    "*.fls",
    "*.fdb_latexmk",
    "*.synctex.gz",
    "*.blg",
    "*.bcf",
    "*.run.xml",
    "*.xdv",
    "*.nav",
    "*.snm",
    "*.vrb",
];

/// Paths olf writes inside an adopted clone, which must never reach Overleaf.
const ADOPTED_EXTRAS: &[&str] = &["/.olf/", "/.claude/"];

/// Lines for olf's block in the checkout's `info/exclude`.
pub fn exclude_lines(adopted: bool) -> Vec<&'static str> {
    let extras = if adopted { ADOPTED_EXTRAS } else { &[] };
    extras.iter().chain(LATEX_ARTIFACTS).copied().collect()
}

pub fn run(args: &InitArgs) -> Result<()> {
    let cwd = std::env::current_dir()?;
    let mut target = std::path::absolute(cwd.join(args.path.as_deref().unwrap_or(Path::new("."))))?;
    // Re-running from inside a workspace (e.g. its checkout) repairs that workspace.
    if let Some(root) = Workspace::find_root(&target) {
        target = root;
    }

    let config_path = Workspace::config_path(&target);
    let existing = if config_path.is_file() {
        Some(Config::load(&config_path)?)
    } else {
        None
    };

    let adopted_id = check_enclosing_repo(&target)?;
    let adopted = adopted_id.is_some();
    let project_dir = existing.as_ref().map_or_else(
        || PathBuf::from(if adopted { "." } else { "paper" }),
        |c| c.project_dir.clone(),
    );
    let checkout = target.join(&project_dir);

    let id = resolve_id(args, existing.as_ref(), adopted_id, &checkout)?;
    let url = overleaf::git_url(&id);
    let token = args.token.as_deref().filter(|t| !t.is_empty());

    // Fail on an index conflict before cloning, so it has no side effects.
    index::check(&id, &target, args.force)?;

    if needs_clone(&checkout)? {
        fs::create_dir_all(&target)?;
        git::clone(&target, &url, &checkout, token)?;
        println!("cloned {url} into {}", checkout.display());
    } else if adopted {
        println!("adopting existing Overleaf clone at {}", checkout.display());
    }

    if let Some(token) = token {
        match git::store_credential(&checkout, &url, token)? {
            CredentialStatus::Stored { helper } => {
                println!("stored token in git credential helper `{helper}`");
            }
            CredentialStatus::NoHelper => eprintln!(
                "warning: no git credential helper configured; git will ask for the token \
                 on pull/push (set one with `git config --global credential.helper <helper>`)"
            ),
            CredentialStatus::NotHttp => {}
        }
    }

    let existing_main = existing.as_ref().and_then(|c| c.build.main.clone());
    let main = match (&args.main, existing_main) {
        (Some(main), _) => {
            if !checkout.join(main).is_file() {
                bail!(
                    "--main {} not found in {}",
                    main.display(),
                    checkout.display()
                );
            }
            main.clone()
        }
        (None, Some(main)) => main,
        (None, None) => detect_main(&checkout)?,
    };

    let mut config = Config::new(id.to_string(), project_dir);
    config.build.main = Some(main.clone());
    if config.write_merged(&config_path)? {
        println!("wrote {}", config_path.display());
    }
    if args.main.is_some() {
        // An explicit flag is a request to change the value, unlike defaults.
        config::set_string(&config_path, &["build", "main"], &main.to_string_lossy())?;
    }

    git::set_config(&checkout, "pull.rebase", "true")?;
    // A global `pull.ff=only` would otherwise make `git pull` refuse to rebase
    // as soon as a co-author edits in the browser.
    git::set_config(&checkout, "pull.ff", "true")?;
    match git::install_pre_push_hook(&checkout)? {
        HookStatus::Installed => println!("installed pre-push hook"),
        HookStatus::Unchanged => {}
        HookStatus::Foreign(path) => eprintln!(
            "warning: {} exists and wasn't written by olf; left it alone, \
             so force pushes and pushes to other branches aren't guarded",
            path.display()
        ),
    }
    git::ensure_exclude_block(&checkout, &exclude_lines(adopted))?;

    if let RegisterStatus::Created | RegisterStatus::Relinked =
        index::register(&id, &target, args.force)?
    {
        println!("registered in {}", index::index_dir()?.display());
    }

    println!(
        "workspace ready: {} (project {id}, main {})",
        target.display(),
        main.display()
    );
    Ok(())
}

/// Refuse targets inside non-Overleaf git repos. Returns the project ID when
/// `target` is itself the root of an Overleaf clone (adopt mode).
fn check_enclosing_repo(target: &Path) -> Result<Option<ProjectId>> {
    let probe = target
        .ancestors()
        .find(|p| p.is_dir())
        .unwrap_or(Path::new("/"));
    if !git::is_inside_work_tree(probe) {
        return Ok(None);
    }
    let toplevel = git::toplevel(probe)?;
    let origin_id = git::origin_url(&toplevel).and_then(|u| overleaf::id_from_remote(&u));
    match origin_id {
        Some(id) if same_path(&toplevel, target) => Ok(Some(id)),
        Some(_) => bail!(
            "{} is inside the Overleaf clone at {}\n\
             hint: to use that clone as the workspace, run `olf init {}`",
            target.display(),
            toplevel.display(),
            toplevel.display()
        ),
        None => {
            let name = toplevel
                .file_name()
                .map_or_else(|| "paper".into(), |n| n.to_string_lossy().into_owned());
            let suggestion = toplevel
                .parent()
                .unwrap_or(&toplevel)
                .join(format!("{name}-paper"));
            Err(OlfError::InsideGitRepo {
                target: target.to_path_buf(),
                toplevel,
                suggestion,
            })
        }
    }
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// The project ID, checked for agreement across every place that names one.
fn resolve_id(
    args: &InitArgs,
    existing: Option<&Config>,
    adopted: Option<ProjectId>,
    checkout: &Path,
) -> Result<ProjectId> {
    let mut sources: Vec<(String, ProjectId)> = Vec::new();
    if let Some(id) = &args.id {
        sources.push(("--id".into(), ProjectId::parse(id)?));
    }
    if let Some(url) = &args.url {
        sources.push(("--url".into(), ProjectId::from_url(url)?));
    }
    if let Some(config) = existing {
        sources.push((
            ".olf/config.toml".into(),
            ProjectId::parse(&config.project_id)?,
        ));
    }
    if let Some(id) = adopted {
        sources.push(("the clone's origin".into(), id));
    } else if checkout.is_dir() && git::is_inside_work_tree(checkout) {
        let origin = git::origin_url(checkout).unwrap_or_default();
        let Some(id) = overleaf::id_from_remote(&origin) else {
            bail!(
                "{} is a git checkout whose origin ({origin}) isn't an Overleaf project",
                checkout.display()
            );
        };
        sources.push((format!("the origin of {}", checkout.display()), id));
    }

    let Some((first_src, id)) = sources.first() else {
        bail!(
            "no Overleaf project given\nhint: pass --url <overleaf-project-url> or --id <project-id>"
        );
    };
    if let Some((src, other)) = sources.iter().find(|(_, other)| other != id) {
        bail!("project mismatch: {first_src} says {id} but {src} says {other}");
    }
    Ok(id.clone())
}

fn needs_clone(checkout: &Path) -> Result<bool> {
    match fs::read_dir(checkout) {
        Ok(mut entries) => {
            if entries.next().is_none() {
                return Ok(true);
            }
            if !checkout.join(".git").exists() {
                bail!(
                    "{} exists but isn't a git checkout\nhint: move it away and re-run `olf init`",
                    checkout.display()
                );
            }
            Ok(false)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(e) => Err(e.into()),
    }
}

static DOCUMENTCLASS: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?m)^[ \t]*\\documentclass\s*(?:\[[^\]]*\])?\s*\{([^}]*)\}").expect("valid regex")
});

/// Find the main file: a root `main.tex`, else the single `.tex` file (in
/// any folder, as Overleaf allows) with a `\documentclass` that isn't
/// `standalone`/`subfiles`, else the single such file named `main.tex`.
pub fn detect_main(checkout: &Path) -> Result<PathBuf> {
    if checkout.join("main.tex").is_file() {
        return Ok("main.tex".into());
    }
    let mut candidates = Vec::new();
    collect_root_files(checkout, Path::new(""), &mut candidates)?;
    candidates.sort_by_key(|p| (p.components().count(), p.clone()));
    let named_main: Vec<PathBuf> = candidates
        .iter()
        .filter(|p| p.file_name().is_some_and(|n| n == "main.tex"))
        .cloned()
        .collect();
    match (candidates.as_slice(), named_main.as_slice()) {
        ([main], _) | (_, [main]) => Ok(main.clone()),
        ([], _) => bail!(
            "no main .tex file found in {} (no file with \\documentclass)\n\
             hint: pass --main <file>",
            checkout.display()
        ),
        (many, _) => bail!(
            "several possible main files: {}\nhint: pass --main <file> (Overleaf: Menu → Main document)",
            many.iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// Recursively collect `.tex` files (relative to `base`) that start a document.
fn collect_root_files(base: &Path, rel: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(base.join(rel))? {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_string_lossy().starts_with('.') {
            continue; // .git, .olf, .claude, ...
        }
        let rel = rel.join(&name);
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_root_files(base, &rel, out)?;
        } else if file_type.is_file() && rel.extension().is_some_and(|e| e == "tex") {
            let text = fs::read_to_string(base.join(&rel)).unwrap_or_default();
            let is_root = DOCUMENTCLASS
                .captures_iter(&text)
                .any(|c| !matches!(c[1].trim(), "standalone" | "subfiles"));
            if is_root {
                out.push(rel);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkout(files: &[(&str, &str)]) -> tempfile::TempDir {
        let tmp = tempfile::tempdir().unwrap();
        for (name, content) in files {
            let path = tmp.path().join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
        tmp
    }

    #[test]
    fn finds_main_in_subfolder() {
        let dir = checkout(&[
            ("notes/main.tex", "\\documentclass{book}"),
            ("notes/chapters/gr/index.tex", "\\section{GR}"),
            (".git/x.tex", "\\documentclass{article}"),
        ]);
        assert_eq!(
            detect_main(dir.path()).unwrap(),
            Path::new("notes/main.tex")
        );
    }

    #[test]
    fn prefers_single_main_tex_among_several_roots() {
        let dir = checkout(&[
            ("paper/main.tex", "\\documentclass{article}"),
            ("response/letter.tex", "\\documentclass{letter}"),
        ]);
        assert_eq!(
            detect_main(dir.path()).unwrap(),
            Path::new("paper/main.tex")
        );
    }

    #[test]
    fn prefers_main_tex() {
        let dir = checkout(&[("main.tex", ""), ("paper.tex", "\\documentclass{article}")]);
        assert_eq!(detect_main(dir.path()).unwrap(), Path::new("main.tex"));
    }

    #[test]
    fn finds_single_root_file() {
        let dir = checkout(&[
            (
                "paper.tex",
                "% comment\n\\documentclass[11pt,\n  a4paper]{revtex4-2}\n",
            ),
            ("fig.tex", "\\documentclass{standalone}"),
            ("chapter.tex", "\\documentclass[paper.tex]{subfiles}"),
            ("notes.tex", "% \\documentclass{article}\n\\section{x}"),
            ("readme.md", "\\documentclass{article}"),
        ]);
        assert_eq!(detect_main(dir.path()).unwrap(), Path::new("paper.tex"));
    }

    #[test]
    fn ambiguous_or_missing_main_fails() {
        let dir = checkout(&[
            ("a.tex", "\\documentclass{article}"),
            ("b.tex", "\\documentclass{book}"),
        ]);
        let err = detect_main(dir.path()).unwrap_err().to_string();
        assert!(
            err.contains("a.tex, b.tex") && err.contains("--main"),
            "{err}"
        );

        let dir = checkout(&[("x.tex", "\\section{x}")]);
        assert!(
            detect_main(dir.path())
                .unwrap_err()
                .to_string()
                .contains("--main")
        );
    }

    #[test]
    fn exclude_lines_cover_olf_paths_only_when_adopted() {
        assert!(exclude_lines(true).contains(&"/.olf/"));
        assert!(!exclude_lines(false).contains(&"/.olf/"));
        assert!(exclude_lines(false).contains(&"*.aux"));
    }
}
