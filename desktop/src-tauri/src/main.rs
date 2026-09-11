#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

const BACKEND_HOST: &str = "127.0.0.1";
const BACKEND_PORT: u16 = 12333;

const CREATE_NO_WINDOW: u32 = 0x08000000;
const BACKEND_STARTUP_TIMEOUT_MS: u64 = 20_000;
const BACKEND_CONNECT_CHECK_INTERVAL_MS: u64 = 250;

#[allow(dead_code)]
struct ManagedBackend {
    child: std::process::Child,
    path: String,
    port: u16,
}

fn managed_backend_slot() -> &'static Mutex<Option<ManagedBackend>> {
    static SLOT: OnceLock<Mutex<Option<ManagedBackend>>> = OnceLock::new();
    SLOT.get_or_init(|| Mutex::new(None))
}

struct BackendStartupState {
    starting: bool,
    last_error: Option<String>,
}

fn backend_startup_state() -> &'static (Mutex<BackendStartupState>, Condvar) {
    static STATE: OnceLock<(Mutex<BackendStartupState>, Condvar)> = OnceLock::new();
    STATE.get_or_init(|| {
        (
            Mutex::new(BackendStartupState {
                starting: false,
                last_error: None,
            }),
            Condvar::new(),
        )
    })
}

fn tcp_port_listening(port: u16) -> bool {
    use std::net::TcpStream;

    TcpStream::connect_timeout(
        &std::net::SocketAddr::from(([127, 0, 0, 1], port)),
        Duration::from_millis(250),
    )
    .is_ok()
}

fn shutdown_managed_backend_inner() -> Result<bool, String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let mut cmd = std::process::Command::new("taskkill");
        cmd.args(["/F", "/IM", "galtransl_backend.exe"]);
        cmd.creation_flags(CREATE_NO_WINDOW);
        let output = cmd.output().map_err(|e| format!("执行 taskkill 失败: {}", e))?;

        if output.status.success() {
            let slot = managed_backend_slot();
            if let Ok(mut guard) = slot.lock() {
                *guard = None;
            }
            return Ok(true);
        } else {
            // 如果没有找到进程，taskkill 会返回错误，但这不算失败
            let stderr = String::from_utf8_lossy(&output.stderr);
            if stderr.contains("未找到") || stderr.contains("not found") {
                let slot = managed_backend_slot();
                if let Ok(mut guard) = slot.lock() {
                    *guard = None;
                }
                return Ok(false);
            }
            return Err(format!("杀掉后端进程失败: {}", stderr));
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let slot = managed_backend_slot();
        let mut guard = slot.lock().map_err(|_| "后端进程状态锁定失败".to_string())?;
        let Some(mut managed) = guard.take() else {
            return Ok(false);
        };

        #[cfg(unix)]
        {
            let process_group = format!("-{}", managed.child.id());
            let _ = std::process::Command::new("kill")
                .args(["-TERM", process_group.as_str()])
                .status();
            std::thread::sleep(Duration::from_millis(150));
            let _ = std::process::Command::new("kill")
                .args(["-KILL", process_group.as_str()])
                .status();
        }

        let _ = managed.child.kill();
        let _ = managed.child.wait();
        Ok(true)
    }
}

fn cleanup_managed_backend_if_exited() {
    let slot = managed_backend_slot();
    let Ok(mut guard) = slot.lock() else { return };
    let Some(managed) = guard.as_mut() else { return };

    if matches!(managed.child.try_wait(), Ok(Some(_))) {
        *guard = None;
    }
}

fn backend_executable_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "galtransl_backend.exe"
    } else {
        "galtransl_backend"
    }
}

fn backend_executable_candidates(
    tauri_resource_dir: Option<&std::path::Path>,
) -> Vec<std::path::PathBuf> {
    let mut candidates = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let backend_name = backend_executable_name();
    let mut backend_names = vec![backend_name];
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    backend_names.push("galtransl_backend-x86_64-unknown-linux-gnu");

    if let Some(configured) = std::env::var_os("GALTRANSL_BACKEND_PATH") {
        let path = std::path::PathBuf::from(configured);
        if seen.insert(path.clone()) {
            candidates.push(path);
        }
    }

    if let Some(resource_dir) = tauri_resource_dir {
        for name in &backend_names {
            for candidate in [
                resource_dir.join(name),
                resource_dir.join("backend").join(name),
                resource_dir.join("binaries").join(name),
            ] {
                if seen.insert(candidate.clone()) {
                    candidates.push(candidate);
                }
            }
        }
    }

    let current_exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(_) => return candidates,
    };

    let exe_dir = match current_exe.parent() {
        Some(path) => path.to_path_buf(),
        None => return candidates,
    };

    for dir in std::iter::once(exe_dir.as_path()).chain(exe_dir.ancestors()) {
        for name in &backend_names {
            for candidate in [
                dir.join("backend").join(name),
                dir.join("binaries").join(name),
                dir.join("dist").join(name),
                dir.join(name),
            ] {
                if seen.insert(candidate.clone()) {
                    candidates.push(candidate);
                }
            }
        }
    }

    candidates
}

