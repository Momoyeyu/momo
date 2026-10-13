//! job store：`${XDG_STATE_HOME:-~/.local/state}/momo/jobs/` 持久化。
//! 落盘布局（与 serve 状态目录同约定，不提供 `MOMO_*` 覆盖键）：
//! - `jobs.jsonl`：每次状态迁移 append 一行 `{id, kind, status, ts, detail?}`，
//!   fsync 落盘；重建时按序回放取最后态（索引是迁移的权威记录）；
//! - `meta/<id>.json`：JobRecord 全量，tmp+rename 原子写（同 models.lock.save 口径）；
//! - `logs/<id>.log`：payload stdout/stderr 合流追加，cursor=字节偏移。
//!
//! 迁移写入顺序固定「先索引后 meta」：crash 只可能丢 meta 的新版本，
//! rescan 以索引最后态回补 meta；反过来（先 meta 后索引）会把 meta 已生效的
//! 迁移回滚成索引旧态，不允许。

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::jobs::{now_ms, JobKind, JobRecord, JobStatus};
use crate::output::AppError;
use crate::serve::proc;

const JOBS_JSONL: &str = "jobs.jsonl";

/// jobs 落盘根：`<state_dir>/jobs/`。
pub fn jobs_dir() -> PathBuf {
    proc::state_dir().join("jobs")
}

/// jobs.jsonl 单行：迁移索引。`detail` 为迁移原因（cancel/interrupted 等）。
#[derive(Debug, Serialize, Deserialize)]
struct IndexLine {
    id: String,
    kind: JobKind,
    status: JobStatus,
    ts: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    detail: Option<String>,
}

/// job store 句柄：目录约定 + 读写原语。
#[derive(Debug, Clone)]
pub struct JobStore {
    dir: PathBuf,
}

