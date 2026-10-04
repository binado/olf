//! `olf skill`: install the agent skills bundled in the binary.

use crate::cli::SkillCommand;
use crate::commands::init;
use crate::config::{self, Workspace};
use crate::error::Result;
use crate::git;
use std::fs;
use std::path::{Path, PathBuf};

pub struct Skill {
    pub name: &'static str,
    pub content: &'static str,
}

pub const SKILLS: &[Skill] = &[
    Skill {
        name: "olf-sync",
        content: include_str!("../../skills/olf-sync/SKILL.md"),
    },
    Skill {
        name: "olf-editing",
        content: include_str!("../../skills/olf-editing/SKILL.md"),
    },
    Skill {
        name: "olf-build",
        content: include_str!("../../skills/olf-build/SKILL.md"),
    },
    Skill {
        name: "olf-access",
        content: include_str!("../../skills/olf-access/SKILL.md"),
    },
];

impl Skill {
    /// The `description:` from the SKILL.md frontmatter.
    pub fn description(&self) -> &'static str {
        self.content
            .strip_prefix("---\n")
            .and_then(|rest| rest.split("\n---").next())
            .and_then(|front| front.lines().find_map(|l| l.strip_prefix("description:")))
            .map_or("", str::trim)
    }
}

pub fn run(command: &SkillCommand) -> Result<()> {
    match command {
        SkillCommand::List => {
            for skill in SKILLS {
                println!("{}\t{}", skill.name, skill.description());
            }
            Ok(())
        }
        SkillCommand::Install { project, force } => {
            let dir = if *project {
                project_skills_dir()?
            } else {
                user_skills_dir()?
            };
            install(&dir, *force)
        }
    }
}

/// `$CLAUDE_CONFIG_DIR/skills`, else `~/.claude/skills`.
fn user_skills_dir() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR").filter(|d| !d.is_empty()) {
        return Ok(PathBuf::from(dir).join("skills"));
    }
    Ok(config::home_dir()?.join(".claude").join("skills"))
}

fn project_skills_dir() -> Result<PathBuf> {
    let ws = Workspace::discover(&std::env::current_dir()?)?;
    if ws.config.project_dir == Path::new(".") {
        // The workspace is the Overleaf clone: keep .claude/ out of the project.
        git::ensure_exclude_block(&ws.root, &init::exclude_lines(true))?;
    }
    Ok(ws.root.join(".claude").join("skills"))
}

fn install(dir: &Path, force: bool) -> Result<()> {
    let mut skipped = 0;
    for skill in SKILLS {
        let path = dir.join(skill.name).join("SKILL.md");
        match fs::read_to_string(&path) {
            Ok(current) if current == skill.content => {
                println!("up to date: {}", path.display());
                continue;
            }
            Ok(_) if !force => {
                eprintln!(
                    "skipped: {} exists and differs (--force to overwrite)",
                    path.display()
                );
                skipped += 1;
                continue;
            }
            _ => {}
        }
        fs::create_dir_all(path.parent().expect("skill path has a parent"))?;
        fs::write(&path, skill.content)?;
        println!("installed: {}", path.display());
    }
    if skipped > 0 {
        eprintln!("warning: {skipped} skill(s) not updated");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_skills_have_matching_frontmatter() {
        for skill in SKILLS {
            assert!(
                skill
                    .content
                    .starts_with(&format!("---\nname: {}\n", skill.name)),
                "{} frontmatter",
                skill.name
            );
            assert!(
                skill.description().contains(".olf/config.toml"),
                "{} description should trigger in olf workspaces",
                skill.name
            );
        }
    }
}
