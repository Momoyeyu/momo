#!/usr/bin/env bash
# Run inside the isolated Linux builder with the app at /workspace/app and tools at /tools.
set -euo pipefail
export PATH="/tools/node/bin:/tools/cargo/bin:/tools/bin:$PATH"
export CARGO_HOME=/tools/cargo RUSTUP_HOME=/tools/rustup CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-8}"
cd /workspace/app
backend/.venv/bin/python -m pytest -q \
  backend/tests/test_desktop.py::test_trusted_native_view_can_read_auth_denial \
  backend/tests/test_extend.py::test_stdio_params_preserves_existing_executable_path_with_spaces \
  backend/tests/test_desktop.py::test_fresh_profile_backup_has_explicit_default_configuration \
  backend/tests/test_desktop.py::test_legacy_database_only_restore_materializes_default_configuration \
  backend/tests/test_desktop.py::test_archive_configuration_uses_utf8_on_non_utf8_windows_locale \
  backend/tests/test_desktop.py::test_native_view_can_read_audio_rate_and_interrupt_generation
python3 scripts/freeze-backend.py
backend/.venv/bin/python scripts/smoke-frozen.py resources
cargo test --manifest-path src-tauri/Cargo.toml
node_modules/.bin/tauri build --bundles deb
version=$(python3 -c 'import json; print(json.load(open("package.json"))["version"])')
package="src-tauri/target/release/bundle/deb/momo_${version}_amd64.deb"
mkdir -p /workspace/evidence/normal-package
cp "$package" "/workspace/evidence/normal-package/momo_${version}_amd64-final.deb"
node_modules/.bin/tauri build --features native-e2e --config src-tauri/tauri.e2e.conf.json --bundles deb
dpkg -i "$package"
export MOMO_E2E_APP=/usr/bin/momo LIBGL_ALWAYS_SOFTWARE=1 WEBKIT_DISABLE_DMABUF_RENDERER=1
# The disposable builder runs as root; production launches do not receive this setting.
export WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1 GDK_BACKEND=x11 NO_AT_BRIDGE=1
export XDG_RUNTIME_DIR=/tmp/momo-runtime
export MOMO_NATIVE_DOWNLOAD_DIR="/root/下载 测试"
mkdir -p /root/.config "$MOMO_NATIVE_DOWNLOAD_DIR"
printf 'XDG_DOWNLOAD_DIR="$HOME/下载 测试"\n' > /root/.config/user-dirs.dirs
mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"
dbus-run-session -- xvfb-run -a -s '-screen 0 1280x720x24' \
  node node_modules/@wdio/cli/bin/wdio.js run scripts/wdio.conf.mjs

dbus-run-session -- xvfb-run -a -s '-screen 0 1280x720x24' \
  node node_modules/@wdio/cli/bin/wdio.js run scripts/wdio.conf.mjs --spec scripts/platform-linux/transfer.e2e.mjs
