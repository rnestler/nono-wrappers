#!/usr/bin/env bash
# Build nono-agent and install it with the nono-claude, nono-opencode and
# nono-pi symlinks into the given directory (default ~/bin).
# Usage: install.sh [DEST]
set -euo pipefail

repo=$(cd "$(dirname "$0")" && pwd)
dest=${1:-$HOME/bin}

cargo build --release --manifest-path "$repo/Cargo.toml"
mkdir -p "$dest"
install -m755 "$repo/target/release/nono-agent" "$dest/nono-agent"
echo "$dest/nono-agent"
for name in nono-claude nono-opencode nono-pi; do
	ln -sfn nono-agent "$dest/$name"
	echo "$dest/$name -> nono-agent"
done
