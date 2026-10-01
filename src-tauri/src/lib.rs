pub mod category;
pub mod commands;
pub mod engine;
pub mod manager;
pub mod notifications;
pub mod server;
pub mod speedtest;
pub mod verify;
pub mod video;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager as _, RunEvent, WindowEvent};

use manager::{Event, Manager};

/// Windows ile başlatılırken eklenen argüman; bu durumda pencere açılmadan tepside başlanır.
const AUTOSTART_ARG: &str = "--autostart";

/// Açılışta ana pencere gösterilmeli mi? Yalnızca Windows ile otomatik başlatılmışsa ve kullanıcı
/// "tepside küçültülmüş başla" demişse gösterilmez; elle açılışta her zaman gösterilir.
fn show_window_on_launch(launched_by_autostart: bool, start_minimized: bool) -> bool {
  !(launched_by_autostart && start_minimized)
}

/// Windows başlangıç kaydı açıksa onu güncel yol ve `--autostart` argümanıyla yeniden yazar.
/// Argümansız eski kayıtlar pencereyi açık başlatırdı; bu çağrı onları sessizce düzeltir.
/// Geliştirme derlemesi kaydı kendi (debug) yoluyla ezmesin diye yalnızca yayın sürümünde çalışır.
fn repair_autostart_entry(app: &AppHandle) {
  if cfg!(debug_assertions) {
    return;
  }
  use tauri_plugin_autostart::ManagerExt;
  let launcher = app.autolaunch();
  if launcher.is_enabled().unwrap_or(false) {
    let _ = launcher.enable();
  }
}

/// Pencereyi monitörün çalışma alanına (görev çubuğu hariç) sığdırıp ortalar.
fn fit_to_work_area(w: &tauri::WebviewWindow) {
  let Ok(Some(monitor)) = w.current_monitor().map(|m| m.or_else(|| w.primary_monitor().ok().flatten())) else {
    return;
  };
  let area = monitor.work_area();
  let Ok(size) = w.outer_size() else { return };
  let width = size.width.min(area.size.width);
  let height = size.height.min(area.size.height);
  if width != size.width || height != size.height {
    let _ = w.set_size(tauri::PhysicalSize::new(width, height));
  }
  let x = area.position.x + (area.size.width as i32 - width as i32) / 2;
  let y = area.position.y + (area.size.height as i32 - height as i32) / 2;
  let _ = w.set_position(tauri::PhysicalPosition::new(x, y));
}

