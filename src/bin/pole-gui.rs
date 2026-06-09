#![windows_subsystem = "windows"]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::Duration;
use std::{fs::File, process::Stdio};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(feature = "gui")]
use tao::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoopBuilder},
    window::{Icon as WindowIcon, Window, WindowBuilder},
};

#[cfg(feature = "gui")]
use tray_icon::{
    menu::{CheckMenuItem, Menu, MenuEvent, MenuItem},
    Icon as TrayIconImage, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent,
};

#[cfg(all(feature = "gui", windows))]
use winrt_notification::{Duration as ToastDuration, Sound, Toast};

#[cfg(feature = "gui")]
#[derive(Clone, Debug)]
enum UserEvent {
    ShowWindow,
    OpenConsole,
    ToggleAutostart,
    Exit,
}

#[cfg(feature = "gui")]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct WindowState {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

fn main() {
    let start_hidden = std::env::args().any(|arg| arg == "--start-hidden");
    let exe_path = std::env::current_exe().expect("failed to get current exe path");
    let client_exe_path = exe_path
        .parent()
        .expect("failed to get executable directory")
        .join("pole-client.exe");
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let config_path = resolve_config_path(&current_dir, &exe_path);
    let data_dir = config_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or(current_dir);
    let window_state_path = data_dir.join("pole-gui-state.json");

    println!("PoLE GUI starting...");
    println!("data_dir={}", data_dir.display());
    println!("config={}", config_path.display());
    println!("client_exe={}", client_exe_path.display());
    println!("start_hidden={}", start_hidden);

    let mut server_child = spawn_control_api_process(&client_exe_path, &config_path, &data_dir);

    match server_child.as_ref() {
        Some(child) => println!("control-api-serve started, pid={}", child.id()),
        None => eprintln!("failed to start control-api-serve"),
    }

    std::thread::sleep(Duration::from_secs(2));

    #[cfg(feature = "gui")]
    {
        use wry::WebViewBuilder;

        let mut event_loop_builder = EventLoopBuilder::<UserEvent>::with_user_event();
        let event_loop = event_loop_builder.build();
        let proxy = event_loop.create_proxy();
        let autostart_enabled = is_gui_autostart_enabled();

        TrayIconEvent::set_event_handler(Some({
            let proxy = proxy.clone();
            move |event| match event {
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
                | TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                } => {
                    let _ = proxy.send_event(UserEvent::ShowWindow);
                }
                _ => {}
            }
        }));

        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            let next_event = match event.id.as_ref() {
                "show" => Some(UserEvent::ShowWindow),
                "open-console" => Some(UserEvent::OpenConsole),
                "autostart" => Some(UserEvent::ToggleAutostart),
                "exit" => Some(UserEvent::Exit),
                _ => None,
            };

            if let Some(next_event) = next_event {
                let _ = proxy.send_event(next_event);
            }
        }));

        let mut window_builder = WindowBuilder::new();
        window_builder = window_builder
            .with_title("PoLE 控制台")
            .with_inner_size(tao::dpi::LogicalSize::new(1280.0, 800.0))
            .with_min_inner_size(tao::dpi::LogicalSize::new(800.0, 600.0))
            .with_window_icon(Some(build_window_icon()))
            .with_visible(!start_hidden);

        if let Some(saved_state) = load_window_state(&window_state_path) {
            window_builder = window_builder
                .with_inner_size(tao::dpi::PhysicalSize::new(
                    saved_state.width,
                    saved_state.height,
                ))
                .with_position(tao::dpi::PhysicalPosition::new(
                    saved_state.x,
                    saved_state.y,
                ));
        }

        let window = window_builder
            .build(&event_loop)
            .expect("failed to create window");

        let _webview = WebViewBuilder::new()
            .with_url("http://127.0.0.1:8787/")
            .with_navigation_handler(|url| {
                println!("Navigating to: {}", url);
                true
            })
            .build(&window)
            .expect("failed to create webview");

        let tray_menu = Menu::new();
        let show_item = MenuItem::with_id("show", "显示主窗口", true, None);
        let open_console_item = MenuItem::with_id("open-console", "打开控制台", true, None);
        let autostart_item =
            CheckMenuItem::with_id("autostart", "开机启动", true, autostart_enabled, None);
        let exit_item = MenuItem::with_id("exit", "退出", true, None);
        tray_menu
            .append_items(&[&show_item, &open_console_item, &autostart_item, &exit_item])
            .expect("failed to build tray menu");

        let _tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            .with_menu_on_left_click(false)
            .with_tooltip("PoLE 控制台")
            .with_icon(build_tray_icon())
            .build()
            .expect("failed to create tray icon");

        if start_hidden {
            notify_background_mode("PoLE 已在后台运行", "可从系统托盘打开主窗口或控制台");
        }

        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;

            match event {
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    save_window_state(&window, &window_state_path);
                    window.set_visible(false);
                    notify_background_mode("PoLE 已最小化到托盘", "右键托盘图标可显示主窗口或退出");
                }
                Event::WindowEvent {
                    event: WindowEvent::Moved(_) | WindowEvent::Resized(_),
                    ..
                } => {
                    save_window_state(&window, &window_state_path);
                }
                Event::UserEvent(UserEvent::ShowWindow) => {
                    ensure_control_api_running(&client_exe_path, &config_path, &data_dir);
                    window.set_visible(true);
                    window.set_minimized(false);
                    window.set_focus();
                }
                Event::UserEvent(UserEvent::OpenConsole) => {
                    open_console(&client_exe_path, &config_path);
                }
                Event::UserEvent(UserEvent::ToggleAutostart) => {
                    let next_state = !autostart_item.is_checked();
                    if set_gui_autostart_enabled(&exe_path, next_state) {
                        autostart_item.set_checked(next_state);
                    }
                }
                Event::UserEvent(UserEvent::Exit) => {
                    save_window_state(&window, &window_state_path);
                    if let Some(child) = server_child.as_mut() {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                    *control_flow = ControlFlow::Exit;
                }
                Event::LoopDestroyed => {
                    if let Some(child) = server_child.as_mut() {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                }
                _ => {}
            }
        });
    }
}

