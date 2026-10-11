"""momo-svc-embed：BGE-M3 embedding 服务（P3；默认 127.0.0.1:7812 / MOMO_EMBED_LISTEN）。

端点：momo-ws v1 /ws + /healthz + /readyz（embed 消息类型 P3 定稿，
当前握手后一律 error not implemented）。
"""

from fastapi import FastAPI

from momo_models.config import get_settings
from momo_models.logging import configure_logging
from momo_models.services.common import make_app, serve


def create_app() -> FastAPI:
    settings = get_settings()
    configure_logging(settings.log_level, settings.log_format)
    return make_app(role="embed", settings=settings)


def main() -> None:
    settings = get_settings()
    app = create_app()
    serve(app, settings.embed_listen)


if __name__ == "__main__":
    main()
