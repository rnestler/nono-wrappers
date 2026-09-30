# nono-wrappers
Simple scripts to wrap tools in nono.sh with profile detection

`nono-claude` and `nono-opencode` launch `claude` / `opencode` via `nono run`
with a base profile, plus `--extends` for every language profile detected in the
project. All arguments are passed through to the tool.

```sh
./install.sh            # symlinks into ~/bin (or: ./install.sh DEST)
cd ~/projects/some-rust-crate
nono-claude             # nono run --profile claude-arch --extends rust-dev -- claude
```

## Base profiles

| command         | profile       | override                |
|-----------------|---------------|-------------------------|
| `nono-claude`   | `claude-arch` | `NONO_CLAUDE_PROFILE`   |
| `nono-opencode` | `opencode`    | `NONO_OPENCODE_PROFILE` |

## Detection

Markers are checked in the git root and in `$PWD` (so running from a monorepo
subdirectory picks up both).

| marker                                                     | extends      |
|------------------------------------------------------------|--------------|
| `Cargo.toml`                                               | `rust-dev`   |
| `go.mod`                                                   | `go-dev`     |
| `pyproject.toml`, `setup.py`, `requirements.txt`, `uv.lock`| `python-dev` |
| `pom.xml`, `build.gradle`, `build.gradle.kts`              | `java-dev`   |
| `bun.lockb`, `bun.lock`                                    | `bun-dev`    |
| `package.json` (without a bun lockfile)                    | `node-dev`   |
| `mise.toml`, `.mise.toml`, `.tool-versions`                | `mise-dev`   |
| `Gemfile` + `bin/rails` or `config/application.rb`         | `rails` (user profile) |

Add languages by editing the `DETECT` table at the top of `nono-agent`.

## Environment

| variable                 | effect                                                  |
|--------------------------|---------------------------------------------------------|
| `NONO_WRAPPER_EXTENDS`   | extra profiles to extend, space separated               |
| `NONO_WRAPPER_NO_DETECT` | skip detection                                          |
| `NONO_WRAPPER_DRY_RUN`   | pass `--dry-run` to nono                                |
| `NONO_WRAPPER_ARGS`      | extra `nono run` flags, space separated (e.g. `-v --allow-cwd`) |
| `NONO_WRAPPER_QUIET`     | don't print the selected profiles to stderr             |
