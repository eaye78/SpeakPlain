// 数据存储模块
use rusqlite::{Connection, Result as SqlResult};
use serde::{Serialize, Deserialize};
use std::path::PathBuf;
use log::{info, debug};

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
                confidence REAL DEFAULT 0.0
            )",
            [],
        )?;
        
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
        
        debug!("数据库表初始化完成");
        Ok(())
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
    
    /// 获取所有设置
    pub fn get_all_settings(&self) -> SqlResult<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare("SELECT key, value FROM app_settings")?;
        
        let items = stmt.query_map([], |row| {
            Ok((row.get(0)?, row.get(1)?))
        })?;
        
        items.collect()
    }
}
