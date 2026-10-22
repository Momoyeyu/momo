ARG BASE_IMAGE=debian:bookworm-slim@sha256:a4672c0cb26fbdde88e38fa2dfb6c681942306680e41e4378b28770b6e79ee91
FROM ${BASE_IMAGE} AS http-runtime
USER root
ENTRYPOINT []
ARG PIP_INDEX_URL=https://pypi.org/simple
ENV PYTHONUNBUFFERED=1 PYTHONDONTWRITEBYTECODE=1 PATH=/venv/bin:$PATH
RUN apt-get update && apt-get install -y --no-install-recommends python3 python3-venv ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && python3 -m venv /venv
WORKDIR /opt/momo
COPY requirements-http.lock ./
RUN --mount=type=cache,target=/root/.cache/pip pip install --index-url "$PIP_INDEX_URL" --require-hashes -r requirements-http.lock

FROM http-runtime AS contract-test
COPY requirements-test.lock ./
RUN --mount=type=cache,target=/root/.cache/pip pip install --index-url "$PIP_INDEX_URL" --require-hashes -r requirements-test.lock
COPY server.py runtime.py resources.py check.py ./
COPY pytest.ini ./
COPY tests ./tests
RUN python -m pytest -q

FROM http-runtime AS inference-build
ARG PIP_INDEX_URL=https://pypi.org/simple
RUN apt-get update && apt-get install -y --no-install-recommends \
    git ffmpeg libsndfile1 libsox-dev libmecab-dev build-essential cmake \
    && rm -rf /var/lib/apt/lists/*
RUN apt-get update && apt-get install -y --no-install-recommends python3-dev \
    && rm -rf /var/lib/apt/lists/*
COPY requirements-build.lock requirements-inference.lock ./
RUN --mount=type=cache,target=/root/.cache/pip pip install --index-url "$PIP_INDEX_URL" --require-hashes -r requirements-build.lock \
    && pip install --index-url "$PIP_INDEX_URL" --require-hashes --no-build-isolation -r requirements-inference.lock \
    && pip check
RUN git init /opt/GPT-SoVITS \
    && git -C /opt/GPT-SoVITS remote add origin https://github.com/RVC-Boss/GPT-SoVITS.git \
    && git -C /opt/GPT-SoVITS -c http.version=HTTP/1.1 -c http.lowSpeedLimit=1024 -c http.lowSpeedTime=60 fetch --depth 1 origin 1d9fe63d2875238bc240bd5e7b8dacaee73f8396 \
    && git -C /opt/GPT-SoVITS checkout --detach FETCH_HEAD \
    && test "$(git -C /opt/GPT-SoVITS rev-parse HEAD)" = 1d9fe63d2875238bc240bd5e7b8dacaee73f8396 \
    && git -C /opt/GPT-SoVITS submodule update --init --recursive \
    && echo 1d9fe63d2875238bc240bd5e7b8dacaee73f8396 > /opt/GPT-SoVITS/.momo-upstream-revision \
    && echo '7c1300f5cba7173f59825045c80bb8fb3d22b7cde94b26529128a03241bf7339  /opt/GPT-SoVITS/api_v2.py' | sha256sum -c - \
    && rm -rf /opt/GPT-SoVITS/.git \
    && rm -rf /opt/GPT-SoVITS/GPT_SoVITS/pretrained_models \
    && ln -s /models/pretrained_models /opt/GPT-SoVITS/GPT_SoVITS/pretrained_models \
    && ln -s /models/G2PWModel /opt/GPT-SoVITS/GPT_SoVITS/text/G2PWModel
# No GPU or network is needed to validate all inference-module imports.
RUN --network=none cd /opt/GPT-SoVITS \
    && PYTHONPATH=/opt/GPT-SoVITS:/opt/GPT-SoVITS/GPT_SoVITS HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1 \
    python -c "import torch; import GPT_SoVITS.TTS_infer_pack.TTS; print('Inference imports passed; CUDA wheel', torch.version.cuda)"

FROM http-runtime AS inference-core
RUN apt-get update && apt-get install -y --no-install-recommends ffmpeg libsndfile1 libsox3 libmecab2 \
    && rm -rf /var/lib/apt/lists/*
COPY --from=inference-build /venv /venv
COPY --from=inference-build /opt/GPT-SoVITS /opt/GPT-SoVITS
COPY server.py runtime.py resources.py check.py ./
COPY resources.lock.json ./
COPY tests/smoke_inference.py ./smoke_inference.py
RUN useradd --create-home --uid 10001 momo \
    && mkdir -p /models /cache /tmp/momo-tts \
    && chown -R momo:momo /opt/GPT-SoVITS /models /cache /tmp/momo-tts
ENV HF_HUB_OFFLINE=1 TRANSFORMERS_OFFLINE=1 HF_HOME=/cache/huggingface \
    NLTK_DATA=/models/nltk_data OPEN_JTALK_DICT_DIR=/models/openjtalk/open_jtalk_dic_utf_8-1.11
USER momo
EXPOSE 9880
HEALTHCHECK --interval=30s --timeout=5s --start-period=180s --retries=3 \
    CMD python -c "import os,urllib.request; r=urllib.request.Request('http://127.0.0.1:9880/readyz'); k=os.environ.get('TTS_API_KEY'); r.add_header('Authorization','Bearer '+k) if k else None; urllib.request.urlopen(r,timeout=4)"
CMD ["uvicorn", "server:production_app", "--factory", "--host", "0.0.0.0", "--port", "9880", "--workers", "1", "--limit-concurrency", "8", "--timeout-graceful-shutdown", "90", "--no-access-log"]

# Keep API import/decorator dependencies separate from the large CUDA layer.
FROM inference-core AS inference
USER root
ARG PIP_INDEX_URL=https://pypi.org/simple
COPY requirements-upstream-api.lock ./
RUN --mount=type=cache,target=/root/.cache/pip pip install --index-url "$PIP_INDEX_URL" --require-hashes -r requirements-upstream-api.lock && pip check
COPY tests/smoke_api_module.py ./smoke_api_module.py
RUN --network=none python /opt/momo/smoke_api_module.py
USER momo
