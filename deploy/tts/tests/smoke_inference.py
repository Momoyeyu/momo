"""Run inside the built inference image after explicit resource setup; no GPU needed.

Mount /models read-only and use --network none. This exercises real language
modules and rejects hidden NLTK downloads; it does not instantiate a TTS model.
"""
import os
from pathlib import Path
import sys

root = Path('/opt/GPT-SoVITS')
os.chdir(root)
sys.path[:0] = [str(root), str(root / 'GPT_SoVITS')]
os.environ.update(HF_HUB_OFFLINE='1', TRANSFORMERS_OFFLINE='1',
                  NLTK_DATA='/models/nltk_data',
                  OPEN_JTALK_DICT_DIR='/models/openjtalk/open_jtalk_dic_utf_8-1.11')

import torch
import nltk

torch.set_num_threads(2)
nltk.data.path.insert(0, '/models/nltk_data')

def reject_download(*args, **kwargs):
    raise RuntimeError('Inference attempted an implicit NLTK download')

nltk.download = reject_download
import GPT_SoVITS.TTS_infer_pack.TTS
from text.cleaner import clean_text
from text.LangSegmenter import LangSegmenter

for text, language in [('你好，很高兴认识你。', 'zh'),
                       ('Hello, it is nice to meet you.', 'en'),
                       ('こんにちは、はじめまして。', 'ja')]:
    phones, _, _ = clean_text(text, language, 'v2')
    assert phones, f'No phones generated for {language}'
    print(f'{language}: {len(phones)} phones')
segments = LangSegmenter.getTexts('你好，hello.')
assert {item['lang'] for item in segments} == {'zh', 'en'}, segments
print(f'Offline inference imports and language resources passed; CUDA wheel {torch.version.cuda}; GPU synthesis not exercised')
