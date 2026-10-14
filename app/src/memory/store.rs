//! 长期记忆 sqlite 存储（docs/03-features/memory.md）。
//!
//! rusqlite bundled：编译期内置 sqlite，无系统依赖/openssl。失效语义：
//! `superseded_by`/`deleted_at` 任一非空即 invalid——标失效不物理删，
//! 审计面保留完整历史（Zep 式纠错）；召回与抽取上下文只见 valid 条目。
//!
//! `Connection` 非 `Sync`，本 store 用 `Mutex` 串行化；所有操作都是本地
//! sqlite 短事务，持锁时间可忽略，调用方不需要 spawn_blocking。

use std::{
    collections::HashSet,
    fmt,
    path::{Path, PathBuf},
    sync::Mutex,
};

use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;

use crate::{error::AppError, proto::message::new_id};

/// 记忆库路径：`{config_dir}/momo/memory.db`（与 settings.json 同目录先例）。
pub fn memory_db_path() -> PathBuf {
    dirs::config_dir()
        .map(|d| d.join("momo").join("memory.db"))
        .unwrap_or_else(|| PathBuf::from("momo-memory.db"))
}

/// 打开默认路径上的记忆库；失败记 warn 返回 `None`（调用方退化为无记忆运行）。
pub fn open_default() -> Option<std::sync::Arc<MemoryStore>> {
    match MemoryStore::open(&memory_db_path()) {
        Ok(store) => Some(std::sync::Arc::new(store)),
        Err(e) => {
            tracing::warn!(%e, "memory store unavailable, running without long-term memory");
            None
        }
    }
}

/// 记忆类型（LangMem 分类学：事实/经历/习惯）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MemoryKind {
    /// 事实/偏好：用户的偏好、习惯、约定。
    Semantic,
    /// 经历/会话摘要：可引用的共同经历。
    Episodic,
    /// 行为模式/规则：互动中沉淀的习惯。
    Procedural,
}

impl MemoryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Semantic => "semantic",
            Self::Episodic => "episodic",
            Self::Procedural => "procedural",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "semantic" => Some(Self::Semantic),
            "episodic" => Some(Self::Episodic),
            "procedural" => Some(Self::Procedural),
            _ => None,
        }
    }
}

impl fmt::Display for MemoryKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 人工编辑产生的条目 provenance。
pub const PROVENANCE_MANUAL: &str = "manual";
/// 无替代条的失效标记：`superseded_by = "void:<source>"`。
pub const VOID_EXTRACT: &str = "void:extract";

/// human block（用户画像）字符上限（Letta core block 式，写不进就只截）。
pub const HUMAN_BLOCK_MAX_CHARS: usize = 2000;
/// 单条记忆正文字符上限。
pub const MEMORY_CONTENT_MAX_CHARS: usize = 2000;
/// 注入 system prompt 的记忆段总字符上限。
pub const MEMORY_BLOCK_CHARS: usize = 1500;
/// 每回合注入的 recent semantic / procedural 条数。
const SEMANTIC_RECENT: usize = 8;
const PROCEDURAL_RECENT: usize = 3;
/// episodic 关键词重叠召回条数。
const EPISODIC_TOP_K: usize = 5;
/// episodic 召回的最小共享 bigram 数。
const EPISODIC_MIN_OVERLAP: usize = 2;
/// add 去重的粗查范围（最近 valid 条数）与 bigram Jaccard 阈值。
const DEDUP_SCAN_LIMIT: usize = 500;
const DEDUP_JACCARD: f64 = 0.85;
/// 抽取 prompt 里喂给 LLM 的 valid 记忆摘要条数。
const EXTRACT_CONTEXT_LIMIT: usize = 30;

/// 一条记忆记录（API 输出同构）。
#[derive(Debug, Clone, Serialize)]
pub struct Memory {
    pub id: String,
    pub kind: MemoryKind,
    pub content: String,
    pub created_at: String,
    pub updated_at: String,
    pub provenance_turn_id: Option<String>,
    pub confidence: f64,
    /// `None` = valid；替代条目 id 或 `void:*`。
    pub superseded_by: Option<String>,
    /// 用户软删时刻。
    pub deleted_at: Option<String>,
}

impl Memory {
    /// 失效 = 被取代或被删除；valid 条目才进召回/抽取上下文。
    pub fn is_valid(&self) -> bool {
        self.superseded_by.is_none() && self.deleted_at.is_none()
    }
}

