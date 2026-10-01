//! Detect language profiles from marker files in the project.

use std::env;
use std::ffi::OsStr;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Detection table: the profile is extended if any of its markers exists in
/// the project root or the current directory.
const DETECT: &[(&str, &[&str])] = &[
    ("rust-dev", &["Cargo.toml"]),
    ("go-dev", &["go.mod"]),
    (
        "python-dev",
        &["pyproject.toml", "setup.py", "requirements.txt", "uv.lock"],
    ),
    ("java-dev", &["pom.xml", "build.gradle", "build.gradle.kts"]),
    ("bun-dev", &["bun.lockb", "bun.lock"]),
    ("mise-dev", &["mise.toml", ".mise.toml", ".tool-versions"]),
];

/// The directories searched for markers.
pub struct Project {
    dirs: Vec<PathBuf>,
}

impl Project {
    /// The git root (if any) and `cwd`, so running from a monorepo
    /// subdirectory picks up markers from both.
    pub fn discover(cwd: &Path) -> Self {
        let mut dirs = Vec::new();
        if let Some(root) = git_root(cwd)
            && root != cwd
        {
            dirs.push(root);
        }
        dirs.push(cwd.to_path_buf());
        Self { dirs }
    }

    fn has(&self, marker: &str) -> bool {
        self.dirs.iter().any(|d| d.join(marker).exists())
    }

    /// Profiles to extend, in table order followed by the special rules.
    /// `mise` says whether mise is available; it wraps most language
    /// toolchains, so `mise-dev` is added whenever a language was detected.
    pub fn detect(&self, mise: bool) -> Vec<String> {
        let mut profiles = Vec::new();
        for (profile, markers) in DETECT {
            if markers.iter().any(|m| self.has(m)) {
                add(&mut profiles, profile);
            }
        }
        // Rules that don't fit the table.
        if self.has("Gemfile") && (self.has("bin/rails") || self.has("config/application.rb")) {
            add(&mut profiles, "rails");
        }
        if self.has("package.json") && !self.has("bun.lockb") && !self.has("bun.lock") {
            add(&mut profiles, "node-dev");
        }
        if mise && !profiles.is_empty() {
            add(&mut profiles, "mise-dev");
        }
        profiles
    }
}

/// Append `profile` unless it is already present.
pub fn add(profiles: &mut Vec<String>, profile: &str) {
    if !profiles.iter().any(|p| p == profile) {
        profiles.push(profile.to_owned());
    }
}

fn git_root(cwd: &Path) -> Option<PathBuf> {
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let root = String::from_utf8(out.stdout).ok()?;
    let root = root.trim_end_matches('\n');
    (!root.is_empty()).then(|| PathBuf::from(root))
}

/// Whether an executable named `name` is on `$PATH`.
pub fn on_path(name: &str) -> bool {
    let Some(path) = env::var_os("PATH") else {
        return false;
    };
    env::split_paths(&path).any(|dir| is_executable(&dir.join(OsStr::new(name))))
}

fn is_executable(path: &Path) -> bool {
    path.metadata()
        .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn project(markers: &[&str]) -> TempDir {
        let dir = TempDir::new().unwrap();
        for m in markers {
            let path = dir.path().join(m);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "").unwrap();
        }
        dir
    }

    fn detect(markers: &[&str], mise: bool) -> Vec<String> {
        let dir = project(markers);
        Project::discover(dir.path()).detect(mise)
    }

    #[test]
    fn nothing_detected() {
        assert!(detect(&[], true).is_empty());
    }

    #[test]
    fn every_marker() {
        for (profile, markers) in DETECT {
            for m in *markers {
                assert_eq!(detect(&[m], false), [*profile], "marker {m}");
            }
        }
    }

    #[test]
    fn table_order_then_special_rules() {
        assert_eq!(
            detect(&["package.json", "Gemfile", "bin/rails", "go.mod"], false),
            ["go-dev", "rails", "node-dev"]
        );
    }

    #[test]
    fn rails_needs_gemfile_and_rails_marker() {
        assert!(detect(&["Gemfile"], false).is_empty());
        assert!(detect(&["bin/rails"], false).is_empty());
        assert_eq!(detect(&["Gemfile", "bin/rails"], false), ["rails"]);
        assert_eq!(
            detect(&["Gemfile", "config/application.rb"], false),
            ["rails"]
        );
    }

    #[test]
    fn node_unless_bun() {
        assert_eq!(detect(&["package.json"], false), ["node-dev"]);
        assert_eq!(detect(&["package.json", "bun.lock"], false), ["bun-dev"]);
        assert_eq!(detect(&["package.json", "bun.lockb"], false), ["bun-dev"]);
    }

    #[test]
    fn mise_only_with_a_language() {
        assert!(detect(&[], true).is_empty());
        assert_eq!(detect(&["Cargo.toml"], true), ["rust-dev", "mise-dev"]);
        assert_eq!(
            detect(&["mise.toml", "go.mod"], true),
            ["go-dev", "mise-dev"]
        );
    }

    #[test]
    fn monorepo_root_and_subdir() {
        let dir = project(&["go.mod", "web/package.json"]);
        let status = Command::new("git")
            .args(["init", "-q"])
            .current_dir(dir.path())
            .status()
            .unwrap();
        assert!(status.success());
        let sub = dir.path().join("web").canonicalize().unwrap();
        let project = Project::discover(&sub);
        assert_eq!(project.dirs.len(), 2);
        assert_eq!(project.detect(false), ["go-dev", "node-dev"]);
    }
}