fn show_main_window(app: &AppHandle) {
  if let Some(w) = app.get_webview_window("main") {
    if !w.is_visible().unwrap_or(false) {
      fit_to_work_area(&w);
    }
    let _ = w.unminimize();
    let _ = w.show();
    let _ = w.set_focus();
  }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
  let app = tauri::Builder::default()
    // İkinci bir kopya açılırsa (ör. kısayola tıklanırsa) yenisini açmak yerine mevcut pencereyi öne getir.
    .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| show_main_window(app)))
    .plugin(tauri_plugin_dialog::init())
    .plugin(tauri_plugin_notification::init())
    .plugin(tauri_plugin_updater::Builder::new().build())
    .plugin(tauri_plugin_autostart::init(
      tauri_plugin_autostart::MacosLauncher::LaunchAgent,
      Some(vec![AUTOSTART_ARG]),
    ))
    .on_window_event(|window, event| {
      // Pencereyi kapatmak uygulamayı kapatmaz; tepsiye indirir, indirmeler sürer.
      if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let _ = window.hide();
      }
    })
    .setup(|app| {
      let open = MenuItem::with_id(app, "open", "İndirme Yöneticisi'ni aç", true, None::<&str>)?;
      let quit = MenuItem::with_id(app, "quit", "Çık", true, None::<&str>)?;
      let menu = Menu::with_items(app, &[&open, &quit])?;
      let mut tray = TrayIconBuilder::new()
        .tooltip("İndirme Yöneticisi")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id.as_ref() {
          "open" => show_main_window(app),
          "quit" => app.exit(0),
          _ => {}
        })
        .on_tray_icon_event(|tray, event| {
          if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
            show_main_window(tray.app_handle());
          }
        });
      if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
      }
      tray.build(app)?;

      if cfg!(debug_assertions) {
        app.handle().plugin(
          tauri_plugin_log::Builder::default()
            .level(log::LevelFilter::Info)
            .build(),
        )?;
      }
      let data_dir = app.path().app_data_dir()?;
      std::fs::create_dir_all(&data_dir)?;
      let downloads = app.path().download_dir()?;
      let handle = app.handle().clone();
      let emit = Arc::new(move |ev: Event| {
        let _ = match ev {
          Event::Update(d) => handle.emit("download-update", d),
          Event::Completed(d) => {
            use tauri_plugin_notification::NotificationExt;
            let _ = handle
              .notification()
              .builder()
              .title("İndirme tamamlandı")
              .body(d.filename.clone())
              .show();
            handle.emit("download-update", d)
          }
          Event::Remove(id) => handle.emit("download-remove", id),
          Event::Notify(n) => {
            // Tehdit bulunduğunda pencere gizliyse (tepside) kaçırılmasın diye Windows bildirimi de gönder.
            let hidden = handle.get_webview_window("main").is_some_and(|w| !w.is_visible().unwrap_or(false));
            if n.kind == "scan_threat" && hidden {
              use tauri_plugin_notification::NotificationExt;
              let _ = handle.notification().builder().title(n.title.clone()).body(n.body.clone()).show();
            }
            handle.emit("notification", n)
          }
        };
      });
      let manager = Manager::new(data_dir.join("downloads.db"), downloads, emit)?;
      tauri::async_runtime::spawn(server::serve(manager.clone()));
      manager.resume_interrupted();
      manager.start_monitors();

      // Ayar okunduktan sonra karar verilir; pencere yapılandırmada gizli başlar.
      let by_autostart = std::env::args().any(|a| a == AUTOSTART_ARG);
      if show_window_on_launch(by_autostart, manager.settings().start_minimized) {
        show_main_window(app.handle());
      }
      repair_autostart_entry(app.handle());
      app.manage(manager);
      app.manage(commands::SpeedTestState::default());
      Ok(())
    })
    .invoke_handler(tauri::generate_handler![
      commands::list_downloads,
      commands::add_download,
      commands::get_settings,
      commands::default_download_dir,
      commands::pick_folder,
      commands::prepare_for_update,
      commands::speed_test,
      commands::cancel_speed_test,
      commands::set_settings,
      commands::pause_download,
      commands::resume_download,
      commands::remove_download,
      commands::open_download,
      commands::reveal_download,
      commands::hash_download,
      commands::video_prepare,
      commands::add_video,
      commands::scan_download,
      commands::list_notifications,
      commands::mark_notification_read,
      commands::mark_all_notifications_read,
      commands::delete_notification,
      commands::clear_notifications,
    ])
    .build(tauri::generate_context!())
    .expect("error while building tauri application");

  // Kapanmadan önce indirmeleri duraklatıp durumlarını diske yaz.
  let closing = Arc::new(AtomicBool::new(false));
  app.run(move |handle, event| {
    if let RunEvent::ExitRequested { api, .. } = event {
      if closing.swap(true, Ordering::SeqCst) {
        return;
      }
      api.prevent_exit();
      let handle = handle.clone();
      let m = handle.state::<Manager>().inner().clone();
      tauri::async_runtime::spawn(async move {
        m.shutdown().await;
        handle.exit(0);
      });
    }
  });
}

#[cfg(test)]
mod tests {
  use super::show_window_on_launch;
  use crate::manager::Settings;

  #[test]
  fn autostart_with_start_minimized_stays_in_tray() {
    assert!(!show_window_on_launch(true, true));
  }

  #[test]
  fn autostart_without_start_minimized_shows_window() {
    assert!(show_window_on_launch(true, false));
  }

  #[test]
  fn manual_launch_always_shows_window() {
    assert!(show_window_on_launch(false, true));
    assert!(show_window_on_launch(false, false));
  }

  #[test]
  fn start_minimized_is_on_by_default_and_for_old_saved_settings() {
    assert!(Settings::default().start_minimized);
    // Bu alan eklenmeden önce kaydedilmiş ayarlar da tepside başlamalı.
    let old: Settings = serde_json::from_str(r#"{"connections":4}"#).unwrap();
    assert!(old.start_minimized);
  }
}
