"""Desktop staging must use the promoted build's exact receipt."""
import hashlib,importlib.util,json,shutil,tempfile,unittest,zipfile
from pathlib import Path
spec=importlib.util.spec_from_file_location('plugin_prepare',Path(__file__).with_name('prepare-plugin-runtime.py'))
prepare=importlib.util.module_from_spec(spec);spec.loader.exec_module(prepare)

class MinecraftStage(unittest.TestCase):
 def fixture(self,folder):
  root=Path(folder);(root/'scripts').mkdir();(root/'migration').mkdir()
  shutil.copy2(Path(__file__).with_name('build-minecraft-component.py'),root/'scripts/build-minecraft-component.py')
  dependency=b'locked nested dependency'
  lock={'baritone':{'file':'baritone.jar','sha256':hashlib.sha256(dependency).hexdigest(),'maxBytes':1000,'minecraft':'1.21.1','version':'1.11.3','source':'https://example.invalid/source','release':'https://example.invalid/release'}}
  (root/'scripts/minecraft-resources.json').write_text(json.dumps(lock))
  (root/'migration/web-source.json').write_text(json.dumps({'revision':'a'*40}))
  artifact=root/'plugin-host/minecraft-fabric/build/libs/momo-fabric-companion-0.1.0.jar';artifact.parent.mkdir(parents=True)
  with zipfile.ZipFile(artifact,'w') as z:
   z.writestr('fabric.mod.json',json.dumps({'id':'momo_companion','jars':[{'file':'META-INF/jars/baritone.jar'}]}))
   z.writestr('META-INF/jars/baritone.jar',dependency)
   z.writestr('META-INF/momo/minecraft-resources.json',json.dumps(lock))
   z.writestr('META-INF/licenses/baritone-LGPL-3.0.txt','test notice')
   z.writestr('META-INF/licenses/baritone-source.txt','https://example.invalid/source https://example.invalid/release')
  import runpy
  builder=runpy.run_path(str(root/'scripts/build-minecraft-component.py'))
  artifact.with_suffix('.provenance.json').write_text(json.dumps(builder['provenance'](artifact,'a'*40)))
  return root,artifact
 def test_exact_promoted_component_and_receipt_are_staged(self):
  with tempfile.TemporaryDirectory() as folder:
   root,artifact=self.fixture(folder);host=root/'resources/plugin-host'
   prepare.stage_minecraft_component(root,host)
   self.assertEqual((host/'resources/minecraft'/artifact.name).read_bytes(),artifact.read_bytes())
   self.assertEqual((host/'resources/minecraft'/artifact.with_suffix('.provenance.json').name).read_bytes(),artifact.with_suffix('.provenance.json').read_bytes())
 def test_stale_build_cannot_enter_desktop_resources(self):
  with tempfile.TemporaryDirectory() as folder:
   root,artifact=self.fixture(folder);host=root/'resources/plugin-host'
   (root/'migration/web-source.json').write_text(json.dumps({'revision':'b'*40}))
   with self.assertRaisesRegex(ValueError,'provenance'):prepare.stage_minecraft_component(root,host)
   self.assertFalse(host.exists())
if __name__=='__main__':unittest.main()
