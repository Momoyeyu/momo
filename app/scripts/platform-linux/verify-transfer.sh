#!/usr/bin/env bash
set -euo pipefail
cd /workspace/app
export PATH="/tools/node/bin:/tools/cargo/bin:/tools/bin:$PATH"
dpkg -i src-tauri/target/release/bundle/deb/momo_0.2.0_amd64.deb
export MOMO_E2E_APP=/usr/bin/momo LIBGL_ALWAYS_SOFTWARE=1 WEBKIT_DISABLE_DMABUF_RENDERER=1
export WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1 GDK_BACKEND=x11 NO_AT_BRIDGE=1
export XDG_RUNTIME_DIR=/tmp/momo-runtime MOMO_NATIVE_DOWNLOAD_DIR="/root/下载 测试"
mkdir -p "$XDG_RUNTIME_DIR" /root/.config "$MOMO_NATIVE_DOWNLOAD_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
printf 'XDG_DOWNLOAD_DIR="$HOME/下载 测试"\n' > /root/.config/user-dirs.dirs
dbus-run-session -- xvfb-run -a -s '-screen 0 1280x720x24' \
 node node_modules/@wdio/cli/bin/wdio.js run scripts/wdio.conf.mjs --spec scripts/platform-linux/transfer.e2e.mjs
