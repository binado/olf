//! `.olf/config.toml` and workspace discovery.

use crate::cli::Engine;
use crate::error::{OlfError, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item};

pub const OLF_DIR: &str = ".olf";
pub const CONFIG_FILE: &str = "config.toml";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub project_id: String,
    #[serde(default = "default_project_dir")]
    pub project_dir: PathBuf,
    #[serde(default)]
    pub build: BuildConfig,
    #[serde(default)]
    pub fmt: FmtConfig,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BuildConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main: Option<PathBuf>,
    #[serde(default)]
    pub compiler: Compiler,
    #[serde(default)]
    pub engine: Engine,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct FmtConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub wrap: bool,
}

/// The TeX engine Overleaf runs, mirrored from the project's settings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Compiler {
    #[default]
    Pdflatex,
    Xelatex,
    Lualatex,
}

fn default_project_dir() -> PathBuf {
    PathBuf::from("paper")
}

impl Config {
    pub fn new(project_id: String, project_dir: PathBuf) -> Self {
        Self {
            project_id,
            project_dir,
            build: BuildConfig::default(),
            fmt: FmtConfig::default(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)?;
        toml::from_str(&text)
            .map_err(|e| OlfError::Error(format!("invalid {}: {e}", path.display())))
    }

    /// Write this config to `path`, inserting only keys missing from the
    /// existing file so user edits and comments survive. Returns whether the
    /// file changed.
    pub fn write_merged(&self, path: &Path) -> Result<bool> {
        let existing = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(e) => return Err(e.into()),
        };
        let mut doc: DocumentMut = existing
            .parse()
            .map_err(|e| OlfError::Error(format!("invalid {}: {e}", path.display())))?;
        let ours: DocumentMut = toml::to_string(self)
            .map_err(|e| OlfError::Error(format!("cannot serialize config: {e}")))?
            .parse()
            .map_err(|e| OlfError::Error(format!("cannot serialize config: {e}")))?;
        merge_missing(doc.as_table_mut(), ours.as_table());

        let updated = doc.to_string();
        if updated == existing {
            return Ok(false);
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, updated)?;
        Ok(true)
    }
}

/// Set a single (possibly nested) string key in a config file, keeping the
/// rest of the document untouched.
pub fn set_string(path: &Path, keys: &[&str], value: &str) -> Result<()> {
    let text = fs::read_to_string(path)?;
    let mut doc: DocumentMut = text
        .parse()
        .map_err(|e| OlfError::Error(format!("invalid {}: {e}", path.display())))?;
    let (last, parents) = keys.split_last().expect("at least one key");
    let mut item = doc.as_item_mut();
    for key in parents {
        item = &mut item[*key];
    }
    item[*last] = toml_edit::value(value);
    fs::write(path, doc.to_string())?;
    Ok(())
}

fn merge_missing(dst: &mut toml_edit::Table, src: &toml_edit::Table) {
    for (key, item) in src {
        match (dst.get_mut(key), item) {
            (None, _) => {
                dst.insert(key, item.clone());
            }
            (Some(Item::Table(d)), Item::Table(s)) => merge_missing(d, s),
            // Present with a user value (or a different shape): leave it alone.
            (Some(_), _) => {}
        }
    }
}

/// A directory holding `.olf/config.toml` plus the Overleaf checkout.
#[derive(Debug)]
pub struct Workspace {
    pub root: PathBuf,
    pub config: Config,
}

impl Workspace {
    pub fn config_path(root: &Path) -> PathBuf {
        root.join(OLF_DIR).join(CONFIG_FILE)
    }

    /// The nearest ancestor of `start` (inclusive) holding `.olf/config.toml`.
    pub fn find_root(start: &Path) -> Option<PathBuf> {
        start
            .ancestors()
            .find(|dir| Self::config_path(dir).is_file())
            .map(Path::to_path_buf)
    }

    pub fn discover(start: &Path) -> Result<Self> {
        let start = std::path::absolute(start)?;
        let root = Self::find_root(&start).ok_or_else(|| OlfError::NotAProject(start.clone()))?;
        let config = Config::load(&Self::config_path(&root))?;
        Ok(Self { root, config })
    }

    pub fn checkout_dir(&self) -> PathBuf {
        self.root.join(&self.config.project_dir)
    }

    pub fn build_dir(&self) -> PathBuf {
        self.root.join(OLF_DIR).join("build")
    }

    /// Whether the workspace is an adopted Overleaf clone (`project_dir = "."`).
    pub fn is_adopted(&self) -> bool {
        self.config.project_dir == Path::new(".")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "64f0c0ffee0123456789abcd";

    #[test]
    fn defaults_match_plan() {
        let config: Config = toml::from_str(&format!("project_id = \"{ID}\"")).unwrap();
        assert_eq!(config.project_dir, Path::new("paper"));
        assert_eq!(config.build.engine, Engine::Auto);
        assert_eq!(config.build.compiler, Compiler::Pdflatex);
        assert!(config.build.main.is_none());
        assert!(!config.fmt.enabled);
    }

    #[test]
    fn discovers_from_nested_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        let nested = root.join("paper/sections/intro");
        fs::create_dir_all(&nested).unwrap();
        Config::new(ID.into(), "paper".into())
            .write_merged(&Workspace::config_path(root))
            .unwrap();

        let ws = Workspace::discover(&nested).unwrap();
        assert_eq!(ws.root, root);
        assert_eq!(ws.config.project_id, ID);
        assert_eq!(ws.checkout_dir(), root.join("paper"));
        assert_eq!(ws.build_dir(), root.join(".olf/build"));
        assert!(!ws.is_adopted());
    }

    #[test]
    fn discover_fails_outside_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        let err = Workspace::discover(tmp.path()).unwrap_err();
        assert!(matches!(err, OlfError::NotAProject(_)));
    }

    #[test]
    fn merge_keeps_user_values_and_comments() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        let user = format!(
            "# my paper\nproject_id = \"{ID}\"\n\n[build]\ncompiler = \"xelatex\" # matches Overleaf\n"
        );
        fs::write(&path, &user).unwrap();

        let mut ours = Config::new(ID.into(), "paper".into());
        ours.build.main = Some("main.tex".into());
        assert!(ours.write_merged(&path).unwrap());

        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# my paper\n"));
        assert!(text.contains("compiler = \"xelatex\" # matches Overleaf"));
        assert!(text.contains("main = \"main.tex\""));
        assert!(text.contains("[fmt]"));

        let merged = Config::load(&path).unwrap();
        assert_eq!(merged.build.compiler, Compiler::Xelatex);
        assert_eq!(merged.build.engine, Engine::Auto);

        // Second merge is a no-op.
        assert!(!ours.write_merged(&path).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }

    #[test]
    fn set_string_overrides_nested_key() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("config.toml");
        fs::write(&path, "# keep\n[build]\nmain = \"a.tex\" # old\n").unwrap();
        set_string(&path, &["build", "main"], "b.tex").unwrap();
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# keep\n"));
        assert!(text.contains("main = \"b.tex\""));
    }
}
