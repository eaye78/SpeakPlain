// 数据存储模块
use rusqlite::{Connection, Result as SqlResult};
use serde::{Serialize, Deserialize};
use std::path::PathBuf;
use log::{info, debug};
use crate::llm::{LlmProviderConfig, Persona};
use crate::config::CommandMapping;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryItem {
    pub id: i64,
    pub text: String,
    pub created_at: String,
    pub duration_sec: u32,
    pub confidence: f32,
}

pub struct Storage {
    conn: Connection,
    #[allow(dead_code)]
    db_path: PathBuf,
}

impl Storage {
    pub fn new() -> anyhow::Result<Self> {
        let db_path = Self::get_db_path()?;
        info!("数据库路径: {:?}", db_path);
        
        let conn = Connection::open(&db_path)?;
        let storage = Self { conn, db_path };
        storage.init_tables()?;
        
        Ok(storage)
    }
    
    fn get_db_path() -> anyhow::Result<PathBuf> {
        // 使用用户数据目录
        let data_dir = dirs::data_dir()
            .ok_or_else(|| anyhow::anyhow!("无法获取数据目录"))?
            .join("SpeakPlain");
        
        std::fs::create_dir_all(&data_dir)?;
        Ok(data_dir.join("speakplain.db"))
    }
    
    fn init_tables(&self) -> SqlResult<()> {
        // 识别历史表
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS recognition_history (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                text TEXT NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                duration_sec INTEGER DEFAULT 0,
                confidence REAL DEFAULT 0.0,
                raw_text TEXT,
                llm_text TEXT,
                persona_id TEXT,
                llm_provider TEXT,
                llm_success INTEGER DEFAULT 0
            )",
            [],
        )?;

        // 历史表迁移：补充 LLM 字段（旧库升级时不出错）
        let migrations = [
            "ALTER TABLE recognition_history ADD COLUMN raw_text TEXT",
            "ALTER TABLE recognition_history ADD COLUMN llm_text TEXT",
            "ALTER TABLE recognition_history ADD COLUMN persona_id TEXT",
            "ALTER TABLE recognition_history ADD COLUMN llm_provider TEXT",
            "ALTER TABLE recognition_history ADD COLUMN llm_success INTEGER DEFAULT 0",
        ];
        for sql in &migrations {
            let _ = self.conn.execute(sql, []); // 字段已存在时会报错，忽略
        }
        
