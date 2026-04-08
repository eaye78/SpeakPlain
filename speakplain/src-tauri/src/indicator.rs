// 指示器窗口模块
use std::sync::{Mutex, Arc};
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::{AppHandle, Emitter, WebviewWindowBuilder, Manager};
use log::info;

/// 给窗口设置 WS_EX_NOACTIVATE，点击时不抢焦点
#[cfg(target_os = "windows")]
fn set_no_activate(window: &tauri::WebviewWindow) {
    // 直接用原始 FFI 调用 Win32，避免 windows crate 版本冲突
    extern "system" {
        fn GetWindowLongPtrW(hwnd: isize, n_index: i32) -> isize;
        fn SetWindowLongPtrW(hwnd: isize, n_index: i32, dw_new_long: isize) -> isize;
    }
    const GWL_EXSTYLE: i32 = -20;
    const WS_EX_NOACTIVATE: isize = 0x08000000;
    if let Ok(hwnd) = window.hwnd() {
        unsafe {
            let ex_style = GetWindowLongPtrW(hwnd.0 as isize, GWL_EXSTYLE);
            SetWindowLongPtrW(hwnd.0 as isize, GWL_EXSTYLE, ex_style | WS_EX_NOACTIVATE);
        }
    }
}

/// 将窗口定位到主屏幕底部中央（离底部 80px 间距）
#[cfg(target_os = "windows")]
fn position_bottom_center(window: &tauri::WebviewWindow) {
    extern "system" {
        fn GetSystemMetrics(n_index: i32) -> i32;
    }
    const SM_CXSCREEN: i32 = 0;
    const SM_CYSCREEN: i32 = 1;
    let (screen_w, screen_h) = unsafe {
        (GetSystemMetrics(SM_CXSCREEN), GetSystemMetrics(SM_CYSCREEN))
    };
    if let Ok(size) = window.outer_size() {
        let x = (screen_w - size.width as i32) / 2;
        let y = screen_h - size.height as i32 - 80;
        let _ = window.set_position(tauri::Position::Physical(
            tauri::PhysicalPosition { x, y },
        ));
    }
}

pub struct IndicatorWindow {
    app_handle: AppHandle,
    window: Mutex<Option<tauri::WebviewWindow>>,
    /// 用于取消延迟隐藏的世代计数器
    hide_gen: Arc<AtomicU64>,
    /// 当前状态（供前端 ready 后补发）
    current_status: Mutex<String>,
    current_message: Mutex<String>,
    /// 录音计时停止信号（send(()) 停止计时）
    recording_stop_tx: Mutex<Option<std::sync::mpsc::Sender<()>>>,
}

impl IndicatorWindow {
    pub fn new(app_handle: AppHandle) -> anyhow::Result<Self> {
        Ok(Self {
            app_handle,
            window: Mutex::new(None),
            hide_gen: Arc::new(AtomicU64::new(0)),
            current_status: Mutex::new("idle".to_string()),
            current_message: Mutex::new("".to_string()),
            recording_stop_tx: Mutex::new(None),
        })
    }

    /// 应用启动时创建并显示窗口
    pub fn startup_show(&self) {
        match self.ensure_window() {
            Ok(w) => {
                let _ = w.show();
                info!("indicator 窗口启动显示");
            }
            Err(e) => log::warn!("indicator 启动显示失败: {}", e),
        }
    }

    fn ensure_window(&self) -> anyhow::Result<tauri::WebviewWindow> {
        let mut guard = self.window.lock().unwrap();
        if guard.is_none() {
            // 尝试获取预定义的窗口
            let window = if let Some(win) = self.app_handle.get_webview_window("indicator") {
                win
            } else {
                // 如果预定义窗口不存在，创建新窗口
                WebviewWindowBuilder::new(
                    &self.app_handle,
                    "indicator",
                    tauri::WebviewUrl::App("#/indicator".into()),
                )
                .title("")
                .inner_size(400.0, 48.0)
                .resizable(false)
                .decorations(false)
                .always_on_top(true)
                .skip_taskbar(true)
                .transparent(false)
                .drag_and_drop(true)
                .visible(false)
                .build()?
            };
            
            // 临时：打开开发者工具
            #[cfg(debug_assertions)]
            window.open_devtools();
            
            // 设置 WS_EX_NOACTIVATE：点击悬浮窗时不抢走其他窗口的焦点
            #[cfg(target_os = "windows")]
            set_no_activate(&window);
            // 将窗口移动到屏幕底部中央
            #[cfg(target_os = "windows")]
            position_bottom_center(&window);
            *guard = Some(window);
        }
        Ok(guard.as_ref().unwrap().clone())
    }

    pub fn show(&self) {
        if let Ok(window) = self.ensure_window() {
            let _ = window.show();
            info!("显示指示器");
        }
    }

    pub fn hide(&self) {
        if let Ok(guard) = self.window.lock() {
            if let Some(ref w) = *guard {
                let _ = w.hide();
                info!("隐藏指示器");
            }
        }
    }

    pub fn is_visible(&self) -> bool {
        if let Ok(guard) = self.window.lock() {
            if let Some(ref w) = *guard {
                return w.is_visible().unwrap_or(false);
            }
        }
        false
    }