#[cfg(feature = "gui")]
fn load_window_state(path: &Path) -> Option<WindowState> {
    let content = fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

#[cfg(feature = "gui")]
fn save_window_state(window: &Window, path: &Path) {
    let Ok(position) = window.outer_position() else {
        return;
    };
    let size = window.inner_size();
    let state = WindowState {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
    };

    if let Ok(content) = serde_json::to_string_pretty(&state) {
        let _ = fs::write(path, content);
    }
}

fn spawn_background_process(executable: &Path, args: &[&str]) -> Option<Child> {
    let mut command = Command::new(executable);
    command.args(args);

    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    command.spawn().ok()
}

fn spawn_control_api_process(
    executable: &Path,
    config_path: &Path,
    data_dir: &Path,
) -> Option<Child> {
    let stdout = File::create(data_dir.join("control-api.out.log")).ok()?;
    let stderr = File::create(data_dir.join("control-api.err.log")).ok()?;

    let mut command = Command::new(executable);
    command
        .arg("control-api-serve")
        .arg(config_path)
        .arg("127.0.0.1:8787")
        .current_dir(config_path.parent().unwrap_or_else(|| Path::new(".")))
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));

    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }

    command.spawn().ok()
}

fn control_api_ready() -> bool {
    std::net::TcpStream::connect_timeout(
        &"127.0.0.1:8787".parse().expect("invalid control api addr"),
        Duration::from_millis(500),
    )
    .is_ok()
}

fn ensure_control_api_running(client_exe_path: &Path, config_path: &Path, data_dir: &Path) {
    if control_api_ready() {
        return;
    }

    let _ = spawn_control_api_process(client_exe_path, config_path, data_dir);
    std::thread::sleep(Duration::from_secs(2));
}

