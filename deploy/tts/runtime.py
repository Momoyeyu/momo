"""Load only a verified upstream v2 inference pipeline, never its HTTP APP."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys

UPSTREAM_REVISION = '1d9fe63d2875238bc240bd5e7b8dacaee73f8396'
UPSTREAM_API_SHA256 = '7c1300f5cba7173f59825045c80bb8fb3d22b7cde94b26529128a03241bf7339'


def verify_source(root: Path):
    path = root / 'api_v2.py'
    if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != UPSTREAM_API_SHA256:
        raise ValueError('upstream api_v2.py differs from the pinned, unpatched revision')
    revision = root / '.momo-upstream-revision'
    if not revision.is_file() or revision.read_text().strip() != UPSTREAM_REVISION:
        raise ValueError('upstream source revision marker is missing or mismatched')


def prepare_config(models: Path, cache: Path) -> Path:
    cache.mkdir(parents=True, exist_ok=True)
    pretrained = models / 'pretrained_models'
    value = {'custom': {
        'device': 'cuda', 'is_half': True, 'version': 'v2',
        'bert_base_path': str(pretrained / 'chinese-roberta-wwm-ext-large'),
        'cnhuhbert_base_path': str(pretrained / 'chinese-hubert-base'),
        't2s_weights_path': str(pretrained / 'gsv-v2final-pretrained/s1bert25hz-5kh-longer-epoch=12-step=369668.ckpt'),
        'vits_weights_path': str(pretrained / 'gsv-v2final-pretrained/s2G2333k.pth'),
    }}
    # JSON is a YAML subset. Upstream may rewrite this private writable copy.
    path = cache / 'tts_infer.yaml'
    path.write_text(json.dumps(value, indent=2))
    return path


def load_upstream():
    from resources import verify_resources
    root = Path('/opt/GPT-SoVITS')
    models = Path('/models')
    cache = Path('/cache')
    verify_source(root)
    verify_resources(models)
    import torch
    if not torch.cuda.is_available():
        raise RuntimeError('CUDA is unavailable; check NVIDIA driver, container toolkit and Compose GPU reservation')
    os.environ['HF_HUB_OFFLINE'] = '1'
    os.environ['TRANSFORMERS_OFFLINE'] = '1'
    os.environ['NLTK_DATA'] = str(models / 'nltk_data')
    os.environ['OPEN_JTALK_DICT_DIR'] = str(models / 'openjtalk/open_jtalk_dic_utf_8-1.11')
    os.environ['HF_HOME'] = str(cache / 'huggingface')
    os.environ['bert_path'] = str(models / 'pretrained_models/chinese-roberta-wwm-ext-large')
    import nltk
    nltk.data.path.insert(0, str(models / 'nltk_data'))
    config = prepare_config(models, cache)
    os.chdir(root)
    sys.path[:0] = [str(root), str(root / 'GPT_SoVITS')]
    previous = sys.argv
    try:
        sys.argv = ['api_v2.py', '-c', str(config)]
        specification = importlib.util.spec_from_file_location('momo_pinned_sovits', root / 'api_v2.py')
        module = importlib.util.module_from_spec(specification)
        specification.loader.exec_module(module)
    finally:
        sys.argv = previous
    # APP contains upstream administrative routes but is never mounted or served.
    # Its handler owns the inference lock, dispatched threads and stream generator.
    return module.tts_handle
