"""momo-svc-* 统一骨架：app 工厂。

端点（设计：momo-docs/02-repos/momo-models.md「统一服务接口」）：
- `WS /ws`      momo-ws v1（protocol.md）：hello{client:orchestrator} → hello.ok{role}
- `GET /healthz` 进程活（momoctl serve status/doctor 只认此口与 readyz）
- `GET /readyz`  模型已加载可服务；未就绪 503
- 调试 POST 口   非流式调试（不在运行时主路径），统一 {"error":{"code","message"}}

协议细节（protocol.md §3/§8）：首帧必须 hello，否则 close 4400；
version 非 v1 → error{VERSION_MISMATCH} + close 4001。
"""

import json
import secrets
import time
from collections.abc import Awaitable, Callable
from contextlib import asynccontextmanager
from typing import Any

import uvicorn
from fastapi import FastAPI, Request, WebSocket
from fastapi.responses import JSONResponse
from starlette.websockets import WebSocketDisconnect, WebSocketState

from momo_models.config import Settings
from momo_models.logging import get_logger

PROTOCOL_VERSION = "v1"

# 协议 §5 消息目录全量；模型服务链路只会上行其中一部分，
# 骨架阶段一律回 error{INTERNAL, "not implemented"}。
KNOWN_TYPES = frozenset(
    {
        "hello",
        "hello.ok",
        "input.text",
        "input.audio.segment",
        "input.vad",
        "output.sentence",
        "audio.chunk",
        "control.barge_in",
        "control.cancel",
        "state.turn",
        "action.register",
        "action.unregister",
        "action.invoke",
        "action.result",
        "error",
    }
)

log = get_logger(__name__)


def _error_frame(
    code: str, message: str, *, ref_id: str | None = None, fatal: bool = False
) -> dict:
    frame: dict[str, Any] = {"type": "error", "code": code, "message": message, "fatal": fatal}
    if ref_id is not None:
        frame["ref_id"] = ref_id
    return frame