        // 自定义词典表
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS custom_dictionary (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                word TEXT UNIQUE NOT NULL,
                replacement TEXT,
                category TEXT DEFAULT 'general'
            )",
            [],
        )?;

        // 应用设置表
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS app_settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            )",
            [],
        )?;

        // 自定义人设表
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS personas (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT,
                system_prompt TEXT NOT NULL
            )",
            [],
        )?;

        // LLM Provider 配置表
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS llm_providers (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                provider_type TEXT NOT NULL,
                api_base_url TEXT NOT NULL,
                api_key TEXT NOT NULL DEFAULT '',
                model_name TEXT NOT NULL,
                timeout_secs INTEGER NOT NULL DEFAULT 30,
                max_tokens INTEGER NOT NULL DEFAULT 512,
                temperature REAL NOT NULL DEFAULT 0.7
            )",
            [],
        )?;

        debug!("数据库表初始化完成");
        Ok(())
    }
    
    /// 添加识别历史（支持 LLM 润色字段）
    pub fn add_history_with_llm(
        &self,
        text: &str,
        duration_sec: u32,
        raw_text: Option<&str>,
        llm_text: Option<&str>,
        persona_id: Option<&str>,
        llm_provider: Option<&str>,
        llm_success: bool,
    ) -> SqlResult<i64> {
        self.conn.execute(
            "INSERT INTO recognition_history 
             (text, duration_sec, raw_text, llm_text, persona_id, llm_provider, llm_success)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                text,
                duration_sec,
                raw_text,
                llm_text,
                persona_id,
                llm_provider,
                llm_success as i32,
            ],
        )?;
        let id = self.conn.last_insert_rowid();
        self.cleanup_old_history(50)?;
        Ok(id)
    }

    /// 添加识别历史
    pub fn add_history(&self, text: &str, duration_sec: u32) -> SqlResult<i64> {
        self.conn.execute(
            "INSERT INTO recognition_history (text, duration_sec) VALUES (?1, ?2)",
            [text, &duration_sec.to_string()],
        )?;
        
        let id = self.conn.last_insert_rowid();
        debug!("添加历史记录: id={}, text={}", id, text.chars().take(30).collect::<String>());
        
        // 只保留最近50条
        self.cleanup_old_history(50)?;
        
        Ok(id)
    }
    
    /// 获取识别历史
    pub fn get_history(&self) -> SqlResult<Vec<HistoryItem>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, text, created_at, duration_sec, confidence 
             FROM recognition_history 
             ORDER BY created_at DESC 
             LIMIT 50"
        )?;
        
        let items = stmt.query_map([], |row| {
            Ok(HistoryItem {
                id: row.get(0)?,
                text: row.get(1)?,
                created_at: row.get(2)?,
                duration_sec: row.get(3)?,
                confidence: row.get(4)?,
            })
        })?;
        
        items.collect()
    }
    
    /// 删除历史记录
    pub fn delete_history(&self, id: i64) -> SqlResult<()> {
        self.conn.execute(
            "DELETE FROM recognition_history WHERE id = ?1",
            [id],
        )?;
        debug!("删除历史记录: id={}", id);
        Ok(())
    }
    
    /// 清空所有历史记录
    pub fn clear_all_history(&self) -> SqlResult<()> {
        self.conn.execute("DELETE FROM recognition_history", [])?;
        debug!("清空所有历史记录");
        Ok(())
    }
    
    /// 清理旧历史记录
    fn cleanup_old_history(&self, keep_count: usize) -> SqlResult<()> {
        self.conn.execute(
            "DELETE FROM recognition_history WHERE id NOT IN (
                SELECT id FROM recognition_history ORDER BY created_at DESC LIMIT ?1
            )",
            [keep_count],
        )?;
        Ok(())
    }
    
    /// 添加自定义词典项
    pub fn add_dictionary_item(&self, word: &str, replacement: Option<&str>, category: &str) -> SqlResult<()> {
        let replacement = replacement.unwrap_or(word);
        
        self.conn.execute(
            "INSERT OR REPLACE INTO custom_dictionary (word, replacement, category) 
             VALUES (?1, ?2, ?3)",
            [word, replacement, category],
        )?;
        
        debug!("添加词典项: {} -> {}", word, replacement);
        Ok(())
    }
    
    /// 获取自定义词典
    pub fn get_dictionary(&self) -> SqlResult<Vec<(String, String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT word, replacement, category FROM custom_dictionary ORDER BY word"
        )?;
        
        let items = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        
        items.collect()
    }
    
    /// 删除词典项
    pub fn delete_dictionary_item(&self, word: &str) -> SqlResult<()> {
        self.conn.execute(
            "DELETE FROM custom_dictionary WHERE word = ?1",
            [word],
        )?;
        Ok(())
    }
    
    /// 保存设置
    pub fn set_setting(&self, key: &str, value: &str) -> SqlResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO app_settings (key, value, updated_at) 
             VALUES (?1, ?2, CURRENT_TIMESTAMP)",
            [key, value],
        )?;
        Ok(())
    }
    
    /// 获取设置
    pub fn get_setting(&self, key: &str) -> SqlResult<Option<String>> {
        let mut stmt = self.conn.prepare(
            "SELECT value FROM app_settings WHERE key = ?1"
        )?;
        
        let mut rows = stmt.query([key])?;
        
        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }
    
    // ── 自定义人设 CRUD ────────────────────────────────────────────────────────

    pub fn get_custom_personas(&self) -> SqlResult<Vec<Persona>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, description, system_prompt FROM personas ORDER BY name"
        )?;
        let items = stmt.query_map([], |row| {
            Ok(Persona {
                id:            row.get(0)?,
                name:          row.get(1)?,
                description:   row.get(2)?,
                system_prompt: row.get(3)?,
                is_builtin:    false,
            })
        })?;
        items.collect()
    }

    pub fn save_persona(&self, p: &Persona) -> SqlResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO personas (id, name, description, system_prompt) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![p.id, p.name, p.description, p.system_prompt],
        )?;
        Ok(())
    }

    pub fn delete_persona(&self, id: &str) -> SqlResult<()> {
        self.conn.execute("DELETE FROM personas WHERE id = ?1", [id])?;
        Ok(())
    }

    // ── LLM Provider CRUD ─────────────────────────────────────────────────────

    pub fn get_llm_providers(&self) -> SqlResult<Vec<LlmProviderConfig>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, name, provider_type, api_base_url, api_key, model_name, timeout_secs, max_tokens, temperature
             FROM llm_providers ORDER BY name"
        )?;
        let items = stmt.query_map([], |row| {
            let provider_type_str: String = row.get(2)?;
            let provider_type = provider_type_str.parse()
                .unwrap_or(crate::llm::LlmProviderType::OpenaiCompatible);
            Ok(LlmProviderConfig {
                id:            row.get(0)?,
                name:          row.get(1)?,
                provider_type,
                api_base_url:  row.get(3)?,
                api_key:       row.get(4)?,
                model_name:    row.get(5)?,
                timeout_secs:  row.get::<_, i64>(6)? as u64,
                max_tokens:    row.get::<_, i64>(7)? as u32,
                temperature:   row.get::<_, f64>(8)? as f32,
            })
        })?;
        items.collect()
    }

    pub fn save_llm_provider(&self, p: &LlmProviderConfig) -> SqlResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO llm_providers 
             (id, name, provider_type, api_base_url, api_key, model_name, timeout_secs, max_tokens, temperature)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            rusqlite::params![
                p.id, p.name, p.provider_type.to_string(),
                p.api_base_url, p.api_key, p.model_name,
                p.timeout_secs as i64, p.max_tokens as i64, p.temperature as f64,
            ],
        )?;
        Ok(())
    }

    pub fn delete_llm_provider(&self, id: &str) -> SqlResult<()> {
        self.conn.execute("DELETE FROM llm_providers WHERE id = ?1", [id])?;
        Ok(())
    }

    /// 获取所有设置
    pub fn get_all_settings(&self) -> SqlResult<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare("SELECT key, value FROM app_settings")?;
        
        let items = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
        
        items.collect()
    }

    // ── 指令映射 CRUD ─────────────────────────────────────────────────────────

    /// 获取所有指令映射
    pub fn get_command_mappings(&self) -> anyhow::Result<Vec<CommandMapping>> {
        match self.get_setting("command_mappings")? {
            Some(value) => {
                let mappings = serde_json::from_str::<Vec<CommandMapping>>(&value)?;
                Ok(mappings)
            }
            None => Ok(Vec::new()),
        }
    }

    /// 保存指令映射列表
    pub fn set_command_mappings(&self, mappings: &[CommandMapping]) -> anyhow::Result<()> {
        let value = serde_json::to_string(mappings)?;
        self.set_setting("command_mappings", &value)?;
        Ok(())
    }
}
