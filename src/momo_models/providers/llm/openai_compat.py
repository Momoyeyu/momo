"""OpenAI-compatible 薄客户端：通吃 mlx_lm.server / vLLM / DeepSeek。

**仅 eval/bench 脚本用**——运行时编排器经 MOMO_LLM_ENDPOINT 直连大脑，
不经本层（无 LLM 网关，ADR-006）。已知坑：vLLM 实测曾 502，评测需容忍。
"""

from collections.abc import AsyncIterator

from momo_models.providers.base import ChatDelta, ChatRequest, LLMProvider

_TODO = "openai_compat 接入：P3 eval/bench 需要时实现（openai.AsyncOpenAI）"


class OpenAICompatProvider(LLMProvider):
    """`base_url` 指向 MOMO_LLM_ENDPOINT；`api_key` 本地可空。"""

    name = "openai_compat"

    def __init__(self, base_url: str, api_key: str = "", model: str = "") -> None:
        self.base_url = base_url
        self.api_key = api_key
        self.model = model

    async def chat_stream(self, req: ChatRequest) -> AsyncIterator[ChatDelta]:
        raise NotImplementedError(_TODO)
        yield  # pragma: no cover —— 保持 async generator 类型

    async def list_models(self) -> list[str]:
        raise NotImplementedError(_TODO)

    async def aclose(self) -> None:
        return None
