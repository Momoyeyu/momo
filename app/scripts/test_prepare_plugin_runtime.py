import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('prepare', Path(__file__).with_name('prepare-plugin-runtime.py'))
prepare = importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)


class CoreResources(unittest.TestCase):
    def test_only_core_host_is_staged(self):
        with tempfile.TemporaryDirectory() as temp:
            host = Path(temp) / 'host'
            prepare.stage_core_host(prepare.ROOT / 'plugin-host', host)
            self.assertTrue((host / 'src/index.mjs').is_file())
            self.assertTrue((host / 'resources/native-control/control.swift').is_file())
            self.assertFalse(any('minecraft' in p.name or 'qq-' in p.name for p in host.rglob('*')))
