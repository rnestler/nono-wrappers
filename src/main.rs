//! Launch a coding agent inside nono, extending its base profile with
//! language profiles detected from marker files in the project.
//!
//! Invoked as nono-claude / nono-opencode / nono-pi / nono-codex (symlinks), or as
//! `nono-agent <tool> [args...]`. Leading nono flags are passed to nono, e.g.
//! `nono-claude --allow ~/other --extends browser --resume`.

mod detect;

use std::env;
use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio, exit};

use detect::{Project, add};

/// Supported tools: (tool, base profile override variable, default base
/// profile, arguments always passed to the tool, after its subcommand if any).
const TOOLS: &[(&str, &str, &str, &[&str])] = &[
    ("claude", "NONO_CLAUDE_PROFILE", "claude-local", &[]),
    // Without --standalone, opencode v2 may run tools in a shared server
    // outside the sandbox: https://github.com/nolabs-ai/nono-packs/issues/40
    (
        "opencode",
        "NONO_OPENCODE_PROFILE",
        "opencode-local",
        &["--standalone"],
    ),
    ("pi", "NONO_PI_PROFILE", "pi-local", &[]),
    ("codex", "NONO_CODEX_PROFILE", "codex-local", &[]),
];

/// Flags passed to nono when they lead the command line. Each takes one value.
/// Long forms only, since `-r` and `-w` mean something else to claude.
const NONO_FLAGS: &[&str] = &[
    "--allow",
    "--read",
    "--write",
    "--allow-file",
    "--read-file",
    "--write-file",
    "--allow-domain",
    "--open-port",
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
    nono_args: Vec<OsString>,
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
        args.extend_from_slice(&self.nono_args);
        args.push("--".into());
        args.push(tool.into());
        args.extend_from_slice(tool_args);
        args
    }
}

/// The command line split into what goes to nono and what goes to the tool.
#[derive(Debug)]
struct Leading<'a> {
    profiles: Vec<String>,
    nono_args: Vec<OsString>,
    tool_args: &'a [OsString],
}

/// Read leading nono flags, as `--flag value` or `--flag=value`, up to the
/// first argument that isn't one. The rest belongs to the tool.
fn split_leading(args: &[OsString]) -> Result<Leading<'_>, String> {
    let mut profiles = Vec::new();
    let mut nono_args = Vec::new();
    let mut rest = args;
    while let Some((arg, tail)) = rest.split_first() {
        let bytes = arg.as_bytes();
        let (name, inline) = match bytes.iter().position(|b| *b == b'=') {
            Some(i) => (&bytes[..i], Some(OsStr::from_bytes(&bytes[i + 1..]))),
            None => (bytes, None),
        };
        let extends = name == b"--extends";
        if !extends && !NONO_FLAGS.iter().any(|f| f.as_bytes() == name) {
            break;
        }
        let (value, tail) = match inline {
            Some(value) => (value, tail),
            None => {
                let (value, tail) = tail
                    .split_first()
                    .ok_or_else(|| format!("{} needs a value", String::from_utf8_lossy(name)))?;
                (value.as_os_str(), tail)
            }
        };
        if extends {
            profiles.push(value.to_string_lossy().into_owned());
        } else {
            nono_args.push(OsStr::from_bytes(name).to_owned());
            nono_args.push(value.to_owned());
        }
        rest = tail;
    }
    Ok(Leading {
        profiles,
        nono_args,
        tool_args: rest,
    })
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