fn has_runtime_resources(dir: &std::path::Path) -> bool {
    ["plugins", "Dict", "translation_guidelines", "res"]
        .iter()
        .any(|name| dir.join(name).exists())
}

fn resolve_backend_resource_root(
    backend_path: &std::path::Path,
    tauri_resource_dir: Option<&std::path::Path>,
) -> Option<std::path::PathBuf> {
    if let Some(configured) = std::env::var_os("GALTRANSL_RESOURCE_DIR") {
        let path = std::path::PathBuf::from(configured);
        if has_runtime_resources(&path) {
            return Some(path);
        }
    }

    if let Some(resource_dir) = tauri_resource_dir {
        if has_runtime_resources(resource_dir) {
            return Some(resource_dir.to_path_buf());
        }
    }

    let backend_dir = backend_path.parent()?;
    for dir in std::iter::once(backend_dir).chain(backend_dir.ancestors()) {
        if has_runtime_resources(dir) {
            return Some(dir.to_path_buf());
        }
    }

    None
}

fn wait_for_backend_port(timeout: Duration) -> Result<(), String> {
    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        cleanup_managed_backend_if_exited();
        if tcp_port_listening(BACKEND_PORT) {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(BACKEND_CONNECT_CHECK_INTERVAL_MS));
    }

    cleanup_managed_backend_if_exited();
    Err(format!(
        "等待本地后端启动超时（{} ms）",
        timeout.as_millis()
    ))
}

fn try_spawn_backend_process(
    tauri_resource_dir: Option<&std::path::Path>,
    hide_console: bool,
) -> Result<String, String> {
    cleanup_managed_backend_if_exited();

    let slot = managed_backend_slot();
    let mut guard = slot.lock().map_err(|_| "后端进程状态锁定失败".to_string())?;

    if guard.is_some() {
        return Ok("managed-existing".to_string());
    }

    if tcp_port_listening(BACKEND_PORT) {
        return Ok("external-existing".to_string());
    }

    let Some(path) = backend_executable_candidates(tauri_resource_dir)
        .into_iter()
        .find(|candidate| candidate.exists())
    else {
        return Err(format!(
            "未找到可用的服务端可执行文件 {}",
            backend_executable_name()
        ));
    };

    let resource_root = resolve_backend_resource_root(&path, tauri_resource_dir);
    let mut command = std::process::Command::new(&path);
    command
        .arg("--host")
        .arg(BACKEND_HOST)
        .arg("--port")
        .arg(BACKEND_PORT.to_string());

    if let Some(resource_root) = resource_root {
        command.current_dir(&resource_root);
        command.env("GALTRANSL_RESOURCE_DIR", &resource_root);
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        if hide_console {
            command.creation_flags(CREATE_NO_WINDOW);
        }
    }

    let child = command.spawn().map_err(|e| {
        format!(
            "启动服务端失败: {} ({})",
            path.to_string_lossy(),
            e
        )
    })?;

    *guard = Some(ManagedBackend {
        child,
        path: path.to_string_lossy().to_string(),
        port: BACKEND_PORT,
    });

    Ok(path.to_string_lossy().to_string())
}

fn ensure_backend_ready_inner(
    tauri_resource_dir: Option<&std::path::Path>,
    hide_console: bool,
    timeout_ms: Option<u64>,
) -> Result<String, String> {
    cleanup_managed_backend_if_exited();

    if tcp_port_listening(BACKEND_PORT) {
        return Ok("后端已在线".to_string());
    }

    let (state_lock, state_cvar) = backend_startup_state();
    let mut state = state_lock
        .lock()
        .map_err(|_| "后端启动状态锁定失败".to_string())?;

    loop {
        if !state.starting {
            state.starting = true;
            state.last_error = None;
            break;
        }

        state = state_cvar
            .wait(state)
            .map_err(|_| "等待后端启动状态失败".to_string())?;

        cleanup_managed_backend_if_exited();
        if tcp_port_listening(BACKEND_PORT) {
            return Ok("后端已在线".to_string());
        }

        if let Some(error) = state.last_error.clone() {
            return Err(error);
        }
    }

    drop(state);

    let startup_result = (|| {
        let timeout = Duration::from_millis(timeout_ms.unwrap_or(BACKEND_STARTUP_TIMEOUT_MS));
        let spawn_outcome = try_spawn_backend_process(tauri_resource_dir, hide_console)?;
        wait_for_backend_port(timeout)?;
        Ok(match spawn_outcome.as_str() {
            "managed-existing" => "复用已拉起的服务端进程".to_string(),
            "external-existing" => "检测到外部已运行的服务端".to_string(),
            _ => format!("已启动本地服务端: {}", spawn_outcome),
        })
    })();

    let mut state = state_lock
        .lock()
        .map_err(|_| "后端启动状态锁定失败".to_string())?;
    state.starting = false;
    state.last_error = startup_result.clone().err();
    state_cvar.notify_all();
    drop(state);

    if startup_result.is_err() {
        cleanup_managed_backend_if_exited();
    }

    startup_result
}

