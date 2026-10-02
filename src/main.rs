//! Launch a coding agent inside nono, extending its base profile with
//! language profiles detected from marker files in the project.
//!
//! Invoked as nono-claude / nono-opencode / nono-pi (symlinks), or as
//! `nono-agent <tool> [args...]`. Leading `+<profile>` arguments add extra
//! profiles to extend, e.g. `nono-claude +browser --resume`.

mod detect;

use std::env;
use std::ffi::{OsStr, OsString};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, exit};

use detect::{Project, add};

/// Supported tools: (tool, base profile override variable, default base
/// profile, arguments always passed to the tool, after its subcommand if any).
const TOOLS: &[(&str, &str, &str, &[&str])] = &[
    ("claude", "NONO_CLAUDE_PROFILE", "claude-arch", &[]),
    // Without --standalone, opencode v2 may run tools in a shared server
    // outside the sandbox: https://github.com/nolabs-ai/nono-packs/issues/40
    (
        "opencode",
        "NONO_OPENCODE_PROFILE",
        "opencode",
        &["--standalone"],
    ),
    ("pi", "NONO_PI_PROFILE", "pi", &[]),
];

/// A non-empty environment variable, like bash's `-n` test.
fn var(name: &str) -> Option<String> {
    env::var(name).ok().filter(|v| !v.is_empty())
}

/// Everything that goes into the `nono run` command line.
#[derive(Debug, Default)]
struct Launch {
    base: String,
    allow_cwd: bool,
    extends: Vec<String>,
    dry_run: bool,
    nono_args: Vec<String>,
}

impl Launch {
    fn args(&self, tool: &str, tool_args: &[OsString]) -> Vec<OsString> {
        let mut args: Vec<OsString> = vec!["run".into(), "--profile".into(), (&self.base).into()];
        if self.allow_cwd {
            args.push("--allow-cwd".into());
        }
        for p in &self.extends {
            args.push("--extends".into());
            args.push(p.into());
        }
        if self.dry_run {
            args.push("--dry-run".into());
        }
        args.extend(self.nono_args.iter().map(OsString::from));
        args.push("--".into());
        args.push(tool.into());
        args.extend_from_slice(tool_args);
        args
    }
}

/// Split leading `+<profile>` arguments off the tool's arguments.
fn split_profiles(args: &[OsString]) -> (Vec<String>, &[OsString]) {
    let mut profiles = Vec::new();
    for (i, arg) in args.iter().enumerate() {
        match arg.to_str().and_then(|a| a.strip_prefix('+')) {
            Some(p) if !p.is_empty() => profiles.push(p.to_owned()),
            _ => return (profiles, &args[i..]),
        }
    }
    (profiles, &[])
}

/// Insert the tool's fixed arguments after its subcommand (a leading
/// argument not starting with `-`), or at the front if there is none.
fn insert_fixed_args(fixed: &[&str], args: &[OsString]) -> Vec<OsString> {
    let at = usize::from(
        args.first()
            .is_some_and(|a| !a.as_encoded_bytes().starts_with(b"-")),
    );
    let mut out = args[..at].to_vec();
    out.extend(fixed.iter().map(OsString::from));
    out.extend_from_slice(&args[at..]);
    out
}

fn main() {
    let mut args: Vec<OsString> = env::args_os().collect();
    let argv0 = args.remove(0);
    let mut prog = Path::new(&argv0)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "nono-agent".into());

    let tool = match prog.strip_prefix("nono-") {
        Some(t @ ("claude" | "opencode" | "pi")) => t.to_owned(),
        _ => {
            if args.is_empty() {
                eprintln!("usage: {prog} <claude|opencode|pi> [args...]");
                exit(2);
            }
            let tool = args.remove(0).to_string_lossy().into_owned();
            prog = format!("nono-{tool}");
            tool
        }
    };
    let Some(&(_, base_var, base_default, fixed_args)) = TOOLS.iter().find(|(t, ..)| *t == tool)
    else {
        eprintln!("{prog}: unknown tool '{tool}'");
        exit(2);
    };

    let mut launch = Launch {
        base: var(base_var).unwrap_or_else(|| base_default.to_owned()),
        allow_cwd: var("NONO_WRAPPER_NO_ALLOW_CWD").is_none(),
        dry_run: var("NONO_WRAPPER_DRY_RUN").is_some(),
        nono_args: words(var("NONO_WRAPPER_ARGS")),
        ..Default::default()
    };

    if var("NONO_WRAPPER_NO_DETECT").is_none() {
        let cwd = env::current_dir().unwrap_or_else(|e| {
            eprintln!("{prog}: cannot determine current directory: {e}");
            exit(1);
        });
        let mise = var("NONO_WRAPPER_NO_MISE").is_none() && detect::on_path("mise");
        launch.extends = Project::discover(&cwd).detect(mise);
    }
    for p in words(var("NONO_WRAPPER_EXTENDS")) {
        add(&mut launch.extends, &p);
    }
    let (profiles, tool_args) = split_profiles(&args);
    for p in profiles {
        add(&mut launch.extends, &p);
    }
    let tool_args = insert_fixed_args(fixed_args, tool_args);

    let nono_args = launch.args(&tool, &tool_args);
    if var("NONO_WRAPPER_QUIET").is_none() {
        let words: Vec<String> = nono_args.iter().map(|a| shell_quote(a)).collect();
        eprintln!("nono {}", words.join(" "));
    }

    let err = Command::new("nono").args(nono_args).exec();
    eprintln!("{prog}: failed to run nono: {err}");
    exit(127);
}