/// Whether nono can load the profile. If nono can't be started, say yes, so
/// that the launch reports that error instead.
fn profile_available(name: &str) -> bool {
    Command::new("nono")
        .args(["profile", "show", name, "--json"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_or(true, |s| s.success())
}

fn fallback_warning(prog: &str, missing: &str, fallback: &str) -> String {
    format!(
        "{prog}: profile '{missing}' not available, falling back to '{fallback}'.\n\
         This may not work: with a language profile, network access is limited to \
         nono's developer allowlist, which blocks localhost and most hosts."
    )
}

/// The base profile: the override if set, which is never replaced so that a
/// typo stays an error. Otherwise the default, or the tool's pack profile if
/// the default isn't available.
fn base_profile(prog: &str, tool: &str, base_var: &str, base_default: &str) -> String {
    if let Some(base) = var(base_var) {
        return base;
    }
    if profile_available(base_default) {
        return base_default.to_owned();
    }
    let fallback = format!("nolabs-ai/{tool}");
    eprintln!("{}", fallback_warning(prog, base_default, &fallback));
    fallback
}

fn main() {
    let mut args: Vec<OsString> = env::args_os().collect();
    let argv0 = args.remove(0);
    let mut prog = Path::new(&argv0)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "nono-agent".into());

    let tool = match prog.strip_prefix("nono-") {
        Some(t @ ("claude" | "opencode" | "pi" | "codex")) => t.to_owned(),
        _ => {
            if args.is_empty() {
                eprintln!("usage: {prog} <claude|opencode|pi|codex> [args...]");
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
        base: base_profile(&prog, &tool, base_var, base_default),
        allow_cwd: var("NONO_WRAPPER_NO_ALLOW_CWD").is_none(),
        dry_run: var("NONO_WRAPPER_DRY_RUN").is_some(),
        nono_args: words(var("NONO_WRAPPER_ARGS"))
            .into_iter()
            .map(OsString::from)
            .collect(),
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
    let leading = split_leading(&args).unwrap_or_else(|e| {
        eprintln!("{prog}: {e}");
        exit(2);
    });
    for p in &leading.profiles {
        add(&mut launch.extends, p);
    }
    launch.nono_args.extend(leading.nono_args);
    let tool_args = insert_fixed_args(fixed_args, leading.tool_args);

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
    fn leading_flags_in_any_order() {
        let args = os(&[
            "--allow",
            "/a",
            "--extends",
            "browser",
            "--allow-domain=example.com",
            "--extends=selenium",
            "--resume",
        ]);
        let leading = split_leading(&args).unwrap();
        assert_eq!(leading.profiles, ["browser", "selenium"]);
        assert_eq!(
            leading.nono_args,
            os(&["--allow", "/a", "--allow-domain", "example.com"])
        );
        assert_eq!(leading.tool_args, os(&["--resume"]));
    }

    #[test]
    fn every_nono_flag_is_recognised() {
        for flag in NONO_FLAGS {
            let args = os(&[flag, "x", "run"]);
            let leading = split_leading(&args).unwrap();
            assert_eq!(leading.nono_args, os(&[flag, "x"]));
            assert_eq!(leading.tool_args, os(&["run"]));
        }
    }

    #[test]
    fn stops_at_first_unrecognised_argument() {
        let args = os(&["--resume", "--allow", "/a"]);
        let leading = split_leading(&args).unwrap();
        assert!(leading.nono_args.is_empty());
        assert_eq!(leading.tool_args, args);

        let args = os(&["+browser", "--allow", "/a"]);
        let leading = split_leading(&args).unwrap();
        assert!(leading.profiles.is_empty());
        assert_eq!(leading.tool_args, args);

        let args = os(&["--", "--allow", "/a"]);
        assert_eq!(split_leading(&args).unwrap().tool_args, args);

        let args = os(&["--allowedTools=Bash"]);
        assert_eq!(split_leading(&args).unwrap().tool_args, args);
    }

    #[test]
    fn missing_value_is_an_error() {
        for flag in ["--allow", "--extends"] {
            assert_eq!(
                split_leading(&os(&[flag])).unwrap_err(),
                format!("{flag} needs a value")
            );
        }
    }

    #[test]
    fn empty_inline_value_is_left_for_nono() {
        let args = os(&["--allow="]);
        let leading = split_leading(&args).unwrap();
        assert_eq!(leading.nono_args, os(&["--allow", ""]));
    }

    #[test]
    fn values_stay_one_argument() {
        let args = os(&["--allow", "/my dir", "--read=/a=b c"]);
        let leading = split_leading(&args).unwrap();
        assert_eq!(
            leading.nono_args,
            os(&["--allow", "/my dir", "--read", "/a=b c"])
        );
    }

    #[test]
    fn non_utf8_value_survives() {
        let value = OsStr::from_bytes(b"/caf\xe9");
        let mut inline = OsString::from("--allow=");
        inline.push(value);
        let args = [inline];
        let leading = split_leading(&args).unwrap();
        assert_eq!(leading.nono_args[1], value);
    }

    #[test]
    fn fallback_warning_names_both_profiles() {
        let warning = fallback_warning("nono-pi", "pi-local", "nolabs-ai/pi");
        assert!(warning.starts_with(
            "nono-pi: profile 'pi-local' not available, falling back to 'nolabs-ai/pi'.\n"
        ));
        assert!(warning.contains("blocks localhost"));
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
            base: "claude-local".into(),
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
                "claude-local",
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