#[tauri::command]
fn ensure_backend_ready(
    app: tauri::AppHandle,
    hide_console: Option<bool>,
    timeout_ms: Option<u64>,
) -> Result<String, String> {
    use tauri::Manager;

    let resource_dir = app.path().resource_dir().ok();
    ensure_backend_ready_inner(
        resource_dir.as_deref(),
        hide_console.unwrap_or(true),
        timeout_ms,
    )
}

#[cfg(target_os = "windows")]
/// 使用 Windows Shell API 打开 Explorer：
/// - 当 path 指向目录时：打开该目录（若已有同路径 Explorer 窗口则复用并激活）
/// - 当 path 指向文件时：打开其父目录并滚动/高亮选中该文件（VSCode 风格）
fn windows_shell_open(path: &str) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Com::{
        CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE,
    };
    use windows::Win32::UI::Shell::{ILCreateFromPathW, ILFree, SHOpenFolderAndSelectItems};

    let win_path = path.replace('/', "\\");
    let wide: Vec<u16> = win_path.encode_utf16().chain(std::iter::once(0)).collect();

    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE);

        let pidl = ILCreateFromPathW(PCWSTR(wide.as_ptr()));
        if pidl.is_null() {
            CoUninitialize();
            return Err(format!("无法解析路径: {}", win_path));
        }

        let hr = SHOpenFolderAndSelectItems(pidl, None, 0);

        ILFree(Some(pidl));
        CoUninitialize();

        if let Err(e) = hr {
            return Err(format!("打开 Explorer 失败: {}", e));
        }
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn windows_explorer_select(path: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    let win_path = path.replace('/', "\\");
    std::process::Command::new("explorer")
        .arg("/select,")
        .arg(&win_path)
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| format!("定位文件失败: {}", e))?;
    Ok(())
}

#[tauri::command]
fn open_folder(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;

        // explorer 打开目录时默认会复用已经显示该路径的窗口（除非用户关闭了
        // “在不同窗口中打开文件夹”选项）。不要用 SHOpenFolderAndSelectItems，
        // 否则会在父目录中把该文件夹“选中”而不是进入它。
        let win_path = path.replace('/', "\\");
        std::process::Command::new("explorer")
            .arg(&win_path)
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| format!("打开文件夹失败: {}", e))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("打开文件夹失败: {}", e))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("打开文件夹失败: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
fn reveal_file(path: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        if let Err(shell_error) = windows_shell_open(&path) {
            windows_explorer_select(&path)
                .map_err(|fallback_error| format!("{}；备用方式也失败: {}", shell_error, fallback_error))?;
        }
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("定位文件失败: {}", e))?;
    }
    #[cfg(target_os = "linux")]
    {
        let parent = std::path::Path::new(&path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());
        std::process::Command::new("xdg-open")
            .arg(&parent)
            .spawn()
            .map_err(|e| format!("定位文件失败: {}", e))?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(())
    }
}

#[tauri::command]
fn create_dir(path: String) -> Result<(), String> {
    std::fs::create_dir_all(&path).map_err(|e| format!("创建目录失败: {}", e))
}

#[tauri::command]
fn write_text_file(path: String, content: String) -> Result<(), String> {
    std::fs::write(&path, content).map_err(|e| format!("写入文件失败: {}", e))
}

#[tauri::command]
fn copy_files(sources: Vec<String>, destination_dir: String) -> Result<(), String> {
    std::fs::create_dir_all(&destination_dir).map_err(|e| format!("创建目录失败: {}", e))?;
    for src in &sources {
        let file_name = std::path::Path::new(src)
            .file_name()
            .ok_or_else(|| format!("无效的文件路径: {}", src))?
            .to_string_lossy()
            .to_string();
        let dest = std::path::Path::new(&destination_dir).join(&file_name);
        std::fs::copy(src, &dest).map_err(|e| format!("复制文件失败: {} → {} ({})", src, dest.display(), e))?;
    }
    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            ensure_backend_ready,
            open_folder,
            reveal_file,
            create_dir,
            write_text_file,
            copy_files,
        ])
        .on_window_event(|_window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                let _ = shutdown_managed_backend_inner();
            }
        })
        .setup(|_app| {
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