def make_app(
    *,
    role: str,
    settings: Settings,
    debug_path: str | None = None,
    on_startup: Callable[[FastAPI], Awaitable[None]] | None = None,
) -> FastAPI:
    """构建某个角色（asr/tts/embed）的服务 app。"""

    @asynccontextmanager
    async def lifespan(app: FastAPI):
        app.state.ready = False
        app.state.provider = None
        if on_startup is not None:
            await on_startup(app)
        yield

    app = FastAPI(title=f"momo-svc-{role}", lifespan=lifespan)

    # ---------- 访问日志 ----------
    @app.middleware("http")
    async def access_log(request: Request, call_next):
        started = time.perf_counter()
        resp = await call_next(request)
        log.info(
            "http.access",
            method=request.method,
            path=request.url.path,
            status=resp.status_code,
            ms=round((time.perf_counter() - started) * 1000, 1),
        )
        return resp

    # ---------- 健康端点 ----------
    @app.get("/healthz")
    async def healthz() -> dict:
        return {"status": "ok", "role": role}

    @app.get("/readyz")
    async def readyz(request: Request) -> JSONResponse:
        if getattr(request.app.state, "ready", False):
            return JSONResponse({"status": "ready", "role": role})
        return JSONResponse(
            {"status": "not_ready", "role": role},
            status_code=503,
        )

    # ---------- 调试 POST 口（不在运行时主路径） ----------
    if debug_path is not None:

        @app.post(debug_path)
        async def debug_endpoint() -> JSONResponse:
            return JSONResponse(
                {"error": {"code": "NOT_IMPLEMENTED", "message": f"{debug_path} P0 实现"}},
                status_code=501,
            )

    # ---------- 异常 → error 信封 ----------
    @app.exception_handler(Exception)
    async def unhandled(request: Request, exc: Exception) -> JSONResponse:
        log.error("http.error", path=request.url.path, error=str(exc))
        return JSONResponse(
            {"error": {"code": "INTERNAL", "message": str(exc)}},
            status_code=500,
        )

    # ---------- momo-ws v1 /ws ----------
    @app.websocket("/ws")
    async def ws_endpoint(ws: WebSocket) -> None:
        await ws.accept()
        conn = log.bind(role=role, remote=str(ws.client))

        # 首帧必须文本 hello（protocol §3/§8）
        first = await ws.receive()
        if first.get("text") is None:
            await ws.close(code=4400)
            return
        try:
            hello = json.loads(first["text"])
        except (json.JSONDecodeError, TypeError):
            await ws.send_json(_error_frame("BAD_MESSAGE", "first frame must be hello json"))
            await ws.close(code=4400)
            return

        if not isinstance(hello, dict) or hello.get("type") != "hello":
            await ws.send_json(
                _error_frame("BAD_MESSAGE", "first message must be hello", fatal=True)
            )
            await ws.close(code=4400)
            return

        if hello.get("protocol") != "momo-ws" or hello.get("version") != PROTOCOL_VERSION:
            await ws.send_json(
                _error_frame(
                    "VERSION_MISMATCH",
                    f"server speaks momo-ws {PROTOCOL_VERSION}",
                    fatal=True,
                )
            )
            await ws.close(code=4001)
            return

        if hello.get("client") != "orchestrator":
            await ws.send_json(
                _error_frame("BAD_MESSAGE", "model link expects client=orchestrator", fatal=True)
            )
            await ws.close(code=4400)
            return

        session_id = f"sess_{secrets.token_hex(13).upper()}"
        await ws.send_json(
            {
                "type": "hello.ok",
                "version": PROTOCOL_VERSION,
                "session_id": session_id,
                "server": f"momo-svc-{role}",
                "role": role,
                "capabilities": [],
            }
        )
        conn.info("ws.hello", session_id=session_id)

        # 骨架阶段：握手后一切消息回 error 帧（known type → INTERNAL not implemented，
        # 未知 type → BAD_MESSAGE，协议 §2）。
        try:
            while ws.client_state == WebSocketState.CONNECTED:
                frame = await ws.receive()
                if frame.get("bytes") is not None:
                    await ws.send_json(
                        _error_frame("INTERNAL", "audio.chunk upstream not implemented (P0)")
                    )
                    continue
                text = frame.get("text")
                if text is None:
                    break
                try:
                    msg = json.loads(text)
                except json.JSONDecodeError:
                    await ws.send_json(_error_frame("BAD_MESSAGE", "invalid json"))
                    continue
                msg_type = msg.get("type") if isinstance(msg, dict) else None
                if msg_type not in KNOWN_TYPES:
                    await ws.send_json(_error_frame("BAD_MESSAGE", f"unknown type {msg_type!r}"))
                    continue
                if msg_type == "hello":
                    await ws.send_json(_error_frame("BAD_MESSAGE", "already negotiated"))
                    continue
                await ws.send_json(_error_frame("INTERNAL", f"{msg_type} not implemented (P0)"))
        except WebSocketDisconnect:
            conn.info("ws.disconnect", session_id=session_id)

    return app


def attach_warmup(
    provider_factory: Callable[[Settings], Any],
    settings: Settings,
    warmup_method: str,
) -> Callable[[FastAPI], Awaitable[None]]:
    """生成 on_startup 钩子：实例化 provider → warmup → 置 ready。

    骨架阶段 warmup 均为 NotImplementedError：记日志、readyz 维持 503。
    warmup 抛其他异常同样不就绪但不致命（服务仍可握手回报错误）。
    """

    async def _startup(app: FastAPI) -> None:
        try:
            provider = provider_factory(settings)
        except Exception as e:  # 含 MOMO_TTS_PROVIDER 非法值
            log.error("provider.init_failed", error=str(e))
            return
        app.state.provider = provider
        try:
            await getattr(provider, warmup_method)()
        except NotImplementedError as e:
            log.warning("provider.stub", provider=provider.name, todo=str(e))
        except Exception as e:
            log.error("provider.warmup_failed", provider=provider.name, error=str(e))
        else:
            app.state.ready = True
            log.info("provider.ready", provider=provider.name)

    return _startup


def serve(app: FastAPI, listen: str) -> None:
    """console script 共用入口：`host:port` → uvicorn。"""
    from momo_models.config import parse_listen

    host, port = parse_listen(listen)
    uvicorn.run(app, host=host, port=port, log_level="warning")
