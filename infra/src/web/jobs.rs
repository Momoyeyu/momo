//! `/api/jobs` handler 集：REST（submit/list/detail/cancel）+ SSE 日志 tail。
//! handler 只做 HTTP↔core 适配：业务全部走 `jobs::store`/`jobs::runner`，
//! JSON 顶层恒 `schema:1`，与 CLI `--json` 同口径。

use std::convert::Infallible;

use axum::extract::{Path, Query};
use axum::http::StatusCode;
use axum::response::sse::{Event, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use tokio_stream::wrappers::ReceiverStream;

use super::api::error;
use crate::api::dto;
use crate::jobs::runner;
use crate::jobs::store::{self, JobStore};

/// `/api/jobs` 子路由：挂到 api.rs 的 `nest("/jobs", ..)`。
/// 与 WebState 无关（jobs 只读写 store），泛型以便 nest 到任意 state 下。
pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    Router::new()
        .route("/", get(list_jobs).post(submit_job))
        .route("/{id}", get(get_job))
        .route("/{id}/cancel", post(cancel_job))
        .route("/{id}/logs", get(job_logs))
}

/// rescan（#54）：读侧 reconcile，把非终态 job 按 process identity 收口；
/// 失败只 warn 不挡请求（读侧降级，list/load 自己再报 IO 错误）。
fn reconcile(store: &JobStore) {
    match store.reconcile() {
        Ok(notes) => {
            for n in notes {
                eprintln!("warn: jobs rescan: {n}");
            }
        }
        Err(e) => eprintln!("warn: jobs rescan failed: {e}"),
    }
}

/// `GET /api/jobs`：全量列表（created_at 升序）。
async fn list_jobs() -> Response {
    let store = JobStore::open_default();
    reconcile(&store);
    match store.list() {
        Ok((jobs, _warnings)) => Json(dto::JobListReport {
            schema: dto::SCHEMA_VERSION,
            status: "ok",
            jobs,
        })
        .into_response(),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, "internal", e.to_string()),
    }
}

/// `GET /api/jobs/{id}`：单条 record；未知 id → JSON 404。
async fn get_job(Path(id): Path<String>) -> Response {
    let store = JobStore::open_default();
    reconcile(&store);
    match store.load(&id) {
        Ok(Some(rec)) => Json(dto::JobReport {
            schema: dto::SCHEMA_VERSION,
            status: "ok",
            job: rec,
            note: None,
        })
        .into_response(),
        Ok(None) => not_found(&id),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, "internal", e.to_string()),
    }
}

/// `POST /api/jobs`：建档 + spawn detached runner（同 `jobs submit`）。
async fn submit_job(Json(body): Json<dto::JobSubmitRequest>) -> Response {
    if body.args.is_empty() || body.args[0].trim().is_empty() {
        return error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "body.args must be a non-empty argv array",
        );
    }
    let store = JobStore::open_default();
    let desc = body.desc.unwrap_or_else(|| body.args.join(" "));
    match runner::submit(
        &store,
        body.kind.unwrap_or(crate::jobs::JobKind::Train),
        body.args,
        desc,
    ) {
        Ok(rec) => Json(dto::JobReport {
            schema: dto::SCHEMA_VERSION,
            status: "ok",
            job: rec,
            note: None,
        })
        .into_response(),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, "internal", e.to_string()),
    }
}

/// `POST /api/jobs/{id}/cancel`：canceling + kill；终态幂等。
async fn cancel_job(Path(id): Path<String>) -> Response {
    let store = JobStore::open_default();
    reconcile(&store);
    match runner::cancel(&store, &id) {
        Ok((rec, note)) => Json(dto::JobReport {
            schema: dto::SCHEMA_VERSION,
            status: "ok",
            job: rec,
            note: Some(note.to_string()),
        })
        .into_response(),
        Err(crate::output::AppError::Config(m)) if m.contains("not found") => not_found(&id),
        Err(e) => error(StatusCode::INTERNAL_SERVER_ERROR, "internal", e.to_string()),
    }
}

/// `GET /api/jobs/{id}/logs?cursor=N`：SSE tail。
/// 每条 `event: log` 携带 `{cursor, chunk}`（cursor=字节偏移，重连续传用）；
/// job 终态且日志排空后发 `event: end`（携带最终 status）并关闭流。
/// cursor 越过文件长度（截断）时从 0 重读。未知 id → JSON 404（非 SSE）。
async fn job_logs(Path(id): Path<String>, Query(q): Query<LogQuery>) -> Response {
    let store = JobStore::open_default();
    reconcile(&store);
    let rec = match store.load(&id) {
        Ok(Some(r)) => r,
        Ok(None) => return not_found(&id),
        Err(e) => return error(StatusCode::INTERNAL_SERVER_ERROR, "internal", e.to_string()),
    };
    let path = std::path::PathBuf::from(&rec.log_path);
    let mut cursor = q.cursor.unwrap_or(0);

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(16);
    tokio::spawn(async move {
        loop {
            match store::read_chunk(&path, cursor) {
                Ok((cur, chunk)) => {
                    cursor = cur;
                    if !chunk.is_empty() {
                        let data = serde_json::to_string(&dto::JobLogChunk { cursor: cur, chunk })
                            .expect("job log chunk serialize");
                        if tx
                            .send(Ok(Event::default().event("log").data(data)))
                            .await
                            .is_err()
                        {
                            return; // 客户端断开
                        }
                    }
                }
                Err(e) => {
                    let data =
                        serde_json::to_string(&dto::ApiError::new("internal", e.to_string()))
                            .expect("api error serialize");
                    let _ = tx
                        .send(Ok(Event::default().event("error").data(data)))
                        .await;
                    return;
                }
            }
            let terminal = store
                .load(&id)
                .ok()
                .flatten()
                .is_some_and(|r| r.status.is_terminal());
            if terminal {
                // 终态：本轮增量已发完即可关流。
                let status = store
                    .load(&id)
                    .ok()
                    .flatten()
                    .map(|r| r.status.as_str().to_string())
                    .unwrap_or_else(|| "unknown".into());
                let _ = tx
                    .send(Ok(Event::default().event("end").data(
                        serde_json::to_string(&dto::JobLogEnd { status })
                            .expect("job log end serialize"),
                    )))
                    .await;
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        }
    });
    Sse::new(ReceiverStream::new(rx)).into_response()
}

#[derive(Debug, Deserialize)]
struct LogQuery {
    #[serde(default)]
    cursor: Option<u64>,
}

fn not_found(id: &str) -> Response {
    error(
        StatusCode::NOT_FOUND,
        "not_found",
        format!("job {id} not found"),
    )
}
