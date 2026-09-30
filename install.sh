#!/usr/bin/env bash
# Symlink nono-claude and nono-opencode into the given directory (default ~/bin).
# Usage: install.sh [DEST]
set -euo pipefail

src=$(cd "$(dirname "$0")" && pwd)/nono-agent
dest=${1:-$HOME/bin}

mkdir -p "$dest"
for name in nono-claude nono-opencode; do
	ln -sfn "$src" "$dest/$name"
	echo "$dest/$name -> $src"
done
