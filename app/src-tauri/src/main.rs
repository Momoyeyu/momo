//! momo desktop shell：透明 companion 窗口 + tray + momod 生命周期
//! （docs/01-architecture/desktop.md，本轮交付窗口壳 + 第一档能力）。
//!
//! ## momod 拓扑（attach-or-spawn）
//!
//! 启动时探测 `MOMO_ORCH_LISTEN`（默认 127.0.0.1:7800）端口：
//! - **已在跑 → attach**：不接管生命周期，退出时不 kill；注入空 UI token
//!   （standalone momod 不比对 `hello.auth_token`，字段仍必填）。
//! - **未在跑 → spawn**：按 `MOMO_MOMOD_BIN` → exe 同级目录 → `PATH` 解析二进制，
//!   生成 per-process `MOMO_UI_TOKEN` 传给子进程并注入 webview，退出时 kill
//!   （desktop.md：「只终止 owned child process」）。
//! - **解析不到 → attach-only 告警**：窗口照常打开，UI 停在 Connecting…。

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{
    io,
    net::{SocketAddr, TcpStream, ToSocketAddrs},
    path::PathBuf,
    process::{Child, Command},
    sync::Mutex,
    time::Duration,
};

use tauri::{
    AppHandle, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder,
    menu::{CheckMenuItemBuilder, MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
};
use tauri_plugin_window_state::{StateFlags, WindowExt};

/// shell 启动可覆盖 momod 二进制路径的 env（config.md §2.2）。
const MOMOD_BIN_ENV: &str = "MOMO_MOMOD_BIN";
const MAIN_WINDOW: &str = "main";
const TRAY_ID: &str = "tray";
/// spawn 后等待 momod 就绪的最长时间。
const READY_TIMEOUT: Duration = Duration::from_secs(5);
const PROBE_TIMEOUT: Duration = Duration::from_millis(300);

/// owned momod 子进程句柄（attach 模式为 None）。
struct ShellState {
    momod_child: Mutex<Option<Child>>,
}

/// 解析 shell 关心的 momod 监听地址；`MOMO_ORCH_LISTEN` 与 momod 同口径。
fn orch_addr() -> SocketAddr {
    std::env::var("MOMO_ORCH_LISTEN")
        .ok()
        .and_then(|v| {
            v.parse::<SocketAddr>().ok().or_else(|| {
                // host 形式（非 ip 字面量）走 DNS/hosts 解析，取首个结果。
                v.to_socket_addrs().ok().and_then(|mut it| it.next())
            })
        })
        .unwrap_or_else(|| SocketAddr::from(([127, 0, 0, 1], 7800)))
}

/// momod 是否已在监听：TCP connect 即视为 alive（非严格 /healthz）。
fn momod_alive(addr: SocketAddr) -> bool {
    TcpStream::connect_timeout(&addr, PROBE_TIMEOUT).is_ok()
}

/// momod 二进制解析顺序：`MOMO_MOMOD_BIN` → exe 同级 → PATH。
fn find_momod() -> Option<PathBuf> {
    if let Ok(custom) = std::env::var(MOMOD_BIN_ENV) {
        let p = PathBuf::from(custom);
        if p.is_file() {
            return Some(p);
        }
        eprintln!(
            "momo: {MOMOD_BIN_ENV} is set but not a file: {}",
            p.display()
        );
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in ["momod", "momod.exe"] {
                let p = dir.join(name);
                if p.is_file() {
                    return Some(p);
                }
            }
        }
    }
    let exe_name = if cfg!(windows) { "momod.exe" } else { "momod" };
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|dir| dir.join(exe_name))
            .find(|p| p.is_file())
    })
}

