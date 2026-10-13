//! persona card → system prompt（docs/04-tracks/persona.md）。
//!
//! `Persona` 持 card 路径，每回合 `system_prompt()` 重读文件渲染——
//! 改 yaml 无需重启即下一回合生效（hot reload）。文件缺失/损坏回退
//! 到编译期内嵌 card，保证人格永远在场。

use std::path::PathBuf;

use serde::Deserialize;
use tracing::warn;

/// 编译期内嵌的默认 card（`persona/momo.yaml` 入库）。
const EMBEDDED_CARD: &str = include_str!("../persona/momo.yaml");

/// persona 数据源：`MOMO_PERSONA_FILE` 指向的 card，读不到/解析失败时用内嵌 card。
#[derive(Debug, Clone)]
pub struct Persona {
    file: PathBuf,
}

impl Persona {
    pub fn new(file: PathBuf) -> Self {
        Self { file }
    }

    /// 每回合调用：读 card → 渲染 system prompt。文件读失败/解析失败
    /// 回退内嵌 card（warn 一次，不阻断回合）。
    pub fn system_prompt(&self) -> String {
        let source = match std::fs::read_to_string(&self.file) {
            Ok(text) => text,
            Err(err) => {
                warn!(
                    path = %self.file.display(),
                    %err,
                    "persona card unreadable, using embedded default"
                );
                EMBEDDED_CARD.to_string()
            }
        };
        match serde_yaml::from_str::<PersonaCard>(&source) {
            Ok(card) => card.render(),
            Err(err) => {
                warn!(%err, "persona card parse failed, using embedded default");
                match serde_yaml::from_str::<PersonaCard>(EMBEDDED_CARD) {
                    Ok(card) => card.render(),
                    Err(e) => {
                        warn!(%e, "embedded persona card invalid");
                        String::new()
                    }
                }
            }
        }
    }
}

/// card schema v1（docs/04-tracks/persona.md）；字段全部可选，
/// 部分填写的 card 也能渲染（空字段跳过）。
#[derive(Debug, Default, Deserialize)]
pub struct PersonaCard {
    pub name: Option<String>,
    pub one_liner: Option<String>,
    pub identity: Option<Identity>,
    pub personality: Option<Personality>,
    pub speech_style: Option<SpeechStyle>,
    pub emotion: Option<Emotion>,
    pub behavior: Option<Behavior>,
    #[serde(default)]
    pub dialogue_examples: Vec<DialogueExample>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Identity {
    pub species: Option<String>,
    #[serde(default)]
    pub likes: Vec<String>,
    #[serde(default)]
    pub dislikes: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Personality {
    #[serde(default)]
    pub traits: Vec<String>,
    pub tone: Option<String>,
    #[serde(default)]
    pub taboos: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct SpeechStyle {
    #[serde(default)]
    pub language: Vec<String>,
    pub code_switch: Option<String>,
    pub sentence_habit: Option<String>,
    pub honorifics: Option<String>,
    #[serde(default)]
    pub examples: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Emotion {
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub tag_guide: std::collections::BTreeMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct Behavior {
    pub on_interrupt: Option<String>,
    pub on_idle: Option<String>,
    pub on_unknown: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub struct DialogueExample {
    pub user: String,
    pub assistant: String,
}

impl PersonaCard {
    /// 渲染成 system prompt（token 预算 ≤800，先砍示例再砍记忆——docs 约束）。
    pub fn render(&self) -> String {
        let name = self.name.as_deref().unwrap_or("momo");
        let mut out = String::with_capacity(1600);
        match &self.one_liner {
            Some(line) => {
                out.push_str(&format!("你是 {name}，{line}。"));
            }
            None => out.push_str(&format!("你是 {name}。")),
        }
        if let Some(id) = &self.identity {
            let mut sec = String::from("\n\n[身份]");
            if let Some(s) = &id.species {
                sec.push_str(&format!("\n- 设定：{s}"));
            }
            push_list(&mut sec, "喜欢", &id.likes);
            push_list(&mut sec, "不喜欢", &id.dislikes);
            out.push_str(&sec);
        }
        if let Some(p) = &self.personality {
            let mut sec = String::from("\n\n[性格]");
            for t in &p.traits {
                sec.push_str(&format!("\n- {t}"));
            }
            if let Some(t) = &p.tone {
                sec.push_str(&format!("\n语气基调：{t}"));
            }
            push_list(&mut sec, "绝不能", &p.taboos);
            out.push_str(&sec);
        }
        if let Some(s) = &self.speech_style {
            let mut sec = String::from("\n\n[说话方式]");
            if !s.language.is_empty() {
                sec.push_str(&format!("\n- 语言：{}", s.language.join("/")));
            }
            if let Some(v) = &s.code_switch {
                sec.push_str(&format!("\n- 混语：{v}"));
            }
            if let Some(v) = &s.sentence_habit {
                sec.push_str(&format!("\n- 句式：{v}"));
            }
            if let Some(v) = &s.honorifics {
                sec.push_str(&format!("\n- 称呼用户：{v}"));
            }
            for e in s.examples.iter().take(3) {
                sec.push_str(&format!("\n- 例：{e}"));
            }
            out.push_str(&sec);
        }
        if let Some(b) = &self.behavior {
            let mut sec = String::from("\n\n[行为约定]");
            if let Some(v) = &b.on_interrupt {
                sec.push_str(&format!("\n- 被打断：{v}"));
            }
            if let Some(v) = &b.on_unknown {
                sec.push_str(&format!("\n- 不会的问题：{v}"));
            }
            if let Some(v) = &b.on_idle {
                sec.push_str(&format!("\n- 空闲时：{v}"));
            }
            out.push_str(&sec);
        }
        // emotion tag：与 splitter `[tag]` 前缀解析对齐；tag 集合以 card 为准。
        if let Some(e) = &self.emotion {
            if !e.tags.is_empty() {
                let mut sec = format!(
                    "\n\n[输出规则]\n- 回复拆成短句，每句必须以情感 tag 开头，格式 `[tag]`，tag 只能取：{}",
                    e.tags.join(", ")
                );
                for (tag, guide) in &e.tag_guide {
                    sec.push_str(&format!("\n  - {tag}：{guide}"));
                }
                out.push_str(&sec);
            }
        }
        for ex in self.dialogue_examples.iter().take(3) {
            out.push_str(&format!(
                "\n\n[示例]\nuser: {}\n{name}: {}",
                ex.user, ex.assistant
            ));
        }
        out.push_str(&format!(
            "\n\n始终用 {name} 的人格说话；绝不自称 AI 助手、语言模型或任何产品/模型名。"
        ));
        out
    }
}

fn push_list(sec: &mut String, label: &str, items: &[String]) {
    if !items.is_empty() {
        sec.push_str(&format!("\n- {label}：{}", items.join("；")));
    }
}
