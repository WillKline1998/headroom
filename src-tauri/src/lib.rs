//! Headroom: a tiny always-visible widget for your Claude plan limits.

pub mod analytics;
mod claude;
mod hermes;
mod model;
mod poller;
pub mod pricing;
mod settings;

use poller::AppState;
use settings::Settings;
use std::sync::Mutex;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

#[tauri::command]
fn get_state(state: tauri::State<AppState>) -> model::UsageState {
    state.usage.lock().unwrap().clone()
}

#[tauri::command]
fn refresh_now(state: tauri::State<AppState>) {
    state.wake.notify_one();
}

#[tauri::command]
async fn get_analytics(state: tauri::State<'_, AppState>) -> Result<analytics::Analytics, String> {
    let include_hermes = state.settings.lock().unwrap().include_hermes;
    tauri::async_runtime::spawn_blocking(move || analytics::scan(include_hermes))
        .await
        .map_err(|e| e.to_string())
}

/// Whether Hermes Agent is installed here (decides if its Settings switch shows).
#[tauri::command]
fn hermes_detected() -> bool {
    !hermes::databases().is_empty()
}

#[tauri::command]
fn get_settings(state: tauri::State<AppState>) -> Settings {
    state.settings.lock().unwrap().clone()
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: tauri::State<AppState>,
    settings: Settings,
) -> Result<Settings, String> {
    let s = settings.sanitized();
    s.save(&state.settings_path).map_err(|e| e.to_string())?;
    if let Some(w) = app.get_webview_window("widget") {
        let _ = w.set_always_on_top(s.always_on_top);
    }
    *state.settings.lock().unwrap() = s.clone();
    state.wake.notify_one(); // re-draw the tray text and restart the timer
    Ok(s)
}

#[tauri::command]
fn open_details(app: AppHandle, tab: Option<String>) -> Result<(), String> {
    show_details(&app, tab.as_deref()).map_err(|e| e.to_string())
}

fn show_widget(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("widget") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

fn show_details(app: &AppHandle, tab: Option<&str>) -> tauri::Result<()> {
    let tab = tab.unwrap_or("limits");
    if let Some(w) = app.get_webview_window("details") {
        w.eval(format!("window.location.hash = 'details/{tab}'"))?;
        w.show()?;
        return w.set_focus();
    }
    WebviewWindowBuilder::new(
        app,
        "details",
        WebviewUrl::App(format!("index.html#details/{tab}").into()),
    )
    .title("Headroom")
    .inner_size(760.0, 600.0)
    .min_inner_size(520.0, 420.0)
    .build()?;
    Ok(())
}

fn build_tray(app: &AppHandle) -> tauri::Result<()> {
    let show = MenuItem::with_id(app, "show", "Show widget", true, None::<&str>)?;
    let details = MenuItem::with_id(app, "details", "Details…", true, None::<&str>)?;
    let refresh = MenuItem::with_id(app, "refresh", "Refresh now", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Headroom", true, None::<&str>)?;
    let sep1 = PredefinedMenuItem::separator(app)?;
    let sep2 = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(
        app,
        &[&show, &details, &refresh, &sep1, &settings, &sep2, &quit],
    )?;

    TrayIconBuilder::with_id("main")
        .icon(tauri::include_image!("icons/tray.png"))
        .icon_as_template(true)
        .menu(&menu)
        .show_menu_on_left_click(cfg!(target_os = "macos"))
        .on_menu_event(|app, e| match e.id.as_ref() {
            "show" => show_widget(app),
            "details" => {
                let _ = show_details(app, None);
            }
            "settings" => {
                let _ = show_details(app, Some("settings"));
            }
            "refresh" => app.state::<AppState>().wake.notify_one(),
            "quit" => app.exit(0),
            _ => {}
        })
        .on_tray_icon_event(|tray, e| {
            // Windows/Linux: left-click brings up the widget; the menu is on right-click.
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = e
            {
                if cfg!(not(target_os = "macos")) {
                    show_widget(tray.app_handle());
                }
            }
        })
        .build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .setup(|app| {
            // A widget, not a "real" app: no Dock icon on macOS.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let settings_path = app.path().app_config_dir()?.join("settings.json");
            let settings = Settings::load(&settings_path);
            if let Some(w) = app.get_webview_window("widget") {
                let _ = w.set_always_on_top(settings.always_on_top);
            }
            app.manage(AppState {
                usage: Mutex::new(model::UsageState {
                    status: "loading".into(),
                    ..Default::default()
                }),
                settings: Mutex::new(settings),
                settings_path,
                wake: tokio::sync::Notify::new(),
                alerted: Mutex::new(Default::default()),
                client: reqwest::Client::builder()
                    .user_agent(concat!("headroom/", env!("CARGO_PKG_VERSION")))
                    .timeout(std::time::Duration::from_secs(20))
                    .build()?,
            });
            build_tray(app.handle())?;
            poller::start(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            // Closing the widget hides it; Headroom keeps running in the menu bar / tray.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "widget" {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            refresh_now,
            get_analytics,
            hermes_detected,
            get_settings,
            save_settings,
            open_details
        ])
        .run(tauri::generate_context!())
        .expect("error while running Headroom");
}