impl JobStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// 默认 store（state dir 约定）。
    pub fn open_default() -> Self {
        Self::new(jobs_dir())
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn meta_dir(&self) -> PathBuf {
        self.dir.join("meta")
    }

    fn logs_dir(&self) -> PathBuf {
        self.dir.join("logs")
    }

    fn jsonl_path(&self) -> PathBuf {
        self.dir.join(JOBS_JSONL)
    }

    fn meta_path(&self, id: &str) -> Result<PathBuf, AppError> {
        check_id(id)?;
        Ok(self.meta_dir().join(format!("{id}.json")))
    }

    /// `logs/<id>.log`；id 先过 `check_id` 防 `../` 穿越。
    pub fn log_path(&self, id: &str) -> Result<PathBuf, AppError> {
        check_id(id)?;
        Ok(self.logs_dir().join(format!("{id}.log")))
    }

    fn ensure_dirs(&self) -> Result<(), AppError> {
        fs::create_dir_all(self.meta_dir())?;
        fs::create_dir_all(self.logs_dir())?;
        Ok(())
    }

    /// 建档：id=ULID、status=queued；先写索引行再物化 meta。
    pub fn create(
        &self,
        kind: JobKind,
        args: Vec<String>,
        desc: String,
    ) -> Result<JobRecord, AppError> {
        self.ensure_dirs()?;
        let id = ulid::Ulid::new().to_string();
        let now = now_ms();
        let rec = JobRecord {
            id: id.clone(),
            kind,
            desc,
            args,
            status: JobStatus::Queued,
            created_at: now,
            updated_at: now,
            remote_id: None,
            pid: None,
            runner_pid: None,
            log_path: self.log_path(&id)?.display().to_string(),
            exit: None,
            error: None,
        };
        self.append_index(&rec, None)?;
        self.save(&rec)?;
        Ok(rec)
    }

    /// meta 原子写：先写 `<id>.tmp` 再 rename（Windows 不允许 rename 覆盖，先删目标）。
    pub fn save(&self, rec: &JobRecord) -> Result<(), AppError> {
        self.ensure_dirs()?;
        let path = self.meta_path(&rec.id)?;
        let tmp = path.with_extension("tmp");
        let text = serde_json::to_string_pretty(rec)
            .map_err(|e| AppError::Config(format!("serialize job {}: {e}", rec.id)))?;
        fs::write(&tmp, text)?;
        if path.exists() {
            fs::remove_file(&path)?;
        }
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    /// 状态迁移：非法迁移 → Config 错误（不摸盘）；合法迁移先索引（append+fsync）
    /// 后 meta。同态重写（to == 当前态）只刷 meta，不重复记索引（幂等收尾）。
    pub fn transition(
        &self,
        rec: &mut JobRecord,
        to: JobStatus,
        detail: Option<String>,
    ) -> Result<(), AppError> {
        let changed = to != rec.status;
        if changed && !rec.status.can_transition(to) {
            return Err(AppError::Config(format!(
                "job {}: illegal transition {} -> {}",
                rec.id,
                rec.status.as_str(),
                to.as_str()
            )));
        }
        rec.status = to;
        rec.updated_at = now_ms();
        // error 只承载 failed 原因；离开/未进 failed 时清掉残值。
        rec.error = if to == JobStatus::Failed {
            detail.clone()
        } else {
            None
        };
        if changed {
            self.append_index(rec, detail)?;
        }
        self.save(rec)
    }

    /// jobs.jsonl 追加一行并 fsync。
    fn append_index(&self, rec: &JobRecord, detail: Option<String>) -> Result<(), AppError> {
        self.ensure_dirs()?;
        let line = serde_json::to_string(&IndexLine {
            id: rec.id.clone(),
            kind: rec.kind,
            status: rec.status,
            ts: now_ms(),
            detail,
        })
        .map_err(|e| AppError::Config(format!("serialize index line: {e}")))?;
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.jsonl_path())?;
        f.write_all(line.as_bytes())?;
        f.write_all(b"\n")?;
        f.sync_data()?;
        Ok(())
    }

    /// 读单个 meta；不存在 → None，损坏 → Config 错误（坏 meta 不静默）。
    pub fn load(&self, id: &str) -> Result<Option<JobRecord>, AppError> {
        let path = self.meta_path(id)?;
        if !path.exists() {
            return Ok(None);
        }
        let text = fs::read_to_string(&path)?;
        let rec: JobRecord = serde_json::from_str(&text)
            .map_err(|e| AppError::Config(format!("parse job meta {}: {e}", path.display())))?;
        Ok(Some(rec))
    }

    /// 全量 meta，按 created_at（同毫秒按 id）升序。
    /// 返回 (records, warnings)：坏 meta 文件跳过并记 warn，不拖垮整个 list。
    pub fn list(&self) -> Result<(Vec<JobRecord>, Vec<String>), AppError> {
        let mut out = Vec::new();
        let mut warnings = Vec::new();
        let dir = self.meta_dir();
        if dir.is_dir() {
            for ent in fs::read_dir(&dir)?.flatten() {
                let path = ent.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                match fs::read_to_string(&path)
                    .ok()
                    .and_then(|t| serde_json::from_str::<JobRecord>(&t).ok())
                {
                    Some(rec) => out.push(rec),
                    None => warnings.push(format!("skipped corrupt meta {}", path.display())),
                }
            }
        }
        out.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
        Ok((out, warnings))
    }

    /// 回放 jobs.jsonl：id → 最后一次迁移 (status, ts)；坏行跳过并记 warn。
    pub fn replay_index(&self) -> (BTreeMap<String, (JobStatus, u64)>, Vec<String>) {
        let mut last: BTreeMap<String, (JobStatus, u64)> = BTreeMap::new();
        let mut warnings = Vec::new();
        let path = self.jsonl_path();
        let Ok(text) = fs::read_to_string(&path) else {
            return (last, warnings);
        };
        for (i, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<IndexLine>(line) {
                Ok(l) => {
                    last.insert(l.id, (l.status, l.ts));
                }
                Err(e) => warnings.push(format!(
                    "{}: skipped corrupt line {}: {e}",
                    path.display(),
                    i + 1
                )),
            }
        }
        (last, warnings)
    }
}

/// job id 只允许 ULID 字符集（Crockford base32 大小写+数字），
/// 拒绝路径分隔与 `..`：CLI/API 的 id 入参先过它再拼路径。
fn check_id(id: &str) -> Result<(), AppError> {
    let ok = !id.is_empty() && id.len() <= 64 && id.bytes().all(|b| b.is_ascii_alphanumeric());
    if ok {
        Ok(())
    } else {
        Err(AppError::Config(format!("invalid job id '{id}'")))
    }
}

/// tail 最后 n 行：先读末尾 1 MiB 窗口，不够再全量重读（同 serve logs 口径）。
pub fn read_tail(path: &Path, n: usize) -> Result<Vec<String>, AppError> {
    const WINDOW: u64 = 1 << 20;
    let mut f = fs::File::open(path)?;
    let len = f.metadata()?.len();
    let start = len.saturating_sub(WINDOW);
    let mut lines = tail_lines(&mut f, start, n)?;
    if start > 0 && lines.len() < n {
        lines = tail_lines(&mut f, 0, n)?;
    }
    Ok(lines)
}

fn tail_lines(f: &mut fs::File, start: u64, n: usize) -> Result<Vec<String>, AppError> {
    f.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    let text = String::from_utf8_lossy(&buf);
    let mut lines: Vec<String> = text
        .split('\n')
        .map(|s| s.trim_end_matches('\r').to_string())
        .collect();
    if lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    if start > 0 && !lines.is_empty() {
        // 窗口首行可能是半行，丢弃。
        lines.remove(0);
    }
    Ok(lines
        .into_iter()
        .rev()
        .take(n)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect())
}

