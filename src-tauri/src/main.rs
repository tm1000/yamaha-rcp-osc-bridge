// Prevents additional console window on Windows in release
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::menu::{MenuBuilder, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::Mutex;
use yamaha_rcp_to_osc::{BridgeConfig, LogLevel};

#[derive(Default)]
struct BridgeState {
    handle: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BridgeResponse {
    success: bool,
    message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LogEvent {
    level: LogLevel,
    message: String,
}

#[tauri::command]
async fn start_bridge(
    config: BridgeConfig,
    state: State<'_, BridgeState>,
    app: AppHandle,
) -> Result<BridgeResponse, String> {
    let mut handle_lock = state.handle.lock().await;

    // Check if bridge is already running
    if let Some(handle) = handle_lock.as_ref() {
        if !handle.is_finished() {
            return Ok(BridgeResponse {
                success: false,
                message: "Bridge is already running".to_string(),
            });
        }
    }

    // Spawn the bridge task with logging
    let app_clone = app.clone();
    let handle_arc = state.handle.clone();

    let handle = tokio::spawn(async move {
        let app_for_error = app_clone.clone();
        let app_for_stopped = app_clone.clone();

        // Redirect println! to emit events
        let log_fn = move |level: LogLevel, message: String| {
            let _ = app_clone.emit("bridge-log", LogEvent { level, message });
        };

        let result = yamaha_rcp_to_osc::run_bridge_with_logger(config, Box::new(log_fn)).await;

        if let Err(e) = result {
            let _ = app_for_error.emit(
                "bridge-log",
                LogEvent {
                    level: LogLevel::Error,
                    message: format!("Bridge error: {}", e),
                },
            );
        }

        // Clear the handle from state
        let mut handle_lock = handle_arc.lock().await;
        *handle_lock = None;
        drop(handle_lock);

        // Small delay to ensure log messages are processed
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Notify frontend that bridge has stopped
        let _ = app_for_stopped.emit("bridge-stopped", ());
    });

    *handle_lock = Some(handle);

    Ok(BridgeResponse {
        success: true,
        message: "Bridge started successfully".to_string(),
    })
}

#[tauri::command]
async fn stop_bridge(state: State<'_, BridgeState>) -> Result<BridgeResponse, String> {
    let mut handle_lock = state.handle.lock().await;

    if let Some(handle) = handle_lock.take() {
        handle.abort();

        // Give the task time to clean up and release sockets
        // macOS can be slow to release UDP sockets
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

        Ok(BridgeResponse {
            success: true,
            message: "Bridge stopped successfully".to_string(),
        })
    } else {
        Ok(BridgeResponse {
            success: false,
            message: "Bridge is not running".to_string(),
        })
    }
}

#[tauri::command]
async fn get_bridge_status(state: State<'_, BridgeState>) -> Result<bool, String> {
    let handle_lock = state.handle.lock().await;

    if let Some(handle) = handle_lock.as_ref() {
        Ok(!handle.is_finished())
    } else {
        Ok(false)
    }
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(BridgeState::default())
        .invoke_handler(tauri::generate_handler![
            start_bridge,
            stop_bridge,
            get_bridge_status
        ])
        .setup(|app| {
            let handle = app.handle();

            // Custom Quit item (rather than PredefinedMenuItem::quit) so the
            // click routes through `window.close()` below, which emits a
            // CloseRequested event the frontend can intercept to prompt for
            // unsaved changes. PredefinedMenuItem::quit exits immediately
            // without emitting that event.
            let quit_item = MenuItemBuilder::with_id("quit", "Quit")
                .accelerator("CmdOrCtrl+Q")
                .build(handle)?;

            // App submenu (first submenu on macOS; macOS renames it to the app name)
            let about_submenu = SubmenuBuilder::new(handle, "App")
                .item(&PredefinedMenuItem::about(handle, None::<&str>, None)?)
                .separator()
                .item(&quit_item)
                .build()?;

            // File submenu
            let file_submenu = SubmenuBuilder::new(handle, "File")
                .text("file-open", "Open…")
                .text("file-save", "Save")
                .build()?;

            // Top-level menu containing both submenus
            let menu = MenuBuilder::new(handle)
                .items(&[&about_submenu, &file_submenu])
                .build()?;

            app.set_menu(menu)?;

            // Handle menu item clicks
            app.on_menu_event(|app, event| {
                let id = event.id().as_ref();

                match id {
                    "file-open" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.emit("file-open", ());
                        }
                    }
                    "file-save" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.emit("file-save", ());
                        }
                    }
                    "quit" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.close();
                        }
                    }
                    _ => {}
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