/// `add` 结果：入库或判重跳过。
#[derive(Debug, PartialEq, Eq)]
pub enum AddOutcome {
    /// 新条入库，值为新条 id。
    Added(String),
    /// 命中已存在条目（归一化全等或 bigram Jaccard 超阈值），值为旧条 id。
    Duplicate(String),
}

/// 长期记忆存储；进程内单例（AppState 持有 `Arc` 共享给 session/API）。
pub struct MemoryStore {
    conn: Mutex<Connection>,
}

impl fmt::Debug for MemoryStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryStore").finish_non_exhaustive()
    }
}

impl MemoryStore {
    /// 打开（必要时创建）`path` 上的记忆库并迁移 schema。
    pub fn open(path: &Path) -> Result<Self, AppError> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| AppError::Internal(format!("create {}: {e}", dir.display())))?;
        }
        let conn = Connection::open(path)
            .map_err(|e| AppError::Internal(format!("open {}: {e}", path.display())))?;
        Self::init(conn)
    }

    /// 进程内临时库（测试与 `AppState::env_only`/`from_deps`：不碰用户磁盘）。
    pub fn in_memory() -> Result<Self, AppError> {
        let conn = Connection::open_in_memory()
            .map_err(|e| AppError::Internal(format!("open in-memory memory db: {e}")))?;
        Self::init(conn)
    }

    fn init(conn: Connection) -> Result<Self, AppError> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS memories (
               id TEXT PRIMARY KEY,
               kind TEXT NOT NULL CHECK(kind IN ('semantic','episodic','procedural')),
               content TEXT NOT NULL,
               created_at TEXT NOT NULL,
               updated_at TEXT NOT NULL,
               provenance_turn_id TEXT,
               confidence REAL NOT NULL DEFAULT 1.0,
               superseded_by TEXT,
               deleted_at TEXT
             );
             CREATE INDEX IF NOT EXISTS idx_memories_valid
               ON memories(kind, created_at)
               WHERE superseded_by IS NULL AND deleted_at IS NULL;
             CREATE TABLE IF NOT EXISTS human_block (
               id INTEGER PRIMARY KEY CHECK(id = 1),
               content TEXT NOT NULL DEFAULT '',
               updated_at TEXT NOT NULL
             );",
        )
        .map_err(|e| AppError::Internal(format!("migrate memory db: {e}")))?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().expect("memory db lock poisoned")
    }

    /// 写一条记忆：归一化全等 / bigram Jaccard>阈值 的 valid 条目存在则判重跳过。
    pub fn add(
        &self,
        kind: MemoryKind,
        content: &str,
        provenance_turn_id: Option<&str>,
        confidence: f64,
    ) -> Result<AddOutcome, AppError> {
        let content = content.trim();
        if content.is_empty() {
            return Err(AppError::Internal("memory content empty".into()));
        }
        let conn = self.conn();
        if let Some(existing) = find_duplicate(&conn, content)
            .map_err(|e| AppError::Internal(format!("dedup scan: {e}")))?
        {
            return Ok(AddOutcome::Duplicate(existing));
        }
        let id = new_id("mem");
        conn.execute(
            "INSERT INTO memories
               (id, kind, content, created_at, updated_at, provenance_turn_id, confidence)
             VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                     strftime('%Y-%m-%dT%H:%M:%fZ','now'), ?4, ?5)",
            params![id, kind.as_str(), content, provenance_turn_id, confidence],
        )
        .map_err(|e| AppError::Internal(format!("insert memory: {e}")))?;
        Ok(AddOutcome::Added(id))
    }

    /// 取单条（含 invalid）。
    pub fn get(&self, id: &str) -> Result<Option<Memory>, AppError> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, kind, content, created_at, updated_at,
                        provenance_turn_id, confidence, superseded_by, deleted_at
                 FROM memories WHERE id = ?1",
            )
            .map_err(|e| AppError::Internal(format!("prepare get memory: {e}")))?;
        let mut rows = stmt
            .query_map(params![id], row_to_memory)
            .map_err(|e| AppError::Internal(format!("get memory: {e}")))?;
        match rows.next() {
            Some(Ok(m)) => Ok(Some(m)),
            Some(Err(e)) => Err(AppError::Internal(format!("get memory: {e}"))),
            None => Ok(None),
        }
    }

    /// 列表：默认只返回 valid；`all=true` 含失效/已删（审计视图）。
    /// 分页 `limit`/`offset`，按 created_at 倒序（新的在前）。
    pub fn list(
        &self,
        kind: Option<MemoryKind>,
        all: bool,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Memory>, AppError> {
        let conn = self.conn();
        let valid = if all {
            ""
        } else {
            "AND superseded_by IS NULL AND deleted_at IS NULL"
        };
        let sql = format!(
            "SELECT id, kind, content, created_at, updated_at,
                    provenance_turn_id, confidence, superseded_by, deleted_at
             FROM memories
             WHERE (?1 IS NULL OR kind = ?1) {valid}
             ORDER BY created_at DESC, id DESC LIMIT ?2 OFFSET ?3"
        );
        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| AppError::Internal(format!("prepare list memories: {e}")))?;
        let kind_str = kind.map(|k| k.as_str());
        let rows = stmt
            .query_map(
                params![kind_str, limit as i64, offset as i64],
                row_to_memory,
            )
            .map_err(|e| AppError::Internal(format!("list memories: {e}")))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| AppError::Internal(format!("list memories: {e}")))
    }

    /// 标失效：valid 条目置 `superseded_by = by`（替代条 id 或 `void:*`）。
    /// 返回 `false` = 不存在或已失效。
    pub fn supersede(&self, id: &str, by: &str) -> Result<bool, AppError> {
        let conn = self.conn();
        let n = conn
            .execute(
                "UPDATE memories SET superseded_by = ?2,
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
                 WHERE id = ?1 AND superseded_by IS NULL AND deleted_at IS NULL",
                params![id, by],
            )
            .map_err(|e| AppError::Internal(format!("supersede memory: {e}")))?;
        Ok(n > 0)
    }

    /// 用户编辑 = 旧条 superseded_by 指向新条，新条 `provenance_turn_id='manual'`
    /// （不删旧条，审计面保留历史）。
    pub fn edit(&self, id: &str, new_content: &str) -> Result<Option<Memory>, AppError> {
        let Some(old) = self.get(id)? else {
            return Ok(None);
        };
        if !old.is_valid() {
            return Ok(None);
        }
        // edit 不走 add 的去重：用户显式改成与现存相同文本也应生效。
        let conn = self.conn();
        let new_id_ = new_id("mem");
        conn.execute(
            "INSERT INTO memories
               (id, kind, content, created_at, updated_at, provenance_turn_id, confidence)
             VALUES (?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                     strftime('%Y-%m-%dT%H:%M:%fZ','now'), ?4, ?5)",
            params![
                new_id_,
                old.kind.as_str(),
                new_content.trim(),
                PROVENANCE_MANUAL,
                old.confidence
            ],
        )
        .map_err(|e| AppError::Internal(format!("insert edited memory: {e}")))?;
        conn.execute(
            "UPDATE memories SET superseded_by = ?2,
               updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
             WHERE id = ?1",
            params![id, new_id_],
        )
        .map_err(|e| AppError::Internal(format!("supersede edited memory: {e}")))?;
        drop(conn);
        self.get(&new_id_)
    }

    /// 用户删除 = 软删（`deleted_at`），审计面仍可见。
    /// 返回 `false` = 不存在或已删除。
    pub fn soft_delete(&self, id: &str) -> Result<bool, AppError> {
        let conn = self.conn();
        let n = conn
            .execute(
                "UPDATE memories SET deleted_at = strftime('%Y-%m-%dT%H:%M:%fZ','now'),
                   updated_at = strftime('%Y-%m-%dT%H:%M:%fZ','now')
                 WHERE id = ?1 AND deleted_at IS NULL",
                params![id],
            )
            .map_err(|e| AppError::Internal(format!("delete memory: {e}")))?;
        Ok(n > 0)
    }

    /// L0 human block 全文（未写过返回空串 + epoch）。
    pub fn human_block(&self) -> Result<(String, String), AppError> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare("SELECT content, updated_at FROM human_block WHERE id = 1")
            .map_err(|e| AppError::Internal(format!("prepare human block: {e}")))?;
        let mut rows = stmt
            .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
            .map_err(|e| AppError::Internal(format!("read human block: {e}")))?;
        match rows.next() {
            Some(Ok(v)) => Ok(v),
            Some(Err(e)) => Err(AppError::Internal(format!("read human block: {e}"))),
            None => Ok((String::new(), String::new())),
        }
    }

    /// 全量改写 human block（Letta core-block 式；超限截断由调用方校验）。
    pub fn set_human_block(&self, content: &str) -> Result<(), AppError> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO human_block (id, content, updated_at)
             VALUES (1, ?1, strftime('%Y-%m-%dT%H:%M:%fZ','now'))
             ON CONFLICT(id) DO UPDATE SET content = excluded.content,
               updated_at = excluded.updated_at",
            params![content],
        )
        .map_err(|e| AppError::Internal(format!("write human block: {e}")))?;
        Ok(())
    }

    /// 抽取 prompt 的现有记忆上下文：最近 valid 条目摘要（`id [kind] content`）。
    pub fn valid_context_lines(&self) -> Result<Vec<String>, AppError> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare(
                "SELECT id, kind, content FROM memories
                 WHERE superseded_by IS NULL AND deleted_at IS NULL
                 ORDER BY created_at DESC LIMIT ?1",
            )
            .map_err(|e| AppError::Internal(format!("prepare memory context: {e}")))?;
        let rows = stmt
            .query_map(params![EXTRACT_CONTEXT_LIMIT as i64], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| AppError::Internal(format!("memory context: {e}")))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map(|v| {
                v.into_iter()
                    .map(|(id, kind, content)| format!("{id} [{kind}] {content}"))
                    .collect()
            })
            .map_err(|e| AppError::Internal(format!("memory context: {e}")))
    }

    /// 读路径：拼注入 system prompt 的 `[长期记忆]` 段。
    ///
    /// human block 全文 + 近 N 条 semantic + 近 M 条 procedural + 关键词
    /// 重叠 episodic top-K；总字符超 `MEMORY_BLOCK_CHARS` 时从末尾逐条
    /// 丢弃——顺序即优先级：先丢 episodic，再丢最旧条目
    /// （memory.md §读路径）。无任何内容返回 `None`。
    pub fn prompt_block(&self, user_text: &str) -> Option<String> {
        let conn = self.conn();
        let mut entries: Vec<String> = Vec::new();

        if let Ok(Some(hb)) = human_block_locked(&conn) {
            if !hb.is_empty() {
                entries.push(format!("- 用户画像：{hb}"));
            }
        }
        entries.extend(recent_lines(&conn, MemoryKind::Semantic, SEMANTIC_RECENT));
        entries.extend(recent_lines(
            &conn,
            MemoryKind::Procedural,
            PROCEDURAL_RECENT,
        ));
        entries.extend(episodic_matches(&conn, user_text).unwrap_or_default());
        if entries.is_empty() {
            return None;
        }

        let render = |items: &[String]| format!("\n\n[长期记忆]\n{}", items.join("\n"));
        let mut block = render(&entries);
        while block.chars().count() > MEMORY_BLOCK_CHARS && entries.len() > 1 {
            entries.pop();
            block = render(&entries);
        }
        Some(block)
    }
}