/// 从字节 offset 读增量：返回 (新 cursor, 文本)。
/// `offset > 文件长度`（截断/轮转）→ 从 0 重读；文件不存在 → (0, "")。
pub fn read_chunk(path: &Path, offset: u64) -> Result<(u64, String), AppError> {
    if !path.is_file() {
        return Ok((0, String::new()));
    }
    let len = fs::metadata(path)?.len();
    let start = if offset > len { 0 } else { offset };
    let mut f = fs::File::open(path)?;
    f.seek(SeekFrom::Start(start))?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    Ok((len, String::from_utf8_lossy(&buf).into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::JobStatus;

    fn store(tag: &str) -> (JobStore, PathBuf) {
        let dir = std::env::temp_dir().join(format!("momo-jobs-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        (JobStore::new(dir.clone()), dir)
    }

    /// create → load/list 往返；索引行含 queued。
    #[test]
    fn create_persists_record_and_index() {
        let (s, dir) = store("create");
        let rec = s
            .create(JobKind::Train, vec!["echo".into(), "hi".into()], "t".into())
            .unwrap();
        assert_eq!(rec.status, JobStatus::Queued);

        let loaded = s.load(&rec.id).unwrap().unwrap();
        assert_eq!(loaded.args, vec!["echo", "hi"]);

        let (all, warns) = s.list().unwrap();
        assert!(warns.is_empty());
        assert_eq!(all.len(), 1);

        let jsonl = fs::read_to_string(dir.join("jobs.jsonl")).unwrap();
        assert!(jsonl.contains("\"status\":\"queued\""));
        let _ = fs::remove_dir_all(&dir);
    }

    /// 迁移写：索引+meta 同步更新；非法迁移不摸盘。
    #[test]
    fn transition_appends_index_and_updates_meta() {
        let (s, dir) = store("trans");
        let mut rec = s
            .create(JobKind::Train, vec!["x".into()], String::new())
            .unwrap();
        s.transition(&mut rec, JobStatus::Preparing, None).unwrap();
        s.transition(&mut rec, JobStatus::Running, None).unwrap();
        assert!(s.transition(&mut rec, JobStatus::Canceled, None).is_err());
        assert_eq!(s.load(&rec.id).unwrap().unwrap().status, JobStatus::Running);

        let jsonl = fs::read_to_string(dir.join("jobs.jsonl")).unwrap();
        assert_eq!(jsonl.lines().count(), 3); // queued + preparing + running
        let _ = fs::remove_dir_all(&dir);
    }

    /// 索引回放：多迁移取最后态；坏行跳过并 warn。
    #[test]
    fn replay_index_last_write_wins_and_skips_corrupt() {
        let (s, dir) = store("replay");
        let mut rec = s
            .create(JobKind::Serve, vec!["x".into()], String::new())
            .unwrap();
        s.transition(&mut rec, JobStatus::Preparing, None).unwrap();
        // 手写一行坏行 + 一条新的迁移行
        let mut f = OpenOptions::new()
            .append(true)
            .open(dir.join("jobs.jsonl"))
            .unwrap();
        writeln!(f, "{{not json").unwrap();
        writeln!(
            f,
            "{}",
            serde_json::json!({"id": rec.id, "kind": "serve", "status": "succeeded", "ts": 9})
        )
        .unwrap();
        drop(f);

        let (last, warns) = s.replay_index();
        assert_eq!(last[&rec.id].0, JobStatus::Succeeded);
        assert_eq!(warns.len(), 1, "{warns:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    /// check_id 拒绝路径穿越。
    #[test]
    fn check_id_rejects_traversal() {
        assert!(check_id("01JXYZABC").is_ok());
        assert!(check_id("../evil").is_err());
        assert!(check_id("a/b").is_err());
        assert!(check_id("").is_err());
    }

    /// read_tail / read_chunk：尾部窗口与 cursor 增量。
    #[test]
    fn log_readers() {
        let (_s, dir) = store("tail");
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("x.log");
        fs::write(&p, "l1\nl2\nl3\n").unwrap();
        assert_eq!(read_tail(&p, 2).unwrap(), vec!["l2", "l3"]);

        let (cur, chunk) = read_chunk(&p, 0).unwrap();
        assert_eq!(chunk, "l1\nl2\nl3\n");
        let mut f = OpenOptions::new().append(true).open(&p).unwrap();
        writeln!(f, "l4").unwrap();
        drop(f);
        let (cur2, chunk2) = read_chunk(&p, cur).unwrap();
        assert_eq!(chunk2, "l4\n");
        assert!(cur2 > cur);
        // cursor 越过文件长度（截断）→ 从 0 重读
        let (_, chunk3) = read_chunk(&p, u64::MAX).unwrap();
        assert_eq!(chunk3, "l1\nl2\nl3\nl4\n");
        let _ = fs::remove_dir_all(&dir);
    }
}
