// 系统托盘模块 (Tauri v2)
use tauri::{AppHandle, Emitter, Manager, image::Image};
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use std::sync::Arc;
use log::info;

/// 持有托盘菜单项引用，供运行时动态修改文字
pub struct TrayMenuItems {
    pub toggle_indicator: Arc<MenuItem<tauri::Wry>>,
}

pub fn create_tray(app: &AppHandle) -> anyhow::Result<()> {
    info!("创建系统托盘");

    let toggle_indicator = MenuItem::with_id(app, "toggle_indicator", "显示悬浮窗", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "系统设置", true, None::<&str>)?;
    let history = MenuItem::with_id(app, "history", "识别历史", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&toggle_indicator, &settings, &history, &separator, &quit])?;

    // 将菜单项引用存入全局状态，供事件回调中修改文字
    app.manage(TrayMenuItems {
        toggle_indicator: Arc::new(toggle_indicator.clone()),
    });

    // 加载托盘图标
    let icon = app.default_window_icon().cloned()
        .or_else(|| Image::from_bytes(include_bytes!("../icons/32x32.png")).ok());

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .menu(&menu)
        .tooltip("说人话 - AI语音输入法")
        .on_menu_event(|app, event| {
            match event.id.as_ref() {
                "toggle_indicator" => {
                    let state: tauri::State<crate::AppState> = app.state();
                    let ind = state.indicator.lock();
                    let visible = ind.is_visible();
                    if visible {
                        ind.hide();
                        // 更新菜单文字
                        let items: tauri::State<TrayMenuItems> = app.state();
                        let _ = items.toggle_indicator.set_text("显示悬浮窗");
                    } else {
                        ind.show();
                        let items: tauri::State<TrayMenuItems> = app.state();
                        let _ = items.toggle_indicator.set_text("隐藏悬浮窗");
                    }
                }
                "settings" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                "history" => {
                    let _ = app.emit("tray:open_history", ());
                }
                "quit" => {
                    info!("用户退出应用");
                    if let Some(tray) = app.tray_by_id("main-tray") {
                        let _ = tray.set_visible(false);
                    }
                    
                    // 清理应用资源
                    {
                        let state: tauri::State<crate::AppState> = app.state();
                        state.cleanup();
                    }
                    
                    app.cleanup_before_exit();
                    std::process::exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    if window.is_visible().unwrap_or(false) {
                        let _ = window.hide();
                    } else {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
            }
        });

    if let Some(icon) = icon {
        builder = builder.icon(icon);
    }

    // build() 返回的 TrayIcon 必须存活，通过 manage 持久化
    let tray = builder.build(app)?;
    app.manage(tray);

    Ok(())
}
