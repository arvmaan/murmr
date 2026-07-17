#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod state;

use state::AppState;
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager,
};
use tracing_subscriber::EnvFilter;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::new("murmer=info"))
        .init();

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

    let app_state = Arc::new(AppState::new(config));

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(app_state)
        .setup(|app| {
            // Create system tray
            let show = MenuItem::with_id(app, "show", "Show Window", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;

            TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("murmer — voice to text")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .build(app)?;

            // Hide dock icon on macOS (menu-bar-only app)
            #[cfg(target_os = "macos")]
            {
                app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            }

            Ok(())
        })
        .on_window_event(|window, event| {
            // Hide window on close instead of quitting
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.hide();
                api.prevent_close();
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_config,
            commands::save_config,
            commands::get_transcripts,
            commands::clear_transcripts,
            commands::get_dictionary,
            commands::add_dictionary_entry,
            commands::remove_dictionary_entry,
            commands::get_modes,
            commands::check_system,
        ])
        .run(tauri::generate_context!())
        .expect("error running murmer");
}
