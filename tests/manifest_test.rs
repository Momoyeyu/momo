//! MANIFEST.toml 解析/校验单元测试（口径：momo-models.md `name/repo_id/revision/files/sha256/dest/notes`）。

use momo_infra::manifest::Manifest;

const SAMPLE: &str = r#"
[[model]]
name = "kokoro-82m"
repo_id = "hexgrad/Kokoro-82M"
revision = "main"
notes = "兜底 TTS，实测 209ms/0.76GB"

[[model.files]]
path = "kokoro-v1_0.pth"
sha256 = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
size = 330000000

[[model.files]]
path = "voices.bin"

[[model]]
name = "sensevoice-small"
repo_id = "iic/SenseVoiceSmall"
dest = "sherpa-onnx/sense-voice-zh-en"

[[model.files]]
path = "model.int8.onnx"
"#;

#[test]
fn parses_entries() {
    let m = Manifest::parse(SAMPLE).unwrap();
    assert_eq!(m.models.len(), 2);

    let k = &m.models[0];
    assert_eq!(k.name, "kokoro-82m");
    assert_eq!(k.repo_id, "hexgrad/Kokoro-82M");
    assert_eq!(k.revision, "main");
    // dest 缺省 = repo_id（镜像 <owner>/<repo> 布局）
    assert_eq!(k.dest().to_str().unwrap(), "hexgrad/Kokoro-82M");
    assert_eq!(k.files.len(), 2);
    assert_eq!(k.files[0].size, Some(330000000));

    let s = &m.models[1];
    assert_eq!(s.revision, "main"); // 默认 revision
    assert_eq!(s.dest().to_str().unwrap(), "sherpa-onnx/sense-voice-zh-en");
}

#[test]
fn rejects_bad_sha256() {
    let bad = r#"
[[model]]
name = "x"
repo_id = "a/b"
[[model.files]]
path = "f.bin"
sha256 = "not-hex"
"#;
    assert!(Manifest::parse(bad).is_err());
}

#[test]
fn rejects_empty_repo_id() {
    let bad = r#"
[[model]]
name = "x"
repo_id = ""
"#;
    assert!(Manifest::parse(bad).is_err());
}

#[test]
fn rejects_dest_traversal() {
    let bad = r#"
[[model]]
name = "x"
repo_id = "a/b"
dest = "../escape"
"#;
    assert!(Manifest::parse(bad).is_err());
}

#[test]
fn tolerates_empty_manifest() {
    let m = Manifest::parse("").unwrap();
    assert!(m.models.is_empty());
}
