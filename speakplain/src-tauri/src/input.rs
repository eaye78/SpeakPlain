// 文本输入与剤贴板管理模块
//
// 粘贴策略（二级 fallback）：
//   1. arboard + Ctrl+V  — 最快最兼容
//   2. enigo text()      — 直接输入，内置 fallback
use arboard::Clipboard;
use std::thread;
use std::time::Duration;
use log::{info, debug, warn};
use crate::config::{CommandMapping, ModifierKey};

#[cfg(windows)]
use windows::Win32::UI::Input::KeyboardAndMouse::*;

pub struct TextPaster {
    clipboard: Clipboard,
    saved_content: Option<String>,
    pub restore_clipboard: bool,
    pub paste_delay_ms: u64,
}

impl TextPaster {
    pub fn new() -> Self {
        Self {
            clipboard: Clipboard::new().expect("无法访问剤贴板"),
            saved_content: None,
            restore_clipboard: true,
            paste_delay_ms: 100,
        }
    }

    /// 粘贴文本到当前焦点应用（二级 fallback）
    pub fn paste(&mut self, text: &str) -> anyhow::Result<()> {
        info!("准备粘贴文本: {}", text.chars().take(50).collect::<String>());

        // 第一级：剤贴板 + Ctrl+V
        if self.try_clipboard_paste(text).is_ok() {
            return Ok(());
        }
        warn!("剤贴板粘贴失败，尝试 enigo 直接输入");

        // 第二级： enigo text()
        self.type_text(text)
    }

    fn try_clipboard_paste(&mut self, text: &str) -> anyhow::Result<()> {
        // 保存当前剤贴板
        if self.restore_clipboard {
            self.saved_content = self.clipboard.get_text().ok();
        }

        self.clipboard.set_text(text)?;
        thread::sleep(Duration::from_millis(50));
        self.simulate_ctrl_v();
        thread::sleep(Duration::from_millis(self.paste_delay_ms));

        // 恢复剤贴板
        if self.restore_clipboard {
            if let Some(ref saved) = self.saved_content.take() {
                let _ = self.clipboard.set_text(saved);
            }
        }

        debug!("剤贴板 + Ctrl+V 成功");
        Ok(())
    }

    /// 直接输入文本（不使用剤贴板）
    pub fn type_text(&self, text: &str) -> anyhow::Result<()> {
        info!("直接输入文本 (enigo)");

        #[cfg(windows)]
        {
            use enigo::{Enigo, Keyboard, Settings};
            let mut enigo = Enigo::new(&Settings::default())
                .map_err(|e| anyhow::anyhow!("创建 enigo 失败: {:?}", e))?;
            enigo.text(text)
                .map_err(|e| anyhow::anyhow!("输入文本失败: {:?}", e))?;
        }
        #[cfg(not(windows))]
        { let _ = text; }

        Ok(())
    }

    /// 模拟 Ctrl+V
    #[cfg(windows)]
    fn simulate_ctrl_v(&self) {
        use enigo::{Enigo, Key, Keyboard, Settings};
        if let Ok(mut enigo) = Enigo::new(&Settings::default()) {
            let _ = enigo.key(Key::Control, enigo::Direction::Press);
            let _ = enigo.key(Key::Unicode('v'), enigo::Direction::Click);
            let _ = enigo.key(Key::Control, enigo::Direction::Release);
        }
    }

    #[cfg(not(windows))]
    fn simulate_ctrl_v(&self) {}

    #[allow(dead_code)]
    pub fn set_restore_clipboard(&mut self, restore: bool) {
        self.restore_clipboard = restore;
    }
}

