# nono-wrappers
Wrap coding agents in nono.sh with profile detection

`nono-claude`, `nono-opencode` and `nono-pi` launch `claude` / `opencode` / `pi`
via `nono run --allow-cwd`
with a base profile, plus `--extends` for every language profile detected in the
project. All arguments are passed through to the tool.

```sh
./install.sh            # builds and installs into ~/bin (or: ./install.sh DEST)
cd ~/projects/some-rust-crate
nono-claude             # nono run --profile claude-local --extends rust-dev --extends mise-dev -- claude
```

These nono flags go to nono when they come first on the command line, as
`--flag value` or `--flag=value`: `--allow`, `--read`, `--write`, `--allow-file`,
`--read-file`, `--write-file`, `--allow-domain`, `--open-port` and `--extends`.
Everything from the first other argument onwards goes to the tool:

```sh
nono-claude --allow ~/other --allow-domain example.com --extends browser --resume
```

For other nono flags, use `NONO_WRAPPER_ARGS`.

`install.sh` needs `cargo`. It installs the `nono-agent` binary and the
`nono-claude` / `nono-opencode` / `nono-pi` symlinks to it; the binary picks
the tool from the name it was called by (or use `nono-agent <tool> [args...]`).

## Base profiles

| command         | profile       | override                |
|-----------------|---------------|-------------------------|
| `nono-claude`   | `claude-local` | `NONO_CLAUDE_PROFILE`   |
| `nono-opencode` | `opencode-local` | `NONO_OPENCODE_PROFILE` |
| `nono-pi`       | `pi-local`    | `NONO_PI_PROFILE`       |

If a default profile isn't available, the wrapper prints a warning and falls back
to the tool's pack profile (`nolabs-ai/claude`, `nolabs-ai/opencode` or
`nolabs-ai/pi`), which must be installed with `nono pull`. This may not work: the
pack profiles don't allow localhost, so once a language profile adds the
`developer` allowlist, opencode's standalone server is likely to fail. A profile
set through an override variable is never replaced.

`nono-opencode` always passes `--standalone` to opencode. Otherwise opencode v2
may run tools in a shared server outside the sandbox
([nono-packs#40](https://github.com/nolabs-ai/nono-packs/issues/40)).

The base profiles should set `network_profile` to `developer` and list what the
tool needs beyond that in `allow_domain`. The language profiles (`python-dev`,
`rust-dev`, …) set `developer` too, so setting it in the base profile means the
tool reaches the same hosts whether or not a language is detected. Without it,
`allow_domain` on its own allows only the domains it lists.

The `developer` allowlist blocks localhost, so opencode needs it allowed for its
standalone server. Without that it fails with "Standalone server exited before
reporting readiness". For example, `opencode-local`:

```json
{
  "extends": "nolabs-ai/opencode",
  "meta": { "name": "opencode-local" },
  "workdir": { "access": "readwrite" },
  "network": {
    "network_profile": "developer",
    "allow_domain": ["localhost", "127.0.0.1", "opencode.ai", "models.opencode.ai"]
  }
}
```

Hosts that aren't allowed stay blocked, including for tools like Claude's
WebFetch.

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
| any of the above, and `mise` is installed                  | `mise-dev`   |

Add languages by editing the `DETECT` table in `src/detect.rs`.

## Environment

| variable                 | effect                                                  |
|--------------------------|---------------------------------------------------------|
| `NONO_WRAPPER_EXTENDS`   | extra profiles to extend, space separated               |
| `NONO_WRAPPER_NO_DETECT` | skip detection                                          |
| `NONO_WRAPPER_NO_MISE`   | don't add `mise-dev` automatically                      |
| `NONO_WRAPPER_NO_ALLOW_CWD` | don't pass `--allow-cwd` (nono will prompt for CWD access) |
| `NONO_WRAPPER_DRY_RUN`   | pass `--dry-run` to nono                                |
| `NONO_WRAPPER_ARGS`      | extra `nono run` flags, space separated (e.g. `-v`) |
| `NONO_WRAPPER_QUIET`     | don't print the nono command line to stderr             |
