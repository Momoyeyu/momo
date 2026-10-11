"""momo-svc-tts：TTS 服务（默认 127.0.0.1:7811 / MOMO_TTS_LISTEN）。

端点：momo-ws v1 /ws + /healthz + /readyz；调试口 POST /v1/tts/synthesize
（整段 wav 返回，供 momoctl bench 与手测，不在运行时主路径）。
后端由 MOMO_TTS_PROVIDER 选择（providers/registry.py）。
"""

from fastapi import FastAPI

from momo_models.config import get_settings
from momo_models.logging import configure_logging
from momo_models.providers.registry import create_tts_provider
from momo_models.services.common import attach_warmup, make_app, serve


def create_app() -> FastAPI:
    settings = get_settings()
    configure_logging(settings.log_level, settings.log_format)
    return make_app(
        role="tts",
        settings=settings,
        debug_path="/v1/tts/synthesize",
        on_startup=attach_warmup(create_tts_provider, settings, "warmup"),
    )


def main() -> None:
    settings = get_settings()
    app = create_app()
    serve(app, settings.tts_listen)


if __name__ == "__main__":
    main()
