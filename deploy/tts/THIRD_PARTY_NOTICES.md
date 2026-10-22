# 上游与资源来源

- 推理代码：[RVC-Boss/GPT-SoVITS](https://github.com/RVC-Boss/GPT-SoVITS/tree/1d9fe63d2875238bc240bd5e7b8dacaee73f8396)，固定提交，保留仓库 LICENSE 与各子目录许可。HTTP 包装层不应用原运行机器上的 base64 临时文件补丁。
- 默认 GPT、SoVITS v2、中文 BERT 与 HuBERT 权重：[lj1995/GPT-SoVITS](https://huggingface.co/lj1995/GPT-SoVITS/tree/8f8857563c040d542de108a2c18b0425abd6c9c5)。G2PW：[XXXXRT/GPT-SoVITS-Pretrained](https://huggingface.co/XXXXRT/GPT-SoVITS-Pretrained/tree/0c47645e02a7bc3688d7b263b0042c81e3cd82cd)。这些权重由显式准备命令从上游下载，适用各自来源的条款，不将代码仓库许可推定为所有模型的许可。
- 英语语言资源：[NLTK data](https://github.com/nltk/nltk_data/tree/550b6625bcef1f2abff2ff770a5a0d272c9c6b2a)，压缩包中的许可与说明保留。
- 日语词典：[Open JTalk 1.11](https://github.com/r9y9/open_jtalk/releases/tag/v1.11.1)，原始词典包保留其 COPYING 等文件。
- 语言识别：[fastText lid.176](https://fasttext.cc/docs/en/language-identification.html)，模型按 CC BY-SA 3.0 提供。
- Python、PyTorch/CUDA 与其余 Python 包的许可随各软件包保留。完整下载地址、版本和 SHA-256 在 `resources.lock.json`、`requirements-*.lock`。

本部署不提供或包含用户参考音频、私人微调权重或用户配置。

构建基底固定为 Debian bookworm-slim 的 linux/amd64 manifest `sha256:a4672c0cb26fbdde88e38fa2dfb6c681942306680e41e4378b28770b6e79ee91`，来源为 Docker 官方维护的 [repo-info](https://github.com/docker-library/repo-info/blob/master/repos/debian/remote/bookworm-slim.md)（2026-10-22 核对）。
