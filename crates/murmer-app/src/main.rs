#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod recording;
mod state;
mod transcripts;

use state::AppState;
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Listener, Manager, WebviewUrl, WebviewWindowBuilder,
};
use tracing_subscriber::EnvFilter;

/// Logical size of the recording pill window. Kept a little taller/wider than
/// the pill itself to leave room for the drop-in animation and soft shadow.
const PILL_W: f64 = 260.0;
const PILL_H: f64 = 64.0;

/// Bring the main window forward. On macOS an Accessory-policy app stays
/// backgrounded, so `show()` alone is a visual no-op — we must promote the
/// app to Regular and activate it before focusing the window.
///
/// Tray menu/icon callbacks fire on a background thread on macOS, where AppKit
/// window and activation-policy calls silently do nothing. Marshal the work
/// onto the main thread so it actually takes effect.
fn show_main_window(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        #[cfg(target_os = "macos")]
        let _ = handle.set_activation_policy(tauri::ActivationPolicy::Regular);

        if let Some(window) = handle.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    });
}

/// Build the always-on-top recording pill window (hidden initially). It is
/// borderless, transparent, non-focusable, and skips the taskbar so it behaves
/// like a HUD overlay rather than an app window.
fn build_pill_window(app: &AppHandle) -> tauri::Result<()> {
    let win = WebviewWindowBuilder::new(app, "pill", WebviewUrl::App("pill.html".into()))
        .title("murmr recording")
        .inner_size(PILL_W, PILL_H)
        .decorations(false)
        .transparent(true)
        .always_on_top(true)
        .focused(false)
        .skip_taskbar(true)
        .resizable(false)
        .shadow(false)
        .visible(false)
        .build()?;

    // Float the pill above other windows across all Spaces on macOS.
    #[cfg(target_os = "macos")]
    let _ = win.set_visible_on_all_workspaces(true);

    Ok(())
}

/// Position the pill flush against the bottom of the notch (built-in display),
/// or top-center on displays without a notch. Runs on the main thread.
fn position_pill(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Some(win) = handle.get_webview_window("pill") else {
            return;
        };
        let Ok(Some(monitor)) = win.current_monitor().or_else(|_| win.primary_monitor()) else {
            return;
        };
        let scale = monitor.scale_factor();
        let mon_pos = monitor.position(); // physical pixels
        let mon_size = monitor.size(); // physical pixels

        // Center horizontally on the monitor.
        let pill_w_phys = PILL_W * scale;
        let x = mon_pos.x as f64 + (mon_size.width as f64 - pill_w_phys) / 2.0;
        // Flush with the very top of the monitor so the pill's flat black top
        // edge meets the notch / menu-bar top edge.
        let y = mon_pos.y as f64;

        let _ = win.set_position(tauri::PhysicalPosition::new(x, y));
    });
}

/// Show the pill (recording just started). Emits `pill:record` — a pill-only
/// event distinct from the `recording-started` lifecycle event, so this can
/// never re-trigger the lifecycle listener that called it.
fn show_pill(app: &AppHandle) {
    position_pill(app);
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(win) = handle.get_webview_window("pill") {
            let _ = win.show();
            let _ = win.emit("pill:record", ());
        }
    });
}

/// Switch the (already visible) pill to its processing state.
fn pill_processing(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(win) = handle.get_webview_window("pill") {
            let _ = win.emit("pill:process", ());
        }
    });
}

/// Hide the pill (recording flow finished or cancelled).
fn hide_pill(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(win) = handle.get_webview_window("pill") {
            let _ = win.hide();
        }
    });
}

/// Show a brief error in the pill (e.g. "no speech detected", Bedrock failure),
/// then auto-hide it after a couple of seconds so it doesn't linger.
fn pill_error(app: &AppHandle, message: &str) {
    let msg = message.to_string();
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(win) = handle.get_webview_window("pill") {
            let _ = win.show();
            let _ = win.emit("pill:error", msg);
        }
    });
    // Auto-hide after the message has been readable for a moment.
    let handle2 = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
        hide_pill(&handle2);
    });
}

/// Toggle the pill — used by the tray for manual testing until the audio
/// capture pipeline drives it automatically.
fn toggle_pill(app: &AppHandle) {
    if let Some(win) = app.get_webview_window("pill") {
        if win.is_visible().unwrap_or(false) {
            hide_pill(app);
        } else {
            show_pill(app);
        }
    }
}