fn resolve_config_path(current_dir: &Path, exe_path: &Path) -> PathBuf {
    if let Ok(config_path) = std::env::var("POLE_CONFIG_PATH") {
        let candidate = PathBuf::from(config_path);
        if candidate.exists() {
            return candidate;
        }
    }

    for base in [Some(current_dir), exe_path.parent()] {
        if let Some(base) = base {
            for ancestor in base.ancestors() {
                let candidate = ancestor.join("node.json");
                if candidate.exists() {
                    return candidate;
                }
            }
        }
    }

    current_dir.join("node.json")
}

#[cfg(feature = "gui")]
fn open_console(client_exe_path: &std::path::Path, config_path: &std::path::Path) {
    let data_dir = config_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    ensure_control_api_running(client_exe_path, config_path, &data_dir);

    if spawn_background_process(
        client_exe_path,
        &[
            "control-api-open",
            &config_path.to_string_lossy(),
            "127.0.0.1:8787",
        ],
    )
    .is_none()
    {
        eprintln!("failed to open control console");
    }
}

#[cfg(windows)]
fn is_gui_autostart_enabled() -> bool {
    let output = Command::new("reg")
        .args([
            "query",
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
            "/v",
            "PoLE GUI",
        ])
        .output();

    output.map(|value| value.status.success()).unwrap_or(false)
}

#[cfg(not(windows))]
fn is_gui_autostart_enabled() -> bool {
    false
}

#[cfg(windows)]
fn set_gui_autostart_enabled(exe_path: &Path, enabled: bool) -> bool {
    let status = if enabled {
        let command_value = format!("\"{}\" --start-hidden", exe_path.display());
        Command::new("reg")
            .args([
                "add",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "PoLE GUI",
                "/t",
                "REG_SZ",
                "/d",
                &command_value,
                "/f",
            ])
            .status()
    } else {
        Command::new("reg")
            .args([
                "delete",
                r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run",
                "/v",
                "PoLE GUI",
                "/f",
            ])
            .status()
    };

    status.map(|value| value.success()).unwrap_or(false)
}

#[cfg(not(windows))]
fn set_gui_autostart_enabled(_exe_path: &Path, _enabled: bool) -> bool {
    false
}

#[cfg(all(feature = "gui", windows))]
fn notify_background_mode(title: &str, message: &str) {
    let _ = Toast::new(Toast::POWERSHELL_APP_ID)
        .title(title)
        .text1(message)
        .sound(Some(Sound::SMS))
        .duration(ToastDuration::Short)
        .show();
}

#[cfg(all(feature = "gui", not(windows)))]
fn notify_background_mode(_title: &str, _message: &str) {}

#[cfg(feature = "gui")]
fn build_tray_icon() -> TrayIconImage {
    let (rgba, width, height) = build_icon_rgba(16);
    TrayIconImage::from_rgba(rgba, width, height).expect("failed to create tray icon image")
}

#[cfg(feature = "gui")]
fn build_window_icon() -> WindowIcon {
    let (rgba, width, height) = build_icon_rgba(32);
    WindowIcon::from_rgba(rgba, width, height).expect("failed to create window icon image")
}

#[cfg(feature = "gui")]
fn build_icon_rgba(size: u32) -> (Vec<u8>, u32, u32) {
    let mut rgba = Vec::with_capacity((size * size * 4) as usize);

    for y in 0..size {
        for x in 0..size {
            let border_size = (size / 16).max(1);
            let inner_start = size / 4;
            let inner_end = size - inner_start - 1;
            let is_border = x < border_size
                || y < border_size
                || x >= size - border_size
                || y >= size - border_size;
            let is_center =
                x >= inner_start && x <= inner_end && y >= inner_start && y <= inner_end;
            let pixel = if is_center {
                [0, 173, 181, 255]
            } else if is_border {
                [57, 62, 70, 255]
            } else {
                [34, 40, 49, 255]
            };
            rgba.extend_from_slice(&pixel);
        }
    }

    (rgba, size, size)
}