/// 执行指令映射（支持组合键）
#[cfg(windows)]
pub fn execute_command_mapping(mapping: &CommandMapping) -> anyhow::Result<()> {
    info!("执行指令映射: {} -> {:?} + {}", 
        mapping.command_text, 
        mapping.modifier, 
        mapping.key_name
    );
    
    unsafe {
        let mut inputs: Vec<INPUT> = Vec::new();
        
        // 根据修饰键添加按键按下事件
        match mapping.modifier {
            ModifierKey::Ctrl => {
                inputs.push(create_key_input(0x11, false)); // VK_CONTROL
            }
            ModifierKey::Alt => {
                inputs.push(create_key_input(0x12, false)); // VK_MENU
            }
            ModifierKey::Shift => {
                inputs.push(create_key_input(0x10, false)); // VK_SHIFT
            }
            ModifierKey::None => {}
        }
        
        // 添加主按键按下
        inputs.push(create_key_input(mapping.key_code, false));
        
        // 添加主按键释放
        inputs.push(create_key_input(mapping.key_code, true));
        
        // 根据修饰键添加按键释放事件
        match mapping.modifier {
            ModifierKey::Ctrl => {
                inputs.push(create_key_input(0x11, true));
            }
            ModifierKey::Alt => {
                inputs.push(create_key_input(0x12, true));
            }
            ModifierKey::Shift => {
                inputs.push(create_key_input(0x10, true));
            }
            ModifierKey::None => {}
        }
        
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }
    
    debug!("指令映射执行完成");
    Ok(())
}

#[cfg(not(windows))]
pub fn execute_command_mapping(_mapping: &CommandMapping) -> anyhow::Result<()> {
    Ok(())
}

/// 创建键盘输入事件
#[cfg(windows)]
unsafe fn create_key_input(key_code: i32, key_up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(key_code as u16),
                wScan: 0,
                dwFlags: if key_up { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}

/// 文本后处理
pub struct TextProcessor;

impl TextProcessor {
    /// 去除语气词
    pub fn remove_fillers(text: &str) -> String {
        let fillers = ["嗯", "啊", "呃", "哦", "哎", "um", "uh", "hmm", "ah", "er", "嗯嗯", "啊啊"];
        let mut result = text.to_string();
        
        for filler in &fillers {
            result = result.replace(filler, "");
        }
        
        // 清理多余空格
        while result.contains("  ") {
            result = result.replace("  ", " ");
        }
        
        result.trim().to_string()
    }
    
    /// 句首大写
    pub fn capitalize_sentences(text: &str) -> String {
        let mut result = String::new();
        let mut capitalize_next = true;
        
        for c in text.chars() {
            if capitalize_next && c.is_ascii_lowercase() {
                result.push(c.to_ascii_uppercase());
                capitalize_next = false;
            } else {
                result.push(c);
                if c == '.' || c == '?' || c == '!' || c == '。' || c == '？' || c == '！' {
                    capitalize_next = true;
                } else if c.is_ascii_alphabetic() {
                    capitalize_next = false;
                }
            }
        }
        
        result
    }
    
    /// 中英混合优化：在中英文之间加空格
    pub fn optimize_spacing(text: &str) -> String {
        let mut result = String::with_capacity(text.len() * 2);
        let chars: Vec<char> = text.chars().collect();
        
        for i in 0..chars.len() {
            result.push(chars[i]);
            
            if i + 1 < chars.len() {
                let curr = chars[i];
                let next = chars[i + 1];
                
                let curr_is_cjk = Self::is_cjk(curr);
                let next_is_cjk = Self::is_cjk(next);
                
                // 在中英文之间加空格
                if curr_is_cjk != next_is_cjk && next != ' ' && curr != ' ' {
                    result.push(' ');
                }
            }
        }
        
        result
    }
    
    fn is_cjk(c: char) -> bool {
        matches!(c as u32, 0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x20000..=0x2A6DF)
    }
    
    /// 完整的后处理流程
    pub fn post_process(text: &str, remove_fillers: bool, capitalize: bool) -> String {
        let mut result = text.to_string();
        
        if remove_fillers {
            result = Self::remove_fillers(&result);
        }
        
        result = Self::optimize_spacing(&result);
        
        if capitalize {
            result = Self::capitalize_sentences(&result);
        }
        
        result
    }
}
