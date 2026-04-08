// 全局热键管理模块
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use parking_lot::Mutex;
use tauri::{AppHandle, Emitter};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use log::{info, debug};

/// 长按判定阈值（毫秒）
const HOLD_THRESHOLD_MS: u64 = 300;
/// 连续按键防抖最小间隔（毫秒）
const LAST_STOP_GUARD_MS: u64 = 500;

pub struct HotkeyManager {
    hotkey_str: Arc<Mutex<String>>,
    pub is_active: Arc<AtomicBool>,
    pub is_freetalk: Arc<AtomicBool>,
    pub last_stop_ms: Arc<AtomicU64>,
    pub is_recording_hotkey: Arc<AtomicBool>,
    pub press_time: Arc<Mutex<Option<Instant>>>,
}

impl HotkeyManager {
    pub fn new() -> Self {
        Self {
            hotkey_str: Arc::new(Mutex::new("F2".to_string())),
            is_active: Arc::new(AtomicBool::new(false)),
            is_freetalk: Arc::new(AtomicBool::new(false)),
            last_stop_ms: Arc::new(AtomicU64::new(0)),
            is_recording_hotkey: Arc::new(AtomicBool::new(false)),
            press_time: Arc::new(Mutex::new(None)),
        }
    }

    /// 返回供 Builder::with_handler 使用的闭包
    /// 在 main.rs 的 tauri::Builder 构建阶段调用，只调用一次
    pub fn make_shortcut_handler(
        is_active: Arc<AtomicBool>,
        is_freetalk: Arc<AtomicBool>,
        last_stop_ms: Arc<AtomicU64>,
        is_recording_hotkey: Arc<AtomicBool>,
        press_time: Arc<Mutex<Option<Instant>>>,
    ) -> impl Fn(&AppHandle, &tauri_plugin_global_shortcut::Shortcut, tauri_plugin_global_shortcut::ShortcutEvent) + Send + Sync + 'static {
        move |app, _shortcut, event| {
            if is_recording_hotkey.load(Ordering::Relaxed) { return; }
            let active = is_active.load(Ordering::Relaxed);
            match event.state() {
                ShortcutState::Pressed => {
                    if is_freetalk.load(Ordering::Relaxed) {
                        debug!("自由说话中：按键停止");
                        is_active.store(false, Ordering::Relaxed);
                        is_freetalk.store(false, Ordering::Relaxed);
                        Self::store_stop_now(&last_stop_ms);
                        app.emit("hotkey:stop_freetalk", ()).ok();
                        return;
                    }
                    *press_time.lock() = Some(Instant::now());
                }
                ShortcutState::Released => {
                    let held_ms = press_time.lock()
                        .map_or(0, |t| t.elapsed().as_millis() as u64);
                    *press_time.lock() = None;
                    if active && !is_freetalk.load(Ordering::Relaxed) {
                        debug!("hold-to-talk 松手，停止录音");
                        is_active.store(false, Ordering::Relaxed);
                        Self::store_stop_now(&last_stop_ms);
                        app.emit("hotkey:stop_recording", ()).ok();
                    } else if !active && held_ms < HOLD_THRESHOLD_MS && Self::guard_ok(&last_stop_ms) {
                        debug!("短按，切换 free-talk");
                        is_active.store(true, Ordering::Relaxed);
                        is_freetalk.store(true, Ordering::Relaxed);
                        app.emit("hotkey:toggle_freetalk", ()).ok();
                    }
                    // 长按 start 由轮询线程触发
                }
            }
        }
    }

    /// 注册热键字符串 + 启动长按轮询线程（在 setup 阶段调用）
    pub fn init(&mut self, app_handle: AppHandle) -> anyhow::Result<()> {
        let hotkey_str = self.hotkey_str.lock().clone();
        info!("注册热键: {}", hotkey_str);

        // 注册快捷键
        app_handle.global_shortcut().register(hotkey_str.as_str())
            .map_err(|e| anyhow::anyhow!("注册热键 {} 失败: {:?}", hotkey_str, e))?;

        // 启动长按检测轮询线程
        let is_active2    = self.is_active.clone();
        let is_freetalk2  = self.is_freetalk.clone();
        let last_stop_ms2 = self.last_stop_ms.clone();
        let press_time2   = self.press_time.clone();
        let is_rec2       = self.is_recording_hotkey.clone();
        let handle2       = app_handle.clone();
        std::thread::spawn(move || {
            loop {
                std::thread::sleep(Duration::from_millis(30));
                if is_rec2.load(Ordering::Relaxed) { continue; }
                let active = is_active2.load(Ordering::Relaxed);
                let held = press_time2.lock().map_or(0, |t| t.elapsed().as_millis() as u64);
                if !active
                    && held >= HOLD_THRESHOLD_MS
                    && Self::guard_ok(&last_stop_ms2)
                    && !is_freetalk2.load(Ordering::Relaxed)
                {
                    debug!("长按触发 hold-to-talk");
                    is_active2.store(true, Ordering::Relaxed);
                    handle2.emit("hotkey:start_recording", ()).ok();
                }
            }
        });

        info!("热键 {} 注册成功", hotkey_str);
        Ok(())
    }

    /// 判断距上次停止是否超过防抖门限
    fn guard_ok(last_stop_ms: &Arc<AtomicU64>) -> bool {
        let last = last_stop_ms.load(Ordering::Relaxed);
        if last == 0 { return true; }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        now.saturating_sub(last) >= LAST_STOP_GUARD_MS
    }

    /// 记录当前时间为最后一次停止时间
    fn store_stop_now(last_stop_ms: &Arc<AtomicU64>) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        last_stop_ms.store(now, Ordering::Relaxed);
    }

    pub fn set_hotkey(&self, vk: i32) {
        // vk 转换为字符串名称存储
        let name = vk_to_name(vk);
        *self.hotkey_str.lock() = name;
    }

    pub fn set_recording_hotkey(&self, recording: bool) {
        self.is_recording_hotkey.store(recording, Ordering::Relaxed);
        if !recording {
            self.is_active.store(false, Ordering::Relaxed);
            Self::store_stop_now(&self.last_stop_ms);
        }
    }

    /// 热键当前是否处于按下状态（press_time 为 Some 即表示按住中）
    pub fn is_key_pressed(&self) -> bool {
        self.press_time.lock().is_some()
    }
}

