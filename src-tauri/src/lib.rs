pub mod auth;
pub mod core;
pub mod database;
pub mod engine;
pub mod models;
pub mod process;
pub mod tools;
pub mod updates;
use crate::{core::Core, models::*};
use std::{path::PathBuf, sync::Arc};
use tauri::{Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
async fn bootstrap(app: tauri::AppHandle, core: State<'_, Arc<Core>>) -> Result<Bootstrap> {
    let mut data = core.bootstrap().await?;
    data.version = app.package_info().version.to_string();
    Ok(data)
}
#[tauri::command]
fn save_settings(core: State<'_, Arc<Core>>, settings: Settings) -> Result<Settings> {
    core.save_settings(settings)
}
#[tauri::command]
fn resolve_source(core: State<'_, Arc<Core>>, urls: Vec<String>) -> Result<Source> {
    core.inner().resolve(urls)
}
#[tauri::command]
fn cancel_source(core: State<'_, Arc<Core>>, id: String) {
    core.cancel_source(&id)
}
#[tauri::command]
fn enqueue(core: State<'_, Arc<Core>>, request: EnqueueRequest) -> Result<EnqueueResult> {
    core.enqueue(request)
}
#[tauri::command]
fn control_jobs(core: State<'_, Arc<Core>>, ids: Vec<String>, action: String) -> Result<Vec<Job>> {
    core.control(ids, &action)
}
#[tauri::command]
async fn connect_auth(core: State<'_, Arc<Core>>, request: AuthRequest) -> Result<AuthConnection> {
    core.connect(request).await
}
#[tauri::command]
fn disconnect_auth(core: State<'_, Arc<Core>>) -> Result<AuthConnection> {
    core.disconnect()
}
#[tauri::command]
async fn install_tools(core: State<'_, Arc<Core>>, update: bool) -> Result<Vec<ToolInfo>> {
    core.install(update).await
}
#[tauri::command]
fn open_file(app: tauri::AppHandle, core: State<'_, Arc<Core>>, id: String) -> Result<()> {
    app.opener()
        .open_path(
            core.completed_path(&id)?.to_string_lossy().to_string(),
            None::<&str>,
        )
        .map_err(|e| AppError::new("open_failed", e))
}
#[tauri::command]
fn reveal_file(app: tauri::AppHandle, core: State<'_, Arc<Core>>, id: String) -> Result<()> {
    app.opener()
        .reveal_item_in_dir(core.completed_path(&id)?)
        .map_err(|e| AppError::new("open_failed", e))
}
#[tauri::command]
fn open_url(app: tauri::AppHandle, url: String) -> Result<()> {
    tools::validate_url(&url)?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|e| AppError::new("open_failed", e))
}
#[tauri::command]
fn export_csv(core: State<'_, Arc<Core>>, path: String) -> Result<usize> {
    core.export_csv(&PathBuf::from(path))
}
#[tauri::command]
fn quit(app: tauri::AppHandle, core: State<'_, Arc<Core>>) {
    core.stop_all();
    app.exit(0);
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(updates::Updates::default())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let core = Core::new(app.path().app_data_dir()?)?;
            let handle = app.handle().clone();
            core.subscribe(Arc::new(move |event| {
                let _ = handle.emit("core-event", event);
            }));
            app.manage(core.clone());
            tauri::async_runtime::spawn(async move {
                core.start();
            });
            let language = app.state::<Arc<Core>>().settings()?.language;
            let zh = language != "en";
            let show = tauri::menu::MenuItem::with_id(
                app,
                "show",
                if zh {
                    "显示 YT-Desktop"
                } else {
                    "Show YT-Desktop"
                },
                true,
                None::<&str>,
            )?;
            let quit = tauri::menu::MenuItem::with_id(
                app,
                "quit",
                if zh { "退出" } else { "Quit" },
                true,
                None::<&str>,
            )?;
            let menu = tauri::menu::Menu::with_items(app, &[&show, &quit])?;
            if let Some(icon) = app.default_window_icon() {
                let _ = tauri::tray::TrayIconBuilder::with_id("main-tray")
                    .icon(icon.clone())
                    .tooltip("YT-Desktop")
                    .menu(&menu)
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "show" => {
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.set_focus();
                            }
                        }
                        "quit" => {
                            app.state::<Arc<Core>>().stop_all();
                            app.exit(0);
                        }
                        _ => {}
                    })
                    .build(app);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.app_handle().tray_by_id("main-tray").is_some()
                    && window
                        .state::<Arc<Core>>()
                        .settings()
                        .is_ok_and(|s| s.close_to_tray)
                {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap,
            save_settings,
            resolve_source,
            cancel_source,
            enqueue,
            control_jobs,
            connect_auth,
            disconnect_auth,
            install_tools,
            open_file,
            reveal_file,
            open_url,
            export_csv,
            quit,
            updates::check_update,
            updates::install_update
        ])
        .build(tauri::generate_context!())
        .expect("Failed to initialize YT-Desktop");
    app.run(|app, event| match event {
        tauri::RunEvent::Exit => app.state::<Arc<Core>>().stop_all(),
        #[cfg(target_os = "macos")]
        tauri::RunEvent::Reopen { .. } => {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }
        _ => {}
    });
}