fn row_to_memory(r: &rusqlite::Row<'_>) -> rusqlite::Result<Memory> {
    let kind: String = r.get(1)?;
    Ok(Memory {
        id: r.get(0)?,
        kind: MemoryKind::parse(&kind).unwrap_or(MemoryKind::Semantic),
        content: r.get(2)?,
        created_at: r.get(3)?,
        updated_at: r.get(4)?,
        provenance_turn_id: r.get(5)?,
        confidence: r.get(6)?,
        superseded_by: r.get(7)?,
        deleted_at: r.get(8)?,
    })
}

fn human_block_locked(conn: &Connection) -> Result<Option<String>, rusqlite::Error> {
    conn.query_row("SELECT content FROM human_block WHERE id = 1", [], |r| {
        r.get(0)
    })
    .optional()
}

/// 近 N 条 valid 同 kind 条目（新→旧），渲染成 `- content` 行。
fn recent_lines(conn: &Connection, kind: MemoryKind, n: usize) -> Vec<String> {
    let Ok(mut stmt) = conn.prepare(
        "SELECT content FROM memories
         WHERE kind = ?1 AND superseded_by IS NULL AND deleted_at IS NULL
         ORDER BY created_at DESC LIMIT ?2",
    ) else {
        return Vec::new();
    };
    let rows = stmt.query_map(params![kind.as_str(), n as i64], |r| r.get::<_, String>(0));
    match rows {
        Ok(rows) => rows.flatten().map(|c| format!("- {c}")).collect(),
        Err(_) => Vec::new(),
    }
}

