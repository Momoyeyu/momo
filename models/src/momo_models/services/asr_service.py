"""momo-svc-asr：/ws（v2）+ health/ready + 调试 POST /v1/asr/transcribe。

内部管线：silero-vad 断句 → SenseVoice 识别 → input.vad / input.audio.segment。
"""

from fastapi import FastAPI

from momo_models.config import get_settings
from momo_models.logging import configure_logging
from momo_models.providers.registry import create_asr_provider
from momo_models.services.common import attach_warmup, make_app, serve


def create_app() -> FastAPI:
    settings = get_settings()
    configure_logging(settings.log_level, settings.log_format)
    return make_app(
        role="asr",
        settings=settings,
        debug_path="/v1/asr/transcribe",
        on_startup=attach_warmup(create_asr_provider, settings, "load"),
    )


def main() -> None:
    settings = get_settings()
    app = create_app()
    serve(app, settings.asr_listen)


if __name__ == "__main__":
    main()
