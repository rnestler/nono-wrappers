#!/usr/bin/env bash
# Symlink nono-claude and nono-opencode into $PREFIX (default ~/.local/bin).
set -euo pipefail

src=$(cd "$(dirname "$0")" && pwd)/nono-agent
dest=${PREFIX:-$HOME/.local/bin}

mkdir -p "$dest"
for name in nono-claude nono-opencode; do
	ln -sfn "$src" "$dest/$name"
	echo "$dest/$name -> $src"
done
