"""Import/register the exact pinned upstream API without constructing GPU models.

Only the single `tts_pipeline = TTS(...)` assignment is replaced in memory.
The source remains unchanged and all imports, config parsing and route decorators
execute, catching dependencies that a TTS-class-only import cannot cover.
"""
import ast
import os
from pathlib import Path
import sys

from runtime import verify_source

root = Path('/opt/GPT-SoVITS')
verify_source(root)
os.chdir(root)
sys.path[:0] = [str(root), str(root / 'GPT_SoVITS')]
sys.argv = ['api_v2.py', '-c', str(root / 'GPT_SoVITS/configs/tts_infer.yaml')]
path = root / 'api_v2.py'
tree = ast.parse(path.read_text(), filename=str(path))
replaced = 0
for node in tree.body:
    if (isinstance(node, ast.Assign)
            and any(isinstance(target, ast.Name) and target.id == 'tts_pipeline' for target in node.targets)
            and isinstance(node.value, ast.Call)
            and isinstance(node.value.func, ast.Name) and node.value.func.id == 'TTS'):
        node.value = ast.copy_location(ast.Constant(None), node.value)
        replaced += 1
assert replaced == 1, 'Pinned upstream model-construction assignment changed'
namespace = {'__name__': 'momo_upstream_api_smoke', '__file__': str(path)}
exec(compile(ast.fix_missing_locations(tree), str(path), 'exec'), namespace)
assert namespace['tts_pipeline'] is None
assert callable(namespace['tts_handle'])
assert '/tts' in {route.path for route in namespace['APP'].routes}
from GPT_SoVITS.TTS_infer_pack.TTS import _resampy_gpu_filter
import torch
window, delta, precision = _resampy_gpu_filter(32000, 16000, torch.device('cpu'))
assert window.numel() > 0 and delta.numel() > 0 and precision > 0
assert torch.isfinite(window).all()
print('Pinned upstream API imports, config, route registration and real resampling filter passed; GPU model construction not exercised')
