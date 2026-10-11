"""DeepSeek 薄子类：仅承载 DS 特有参数差异，其余同 openai_compat。

仅 eval/bench 用；key 见 MOMO_DEEPSEEK_API_KEY（app 侧持有，P4 视觉共用）。
"""

from momo_models.providers.llm.openai_compat import OpenAICompatProvider

DEEPSEEK_BASE_URL = "https://api.deepseek.com/v1"


class DeepSeekProvider(OpenAICompatProvider):
    name = "deepseek"

    def __init__(
        self,
        api_key: str,
        model: str = "deepseek-chat",
        base_url: str = DEEPSEEK_BASE_URL,
    ) -> None:
        super().__init__(base_url=base_url, api_key=api_key, model=model)
