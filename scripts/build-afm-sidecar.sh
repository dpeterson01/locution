#!/usr/bin/env bash
# Build the Apple Foundation Models sidecar (Apple Silicon) and stage it where
# Tauri's `externalBin` expects it, with the target-triple suffix:
#
#   src-tauri/binaries/afm-sidecar-aarch64-apple-darwin
#
# AFM / FoundationModels is Apple-Silicon + macOS 26 only, and Locution ships
# per-arch macOS DMGs (the arm64 build runs on the macOS 26 runner), so only the
# aarch64 slice is ever needed — the Intel DMG never bundles the sidecar.
#
# Run this before `tauri build --config src-tauri/tauri.afm.conf.json` on arm64.
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
sidecar_dir="$repo_root/src-tauri/afm-sidecar"
dest_dir="$repo_root/src-tauri/binaries"
triple="aarch64-apple-darwin"

echo "Building afm-sidecar (arm64, release)…"
( cd "$sidecar_dir" && swift build -c release --arch arm64 )

bin_dir="$(cd "$sidecar_dir" && swift build -c release --arch arm64 --show-bin-path)"
src_bin="$bin_dir/afm-sidecar"
[ -x "$src_bin" ] || { echo "error: build did not produce $src_bin" >&2; exit 1; }

mkdir -p "$dest_dir"
cp "$src_bin" "$dest_dir/afm-sidecar-$triple"
chmod +x "$dest_dir/afm-sidecar-$triple"
echo "Staged: src-tauri/binaries/afm-sidecar-$triple"
