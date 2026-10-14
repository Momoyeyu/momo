#!/usr/bin/env bash
# Fetch the pinned Live2D Cubism Web Samples "Hiyori" resource pack.
#
# The sample assets are NOT distributed with this repository (Live2D Free
# Material License). This script downloads the pinned CubismWebSamples
# tarball, extracts Samples/Resources/Hiyori plus the repo-root LICENSE.md,
# and installs them into the given target directories.
#
# Usage:
#   scripts/fetch-hiyori.sh [target-dir ...]
#
# With no arguments, installs into both default locations (repo-relative):
#   site/docs/public/live2d/hiyori
#   app/web/public/live2d/hiyori

set -euo pipefail

PIN="de9eb4bec4326b81fe67a54e84322fdc39bc7446"
TARBALL_URLS=(
  "https://codeload.github.com/Live2D/CubismWebSamples/tar.gz/${PIN}"
  "https://github.com/Live2D/CubismWebSamples/archive/${PIN}.tar.gz"
)
RESOURCE_PATH="Samples/Resources/Hiyori"

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

log() { printf '[fetch-hiyori] %s\n' "$*"; }
die() { printf '[fetch-hiyori] ERROR: %s\n' "$*" >&2; exit 1; }

if [ "$#" -eq 0 ]; then
  set -- "site/docs/public/live2d/hiyori" "app/web/public/live2d/hiyori"
fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
tgz="$work/cubism-web-samples.tar.gz"

downloaded=""
for url in "${TARBALL_URLS[@]}"; do
  log "downloading ${url}"
  if curl --http1.1 -fSL --connect-timeout 20 --retry 3 -o "$tgz" "$url"; then
    downloaded="$url"
    break
  fi
  log "download failed from ${url}"
done
[ -n "$downloaded" ] || die "all download sources failed; check the network or set HTTPS_PROXY (e.g. socks5://127.0.0.1:1080)"

# GitHub tarballs always extract into <repo>-<sha>/; extracting nonexistent
# members fails loudly, which doubles as the layout check.
prefix="CubismWebSamples-${PIN}"

log "extracting ${prefix}/${RESOURCE_PATH}"
tar -xzf "$tgz" -C "$work" "${prefix}/${RESOURCE_PATH}" "${prefix}/LICENSE.md"
src="${work}/${prefix}/${RESOURCE_PATH}"
[ -f "${src}/Hiyori.model3.json" ] || die "archive is missing ${RESOURCE_PATH}/Hiyori.model3.json"
[ -f "${work}/${prefix}/LICENSE.md" ] || die "archive is missing LICENSE.md"

write_notice() {
  cat > "$1/NOTICE.txt" <<'EOF'
桃濑日和 Hiyori sample resources

Official character page: https://www.live2d.com/en/learn/sample/momose-hiyori/
Source: https://raw.githubusercontent.com/Live2D/CubismWebSamples/de9eb4bec4326b81fe67a54e84322fdc39bc7446/Samples/Resources/Hiyori/
Repository commit: de9eb4bec4326b81fe67a54e84322fdc39bc7446
Resource path: Samples/Resources/Hiyori/
Modification: none. The official model, textures, manifest, physics, pose, user data, display data, and motions are retained byte-for-byte. The character design is not modified.

This content uses sample data owned and copyrighted by Live2D Inc. The sample data are utilized in accordance with terms and conditions set by Live2D Inc. This content itself is created at the author’s sole discretion.

Cubism Core is not included in this resource folder. The site loads the official hosted Core separately under the Live2D Proprietary Software License Agreement.
EOF
}

for target in "$@"; do
  case "$target" in
    /*) dest="$target" ;;
    *) dest="${REPO_ROOT}/${target}" ;;
  esac
  log "installing into ${dest}"
  rm -rf "$dest"
  mkdir -p "$dest"
  cp -R "${src}/." "$dest/"
  cp "${work}/${prefix}/LICENSE.md" "$dest/LICENSE.md"
  write_notice "$dest"
  [ -f "$dest/Hiyori.model3.json" ] || die "install incomplete: $dest"
  log "installed $(find "$dest" -type f | wc -l | tr -d ' ') files into ${dest}"
done

log "done (pinned commit ${PIN})"