// 虚拟键码映射
pub mod key_codes {
    pub const VK_F1: i32 = 0x70;
    pub const VK_F2: i32 = 0x71;
    pub const VK_F3: i32 = 0x72;
    pub const VK_F4: i32 = 0x73;
    pub const VK_F5: i32 = 0x74;
    pub const VK_F6: i32 = 0x75;
    pub const VK_F7: i32 = 0x76;
    pub const VK_F8: i32 = 0x77;
    pub const VK_F9: i32 = 0x78;
    pub const VK_F10: i32 = 0x79;
    pub const VK_F11: i32 = 0x7A;
    pub const VK_F12: i32 = 0x7B;
    pub const VK_ESCAPE: i32 = 0x1B;
    pub const VK_SPACE: i32 = 0x20;
    pub const VK_CONTROL: i32 = 0x11;
    pub const VK_SHIFT: i32 = 0x10;
    pub const VK_MENU: i32 = 0x12; // Alt
}

pub fn vk_to_name(vk: i32) -> String {
    use key_codes::*;
    match vk {
        VK_F1 => "F1".to_string(),
        VK_F2 => "F2".to_string(),
        VK_F3 => "F3".to_string(),
        VK_F4 => "F4".to_string(),
        VK_F5 => "F5".to_string(),
        VK_F6 => "F6".to_string(),
        VK_F7 => "F7".to_string(),
        VK_F8 => "F8".to_string(),
        VK_F9 => "F9".to_string(),
        VK_F10 => "F10".to_string(),
        VK_F11 => "F11".to_string(),
        VK_F12 => "F12".to_string(),
        VK_ESCAPE => "Escape".to_string(),
        VK_SPACE => "Space".to_string(),
        VK_CONTROL => "Control".to_string(),
        VK_SHIFT => "Shift".to_string(),
        VK_MENU => "Alt".to_string(),
        0x41..=0x5A => ((vk as u8 - 0x41 + b'A') as char).to_string(),
        0x30..=0x39 => ((vk as u8 - 0x30 + b'0') as char).to_string(),
        _ => format!("Key({})", vk),
    }
}