/// Quote `arg` for display so the printed command can be copied into a shell.
fn shell_quote(arg: &OsStr) -> String {
    let s = arg.to_string_lossy();
    let safe = |c: char| c.is_ascii_alphanumeric() || "-_./=+,:@%".contains(c);
    if !s.is_empty() && s.chars().all(safe) {
        s.into_owned()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

fn words(s: Option<String>) -> Vec<String> {
    s.map(|s| s.split_whitespace().map(str::to_owned).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn leading_profiles() {
        let args = os(&["+browser", "+selenium", "--resume", "+later"]);
        let (profiles, rest) = split_profiles(&args);
        assert_eq!(profiles, ["browser", "selenium"]);
        assert_eq!(rest, os(&["--resume", "+later"]));
    }

    #[test]
    fn bare_plus_goes_to_tool() {
        let args = os(&["+", "x"]);
        let (profiles, rest) = split_profiles(&args);
        assert!(profiles.is_empty());
        assert_eq!(rest, args);
    }

    #[test]
    fn only_profiles() {
        let args = os(&["+mise-dev"]);
        let (profiles, rest) = split_profiles(&args);
        assert_eq!(profiles, ["mise-dev"]);
        assert!(rest.is_empty());
    }

    #[test]
    fn fixed_args_after_subcommand() {
        let fixed = &["--standalone"];
        assert_eq!(insert_fixed_args(fixed, &[]), os(&["--standalone"]));
        assert_eq!(
            insert_fixed_args(fixed, &os(&["-c"])),
            os(&["--standalone", "-c"])
        );
        assert_eq!(
            insert_fixed_args(fixed, &os(&["run", "hi"])),
            os(&["run", "--standalone", "hi"])
        );
        assert_eq!(insert_fixed_args(&[], &os(&["run"])), os(&["run"]));
    }

    #[test]
    fn minimal_args() {
        let launch = Launch {
            base: "pi".into(),
            ..Default::default()
        };
        assert_eq!(
            launch.args("pi", &[]),
            os(&["run", "--profile", "pi", "--", "pi"])
        );
    }

    #[test]
    fn full_args() {
        let launch = Launch {
            base: "claude-arch".into(),
            allow_cwd: true,
            extends: vec!["rust-dev".into(), "mise-dev".into()],
            dry_run: true,
            nono_args: vec!["-v".into()],
        };
        assert_eq!(
            launch.args("claude", &os(&["--version"])),
            os(&[
                "run",
                "--profile",
                "claude-arch",
                "--allow-cwd",
                "--extends",
                "rust-dev",
                "--extends",
                "mise-dev",
                "--dry-run",
                "-v",
                "--",
                "claude",
                "--version",
            ])
        );
    }

    #[test]
    fn shell_quoting() {
        assert_eq!(shell_quote("--extends".as_ref()), "--extends");
        assert_eq!(shell_quote("say hi".as_ref()), "'say hi'");
        assert_eq!(shell_quote("it's".as_ref()), r"'it'\''s'");
        assert_eq!(shell_quote("".as_ref()), "''");
    }

    #[test]
    fn words_splits_on_whitespace() {
        assert_eq!(words(Some(" a  b\tc ".into())), ["a", "b", "c"]);
        assert!(words(None).is_empty());
    }
}