    /// 延迟隐藏，如果期间有新操作则自动取消
    pub fn hide_delayed(&self, delay_ms: u64) {
        let gen = self.hide_gen.fetch_add(1, Ordering::Relaxed) + 1;
        let hide_gen = self.hide_gen.clone();
        if let Ok(guard) = self.window.lock() {
            if let Some(window) = guard.as_ref() {
                let window_clone = window.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                    if hide_gen.load(Ordering::Relaxed) == gen {
                        let _ = window_clone.hide();
                    }
                });
            }
        }
    }

    /// 取消待执行的延迟隐藏
    pub fn cancel_delayed_hide(&self) {
        self.hide_gen.fetch_add(1, Ordering::Relaxed);
    }

    pub fn set_position(&self, x: i32, y: i32) {
        if let Ok(window) = self.ensure_window() {
            let _ = window.set_position(tauri::Position::Physical(
                tauri::PhysicalPosition { x, y },
            ));
        }
    }

    pub fn move_by(&self, dx: i32, dy: i32) {
        if let Ok(guard) = self.window.lock() {
            if let Some(ref w) = *guard {
                if let Ok(pos) = w.outer_position() {
                    let _ = w.set_position(tauri::Position::Physical(
                        tauri::PhysicalPosition {
                            x: pos.x + dx,
                            y: pos.y + dy,
                        },
                    ));
                }
            }
        }
    }

    /// 触发系统级原生拖拽（在 mousedown 事件中调用）
    pub fn start_drag(&self) {
        if let Ok(guard) = self.window.lock() {
            if let Some(ref w) = *guard {
                let _ = w.start_dragging();
            }
        }
    }

    // ── 状态方法 ──
    /// 录音中（hold-to-talk），同时启动前端计时
    pub fn set_recording(&self) {
        self.cancel_delayed_hide();
        self.stop_recording_timer(); // 先停旧的
        self.emit_status("recording", "0:00");
        // 启动计时线程：快速重发确保前端收到事件
        let (tx, rx) = std::sync::mpsc::channel::<()>();
        *self.recording_stop_tx.lock().unwrap() = Some(tx);
        let app_handle = self.app_handle.clone();
        std::thread::spawn(move || {
            let start = std::time::Instant::now();
            // 快速重发：50、10、100、200、500ms 各发一次，确保前端初始化后能收到
            for delay_ms in [10u64, 50, 100, 200, 500] {
                std::thread::sleep(std::time::Duration::from_millis(delay_ms));
                if rx.try_recv().is_ok() { return; }
                let _ = app_handle.emit_to(
                    tauri::EventTarget::WebviewWindow { label: "indicator".to_string() },
                    "indicator:status",
                    serde_json::json!({ "status": "recording", "message": "0:00" }),
                );
            }
            // 之后每秒更新计时
            loop {
                std::thread::sleep(std::time::Duration::from_millis(1000));
                if rx.try_recv().is_ok() { break; }
                let secs = start.elapsed().as_secs();
                let time_str = format!("{}:{:02}", secs / 60, secs % 60);
                let _ = app_handle.emit_to(
                    tauri::EventTarget::WebviewWindow { label: "indicator".to_string() },
                    "indicator:status",
                    serde_json::json!({ "status": "recording", "message": time_str }),
                );
            }
        });
    }

    /// 停止 hold-to-talk 计时线程
    pub fn stop_recording_timer(&self) {
        if let Ok(mut tx_guard) = self.recording_stop_tx.lock() {
            if let Some(tx) = tx_guard.take() {
                let _ = tx.send(());
            }
        }
    }

    /// ASR 引擎加载中
    pub fn set_loading(&self)     { self.emit_status("loading",    "模型加载中"); }
    /// 引擎就绪，回到 idle
    pub fn set_idle(&self)        { self.emit_status("idle", ""); }
    /// 自由说话模式
    pub fn set_freetalk(&self)    { self.cancel_delayed_hide(); self.emit_status("freetalk",    "0:00"); }
    /// 圆形识别中
    pub fn set_processing(&self)  { self.stop_recording_timer(); self.emit_status("processing", "识别中"); }
    /// 完成
    pub fn set_done(&self)        { self.emit_status("done",       "完成"); }
    /// 已取消
    pub fn set_cancelled(&self)   { self.stop_recording_timer(); self.emit_status("cancelled",  "已取消"); }
    /// 无语音内容
    pub fn set_no_voice(&self)    { self.emit_status("no_voice",   "无语音"); }
    /// 错误
    pub fn set_error(&self, msg: &str) { self.emit_status("error", msg); }

    /// 更新录音计时显示（freetalk 状态用）
    pub fn update_timer(&self, seconds: u64, is_freetalk: bool) {
        let time_str = format!("{}:{:02}", seconds / 60, seconds % 60);
        let status = if is_freetalk { "freetalk" } else { "recording" };
        self.emit_status(status, &time_str);
    }

    fn emit_status(&self, status: &str, message: &str) {
        // 记录当前状态供补发
        *self.current_status.lock().unwrap() = status.to_string();
        *self.current_message.lock().unwrap() = message.to_string();
        let result = self.app_handle.emit_to(
            tauri::EventTarget::WebviewWindow { label: "indicator".to_string() },
            "indicator:status",
            serde_json::json!({ "status": status, "message": message }),
        );
        info!("emit_status({}) → {:?}", status, result);
    }

    /// 前端 ready 后补发当前状态
    pub fn resend_status(&self) {
        let status = self.current_status.lock().unwrap().clone();
        let message = self.current_message.lock().unwrap().clone();
        let _ = self.app_handle.emit_to(
            tauri::EventTarget::WebviewWindow { label: "indicator".to_string() },
            "indicator:status",
            serde_json::json!({ "status": status, "message": message }),
        );
    }

    /// 推送实时音量到 indicator 窗口（录音中调用）
    pub fn emit_volume(&self, volume: f32) {
        if let Ok(guard) = self.window.lock() {
            if let Some(ref w) = *guard {
                let _ = w.emit("indicator:volume", volume);
            }
        }
    }
}
