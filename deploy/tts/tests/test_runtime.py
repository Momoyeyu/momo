import json
from pathlib import Path
import pytest
from runtime import prepare_config, verify_source


def test_modified_upstream_is_rejected_before_inference_import(tmp_path):
    (tmp_path/'api_v2.py').write_text('print("modified")')
    with pytest.raises(ValueError,match='upstream'):
        verify_source(tmp_path)


def test_model_configuration_is_explicit_v2_cuda_and_writable_copy(tmp_path):
    models=tmp_path/'models';cache=tmp_path/'cache'
    path=prepare_config(models,cache)
    config=json.loads(path.read_text())['custom']
    assert config['device']=='cuda' and config['version']=='v2' and config['is_half'] is True
    assert Path(config['t2s_weights_path']).is_relative_to(models)
    assert Path(config['vits_weights_path']).is_relative_to(models)
    assert path.is_relative_to(cache)
