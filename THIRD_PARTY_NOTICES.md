# Third-party resources and notices

The [MIT license](LICENSE.md) covers momo's original code. It does not replace the licenses of third-party software, character assets, fonts, or model weights. Keep the applicable copyright notices and license texts when redistributing these components.

The source repository includes the default Hiyori assets and local fonts. Cubism Core and large speech / embedding model binaries are prepared on the build machine from pinned sources; they are included in the initial application installation. Players do not download these models on first launch.

| Component | Source and applicable notices |
| --- | --- |
| Hiyori sample character, textures and motions | © Live2D Inc. See the [model license](app/frontend/public/live2d/hiyori/LICENSE.md) and [copyright notice](app/frontend/public/live2d/hiyori/NOTICE.txt). A model's license is independent of momo's code license. |
| Live2D Cubism Core | [Pinned official source, digest and license URL](app/migration/cubism-core.json). Core uses the Live2D Proprietary Software License; it is prepared as build input and distributed under its own terms, not relicensed as MIT. |
| Noto Sans SC, Noto Sans JP and Nunito | SIL Open Font License. [Font provenance and manifests](app/frontend/public/fonts/README.md); [SC license](app/frontend/public/fonts/notosanssc-OFL.txt), [JP license](app/frontend/public/fonts/notosansjp-OFL.txt), [Nunito license](app/frontend/public/fonts/nunito-OFL.txt). Website copies preserve the same notices. |
| SenseVoice-Small and Silero VAD | [Pinned speech-resource provenance](app/backend/resources/asr/PROVENANCE.md) and [manifest](app/backend/resources/asr/manifest.json). SenseVoice model weights use the [FunASR Model Open Source License Agreement](app/backend/resources/asr/licenses/FunASR-MODEL-LICENSE); its MIT source-code notice does not replace the model license. Silero retains its [MIT notice](app/backend/resources/asr/licenses/Silero-VAD-LICENSE). |
| Multilingual E5 small | [Model license](app/backend/resources/embedding/multilingual-e5-small/LICENSE), [upstream model card](app/backend/resources/embedding/multilingual-e5-small/UPSTREAM_README.md), and [pinned manifest](app/backend/resources/embedding/multilingual-e5-small/manifest.json). |
| Optional GPT-SoVITS deployment | The separately deployed service has its own upstream code, pretrained models, dictionaries and dependencies. See [TTS notices](deploy/tts/THIRD_PARTY_NOTICES.md) and its [resource lock](deploy/tts/resources.lock.json). It is not bundled into the desktop application. |
| Other software dependencies | Python, Rust and JavaScript dependencies retain their individual licenses. Lockfiles identify versions; they do not grant a replacement license for those packages. |

The room background is project-created AI-generated artwork; its provenance is recorded in [stage assets](app/frontend/public/stage/README.md). momo's abstract mark and wordmark are original artwork; see [brand sources](brand/README.md). README and website demonstrations use isolated, synthetic conversations; see the [demonstration provenance](site/docs/public/media/asset-notice.txt) and accompanying Hiyori notices.

No default character reference recording, private voice sample, personal configuration or user memory is provided. Users supply reference audio they have the right to use; synthesis sends that reference to the voice service they configure.