/// episodic 关键词重叠召回：valid episodic 中与 `user_text` 共享 bigram ≥
/// `EPISODIC_MIN_OVERLAP` 的条目，按共享数 desc、新→旧取 top-K。
fn episodic_matches(conn: &Connection, user_text: &str) -> Result<Vec<String>, rusqlite::Error> {
    let query_grams = bigrams(&normalize(user_text));
    if query_grams.is_empty() {
        return Ok(Vec::new());
    }
    let mut stmt = conn.prepare(
        "SELECT content, created_at FROM memories
         WHERE kind = 'episodic' AND superseded_by IS NULL AND deleted_at IS NULL
         ORDER BY created_at DESC LIMIT 200",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    let mut scored: Vec<(usize, String, String)> = rows
        .flatten()
        .filter_map(|(content, at)| {
            let score = bigrams(&normalize(&content))
                .intersection(&query_grams)
                .count();
            (score >= EPISODIC_MIN_OVERLAP).then(|| (score, at, format!("- {content}")))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.cmp(&a.1)));
    Ok(scored
        .into_iter()
        .take(EPISODIC_TOP_K)
        .map(|(_, _, l)| l)
        .collect())
}

/// add 去重：最近 valid 条中找归一化全等或 bigram Jaccard > 阈值的条目 id。
fn find_duplicate(conn: &Connection, content: &str) -> Result<Option<String>, rusqlite::Error> {
    let norm = normalize(content);
    let grams = bigrams(&norm);
    let mut stmt = conn.prepare(
        "SELECT id, content FROM memories
         WHERE superseded_by IS NULL AND deleted_at IS NULL
         ORDER BY created_at DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![DEDUP_SCAN_LIMIT as i64], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
    })?;
    for row in rows.flatten() {
        let (id, existing) = row;
        let norm_existing = normalize(&existing);
        if norm_existing == norm {
            return Ok(Some(id));
        }
        if jaccard(&grams, &bigrams(&norm_existing)) > DEDUP_JACCARD {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

/// 归一化：trim + 折叠空白 + 小写（去重以内容为准，不看大小写/排版差异）。
fn normalize(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// 字符 bigram 集合：CJK 友好的相似度单位（不依赖分词）。
fn bigrams(s: &str) -> HashSet<String> {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() < 2 {
        return chars.into_iter().map(|c| c.to_string()).collect();
    }
    chars
        .windows(2)
        .map(|w| w.iter().collect::<String>())
        .collect()
}

fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let inter = a.intersection(b).count() as f64;
    let union = a.union(b).count() as f64;
    if union == 0.0 { 0.0 } else { inter / union }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> MemoryStore {
        MemoryStore::in_memory().expect("in-memory store")
    }

    #[test]
    fn add_and_get_roundtrip() {
        let s = store();
        let AddOutcome::Added(id) = s
            .add(MemoryKind::Semantic, "喜欢抹茶", Some("t_1"), 0.9)
            .expect("add")
        else {
            panic!("expected Added");
        };
        let m = s.get(&id).expect("get").expect("exists");
        assert_eq!(m.kind, MemoryKind::Semantic);
        assert_eq!(m.content, "喜欢抹茶");
        assert_eq!(m.provenance_turn_id.as_deref(), Some("t_1"));
        assert!(m.is_valid());
    }

    #[test]
    fn add_dedups_exact_and_similar() {
        let s = store();
        let AddOutcome::Added(first) = s
            .add(MemoryKind::Semantic, "用户喜欢抹茶拿铁", None, 1.0)
            .expect("add")
        else {
            panic!("expected Added");
        };
        // 归一化全等。
        assert_eq!(
            s.add(MemoryKind::Semantic, "  用户喜欢抹茶拿铁 ", None, 1.0)
                .expect("add"),
            AddOutcome::Duplicate(first.clone())
        );
        // 高相似（仅差末尾一字 → bigram Jaccard > 0.85）。
        assert_eq!(
            s.add(MemoryKind::Semantic, "用户喜欢抹茶拿铁啊", None, 1.0)
                .expect("add"),
            AddOutcome::Duplicate(first)
        );
    }

    #[test]
    fn add_keeps_dissimilar() {
        let s = store();
        s.add(MemoryKind::Semantic, "喜欢抹茶", None, 1.0).unwrap();
        let AddOutcome::Added(_) = s
            .add(MemoryKind::Semantic, "讨厌香菜", None, 1.0)
            .expect("add dissimilar")
        else {
            panic!("expected Added for dissimilar content");
        };
    }

    #[test]
    fn supersede_marks_invalid_once() {
        let s = store();
        let AddOutcome::Added(id) = s
            .add(MemoryKind::Episodic, "上周一起看了海", None, 1.0)
            .unwrap()
        else {
            panic!("expected Added");
        };
        assert!(s.supersede(&id, VOID_EXTRACT).unwrap());
        let m = s.get(&id).unwrap().unwrap();
        assert_eq!(m.superseded_by.as_deref(), Some(VOID_EXTRACT));
        assert!(!m.is_valid());
        // 已失效条目不可二次 supersede。
        assert!(!s.supersede(&id, "mem_other").unwrap());
    }

    #[test]
    fn edit_supersedes_old_with_manual_provenance() {
        let s = store();
        let AddOutcome::Added(old_id) = s
            .add(MemoryKind::Semantic, "住在朝阳区", Some("t_7"), 1.0)
            .unwrap()
        else {
            panic!("expected Added");
        };
        let new = s.edit(&old_id, "住在海淀区").unwrap().unwrap();
        assert_eq!(new.content, "住在海淀区");
        assert_eq!(new.provenance_turn_id.as_deref(), Some(PROVENANCE_MANUAL));
        assert_eq!(new.kind, MemoryKind::Semantic);
        let old = s.get(&old_id).unwrap().unwrap();
        assert_eq!(old.superseded_by.as_deref(), Some(new.id.as_str()));
        assert!(!old.is_valid());
    }

    #[test]
    fn edit_missing_or_invalid_returns_none() {
        let s = store();
        assert!(s.edit("mem_none", "x").unwrap().is_none());
        let AddOutcome::Added(id) = s.add(MemoryKind::Semantic, "旧值", None, 1.0).unwrap()
        else {
            panic!("expected Added");
        };
        s.soft_delete(&id).unwrap();
        assert!(s.edit(&id, "新值").unwrap().is_none());
    }

    #[test]
    fn soft_delete_excludes_from_valid_list() {
        let s = store();
        let AddOutcome::Added(id) = s.add(MemoryKind::Semantic, "临时事实", None, 1.0).unwrap()
        else {
            panic!("expected Added");
        };
        assert_eq!(s.list(None, false, 50, 0).unwrap().len(), 1);
        assert!(s.soft_delete(&id).unwrap());
        assert!(!s.soft_delete(&id).unwrap(), "second delete is a no-op");
        assert!(s.list(None, false, 50, 0).unwrap().is_empty());
        assert_eq!(s.list(None, true, 50, 0).unwrap().len(), 1);
    }

    #[test]
    fn list_filters_kind_and_paginates() {
        let s = store();
        for i in 0..5 {
            s.add(MemoryKind::Semantic, &format!("事实编号{i}"), None, 1.0)
                .unwrap();
        }
        s.add(MemoryKind::Episodic, "一次海边散步", None, 1.0)
            .unwrap();
        assert_eq!(
            s.list(Some(MemoryKind::Episodic), false, 50, 0)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            s.list(Some(MemoryKind::Semantic), false, 2, 0)
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            s.list(Some(MemoryKind::Semantic), false, 2, 2)
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            s.list(Some(MemoryKind::Semantic), false, 2, 4)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn human_block_roundtrip() {
        let s = store();
        assert_eq!(s.human_block().unwrap().0, "");
        s.set_human_block("用户叫阿树，工程师").unwrap();
        let (content, updated) = s.human_block().unwrap();
        assert_eq!(content, "用户叫阿树，工程师");
        assert!(!updated.is_empty());
        s.set_human_block("改口：住城西").unwrap();
        assert_eq!(s.human_block().unwrap().0, "改口：住城西");
    }

    #[test]
    fn prompt_block_includes_sections_and_respects_budget() {
        let s = store();
        assert!(s.prompt_block("你好").is_none());
        s.set_human_block("工程师，养猫").unwrap();
        s.add(MemoryKind::Semantic, "喜欢抹茶", None, 1.0).unwrap();
        s.add(MemoryKind::Procedural, "睡前喜欢闲聊", None, 1.0)
            .unwrap();
        s.add(MemoryKind::Episodic, "上周一起看了海的日落", None, 1.0)
            .unwrap();
        let block = s.prompt_block("聊聊海的日落吧").unwrap();
        assert!(block.contains("用户画像：工程师，养猫"));
        assert!(block.contains("喜欢抹茶"));
        assert!(block.contains("睡前喜欢闲聊"));
        assert!(
            block.contains("上周一起看了海的日落"),
            "episodic 应命中关键词"
        );
        // 无关 episodic 不进注入段。
        s.add(MemoryKind::Episodic, "去过一次牙医", None, 1.0)
            .unwrap();
        let block = s.prompt_block("今天午饭吃什么").unwrap();
        assert!(!block.contains("去过一次牙医"));
    }
}
