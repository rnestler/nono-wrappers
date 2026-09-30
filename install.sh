#!/usr/bin/env bash
# Symlink nono-claude, nono-opencode and nono-pi into the given directory (default ~/bin).
# Usage: install.sh [DEST]
set -euo pipefail

src=$(cd "$(dirname "$0")" && pwd)/nono-agent
dest=${1:-$HOME/bin}

mkdir -p "$dest"
for name in nono-claude nono-opencode nono-pi; do
	ln -sfn "$src" "$dest/$name"
	echo "$dest/$name -> $src"
done