fn main() {
    // When launched as a .app, stderr is discarded, so also write logs to a
    // file we can inspect: ~/Library/Logs/murmr/murmr.log.
    let log_dir = dirs::home_dir()
        .map(|d| d.join("Library/Logs/murmr"))
        .unwrap_or_else(|| std::path::PathBuf::from("/tmp"));
    let _ = std::fs::create_dir_all(&log_dir);
    let file_layer = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_dir.join("murmr.log"))
        .ok();

    let builder = tracing_subscriber::fmt().with_env_filter(EnvFilter::new(
        "murmer=info,murmer_app=info,murmer_core=info",
    ));
    if let Some(file) = file_layer {
        builder.with_writer(std::sync::Mutex::new(file)).init();
    } else {
        builder.init();
    }

    let config = murmer_core::config::load(None).unwrap_or_else(|e| {
        tracing::warn!("failed to load config: {}, using defaults", e);
        murmer_core::config::Config {
            hotkeys: Default::default(),
            stt: Default::default(),
            llm: Default::default(),
            paste: Default::default(),
            modes: Vec::new(),
            dictionary: Default::default(),
        }
    });

    // Capture the hotkey combos before the config is moved into shared state.
    let dictate_key = config.hotkeys.dictate.clone();
    let command_key = config.hotkeys.command.clone();

    let app_state = Arc::new(AppState::new(config));

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(app_state)
        .setup(move |app| {
            // Build the recording pill overlay window (hidden until recording).
            build_pill_window(app.handle())?;

            // Start the hotkey listener + recording pipeline.
            recording::start(app.handle().clone(), dictate_key, command_key);

            // Create system tray
            let show = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
            let test_pill =
                MenuItem::with_id(app, "test_pill", "Test Recording Pill", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &test_pill, &quit])?;

            // Load the transparent template glyph for the menu bar. macOS
            // recolors template icons to match the menu bar automatically.
            let tray_icon =
                tauri::image::Image::from_bytes(include_bytes!("../icons/tray-icon@2x.png"))?;

            TrayIconBuilder::new()
                .icon(tray_icon)
                .icon_as_template(true)
                .menu(&menu)
                // menu shows on left-click (default) — always a reliable way in.
                // A double/other click also surfaces the window directly.
                .tooltip("murmer — voice to text")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => show_main_window(app),
                    "test_pill" => toggle_pill(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::DoubleClick { .. } = event {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;

            // Drive the pill from the recording lifecycle. Whatever part of the
            // app emits these (audio pipeline, or the manual test trigger), the
            // pill shows on start and hides when the flow ends. It also forwards
            // the state to the pill window so its UI can switch recording→processing.
            let h_start = app.handle().clone();
            app.listen_any("recording-started", move |_| {
                show_pill(&h_start);
            });
            let h_stop = app.handle().clone();
            app.listen_any("recording-stopped", move |_| {
                // recording ended, processing begins — keep the pill up, switch state
                pill_processing(&h_stop);
            });
            let h_done = app.handle().clone();
            app.listen_any("transcript-added", move |_| {
                hide_pill(&h_done);
            });
            let h_err = app.handle().clone();
            app.listen_any("processing-error", move |event| {
                // Show the error briefly in the pill, then auto-hide.
                let msg = event.payload().trim_matches('"').to_string();
                pill_error(&h_err, &msg);
            });

            // Show the window on first launch so opening murmr from Finder /
            // Launchpad / Spotlight actually presents something. After the user
            // closes it, the app drops to a menu-bar-only agent (see the close
            // handler) and the tray / Reopen event brings it back.
            show_main_window(app.handle());

            Ok(())
        })
        .on_window_event(|window, event| {
            // Hide window on close instead of quitting, and return to
            // menu-bar-only (Accessory) mode so no dock icon lingers.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                #[cfg(target_os = "macos")]
                let _ = window
                    .app_handle()
                    .set_activation_policy(tauri::ActivationPolicy::Accessory);
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::get_transcripts,
            commands::clear_transcripts,
            commands::delete_transcript,
            commands::get_dictionary,
            commands::add_dictionary_entry,
            commands::remove_dictionary_entry,
            commands::get_modes,
            commands::check_system,
        ])
        .build(tauri::generate_context!())
        .expect("error running murmer")
        .run(|app, event| {
            // macOS fires Reopen when the user launches the already-running app
            // again (double-click in Finder/Launchpad, click the Dock icon).
            // Bring the window back so it behaves like a normal app.
            #[cfg(target_os = "macos")]
            if let tauri::RunEvent::Reopen { .. } = event {
                show_main_window(app);
            }
            let _ = (app, event);
        });
}