/// per-process UI token：32B CSPRNG hex（protocol §认证 ephemeral token）。
fn gen_ui_token() -> String {
    let mut bytes = [0u8; 32];
    if getrandom::fill(&mut bytes).is_err() {
        // CSPRNG 不可用时退化为时间戳+pid 种子——loopback token 可接受，
        // 但记录告警便于排查（desktop.md 权限档要求 ephemeral）。
        eprintln!("momo: CSPRNG unavailable, falling back to weak ui token");
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = (nanos >> ((i % 16) * 8)) as u8 ^ (std::process::id() >> ((i % 4) * 8)) as u8;
        }
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// spawn owned momod；`MOMO_UI_TOKEN` 与注入 webview 的 token 同源。
fn spawn_momod(bin: &PathBuf, ui_token: &str) -> io::Result<Child> {
    Command::new(bin).env("MOMO_UI_TOKEN", ui_token).spawn()
}

/// spawn 后等端口起来（上限 READY_TIMEOUT），避免 UI 首连必失败。
fn wait_ready(addr: SocketAddr) -> bool {
    for _ in 0..(READY_TIMEOUT.as_millis() / 50) as usize {
        if momod_alive(addr) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// webview 注入脚本：`__MOMO_UI_TOKEN__` / `__MOMO_WS_URL__` / `__MOMO_API_BASE__`
///（web/src/config.ts 对应分支消费）。
fn init_script(addr: SocketAddr, ui_token: &str) -> String {
    format!(
        "window.__MOMO_UI_TOKEN__={token};\
         window.__MOMO_WS_URL__={ws};\
         window.__MOMO_API_BASE__={api};",
        token = serde_json::to_string(ui_token).unwrap_or_default(),
        ws = serde_json::to_string(&format!("ws://{addr}/ws/ui")).unwrap_or_default(),
        api = serde_json::to_string(&format!("http://{addr}")).unwrap_or_default(),
    )
}

fn show_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(MAIN_WINDOW) {
        let _ = win.show();
        let _ = win.unminimize();
        let _ = win.set_focus();
    }
}

fn toggle_main(app: &AppHandle) {
    if let Some(win) = app.get_webview_window(MAIN_WINDOW) {
        match win.is_visible() {
            Ok(true) => {
                let _ = win.hide();
            }
            _ => show_main(app),
        }
    }
}

/// 退出前回收 owned child；attach 模式不动 momod（desktop.md 退出语义）。
fn kill_owned_momod(state: &ShellState) {
    if let Some(mut child) = state.momod_child.lock().unwrap().take() {
        let _ = child.kill();
        let _ = child.wait();
    }
}

fn main() {
    let ctx = tauri::generate_context!();
    let addr = orch_addr();

    // attach-or-spawn：owned 时生成 token 两边下发，attach 时给空串。
    let (ui_token, momod_child) = if momod_alive(addr) {
        (String::new(), None)
    } else {
        let token = gen_ui_token();
        match find_momod() {
            Some(bin) => match spawn_momod(&bin, &token) {
                Ok(child) => {
                    if !wait_ready(addr) {
                        eprintln!("momo: momod spawned but not listening within {READY_TIMEOUT:?}");
                    }
                    (token, Some(child))
                }
                Err(e) => {
                    eprintln!("momo: failed to spawn {}: {e}", bin.display());
                    (String::new(), None)
                }
            },
            None => {
                eprintln!(
                    "momo: momod not found (set {MOMOD_BIN_ENV} or put it on PATH); \
                     opening window anyway"
                );
                (String::new(), None)
            }
        }
    };
    let init = init_script(addr, &ui_token);

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // desktop.md 单实例：已有实例 → 唤起现有可见窗口。
            show_main(app);
        }))
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .manage(ShellState {
            momod_child: Mutex::new(momod_child),
        })
        .setup(move |app| {
            // tray：show / hide / 置顶切换 / quit（desktop.md menu 档）。
            let show = MenuItemBuilder::with_id("show", "Show momo").build(app)?;
            let hide = MenuItemBuilder::with_id("hide", "Hide").build(app)?;
            let pin = CheckMenuItemBuilder::with_id("pin", "Always on Top")
                .checked(true)
                .build(app)?;
            let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
            let menu = MenuBuilder::new(app)
                .items(&[&show, &hide, &pin])
                .separator()
                .item(&quit)
                .build()?;
            let pin_item = pin.clone();
            let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
            TrayIconBuilder::with_id(TRAY_ID)
                .icon(icon)
                .icon_as_template(cfg!(target_os = "macos"))
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| match event.id().as_ref() {
                    "show" => show_main(app),
                    "hide" => {
                        if let Some(w) = app.get_webview_window(MAIN_WINDOW) {
                            let _ = w.hide();
                        }
                    }
                    "pin" => {
                        if let Some(w) = app.get_webview_window(MAIN_WINDOW) {
                            let checked = pin_item.is_checked().unwrap_or(true);
                            let _ = w.set_always_on_top(checked);
                        }
                    }
                    "quit" => {
                        if let Some(state) = app.try_state::<ShellState>() {
                            kill_owned_momod(state.inner());
                        }
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    // 左键点 tray 图标 → 显示/隐藏切换。
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        toggle_main(tray.app_handle());
                    }
                })
                .build(app)?;

            let win = WebviewWindowBuilder::new(app, MAIN_WINDOW, WebviewUrl::default())
                .title("momo")
                .inner_size(1080.0, 720.0)
                .min_inner_size(480.0, 360.0)
                .decorations(false)
                .transparent(true)
                .always_on_top(true)
                .initialization_script(&init)
                .build()?;
            // 窗口位置/尺寸持久化（desktop.md：逻辑坐标 + 显示器恢复由插件处理）。
            let _ = win.restore_state(StateFlags::all());
            // 关闭按钮 = 隐藏回 tray，退出只走 tray Quit。
            let win2 = win.clone();
            win.on_window_event(move |event| {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    let _ = win2.hide();
                    api.prevent_close();
                }
            });
            Ok(())
        })
        .build(ctx)
        .expect("error while running momo desktop shell")
        .run(|app, event| {
            // 兜底：任何退出路径（含 Cmd+Q）都回收 owned child。
            if let RunEvent::Exit = event {
                if let Some(state) = app.try_state::<ShellState>() {
                    kill_owned_momod(state.inner());
                }
            }
        });
}
