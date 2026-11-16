//! 紧凑 Prompt 组装：角色核心信息、检索片段、角色状态和最近历史均有硬长度限制。

use serde_json::Value;
use zhuo_card::Card;
use zhuo_llm::{estimate_tokens, Msg};
use zhuo_memory::PromptHit;

pub const MAX_SYSTEM_CHARS: usize = 200;
pub const MAX_RETRIEVAL_CHARS: usize = 500;
pub const MAX_PROMPT_CHARS: usize = 1500;
pub const MAX_HISTORY_MESSAGES: usize = 6;

pub fn system_prompt(card: &Card, _world: &str) -> String {
    minimal_system_prompt(card, None)
}

pub fn minimal_system_prompt(card: &Card, persona: Option<(&str, &str)>) -> String {
    // 硬约束：禁止模型代替用户说话（最高优先级，必须完整保留）
    const WRITING_RULES: &str = "\n[写作规则]\n只描写你自己（角色）的语言、动作、心理与环境。\n严禁代替用户说话、行动或做决定；写到该由用户反应的地方就停下，把选择权交回用户。\n输出中不要使用 {{user}}、{{char}} 等占位符，直接写名字。";

    let mut parts = vec![format!(
        "扮演「{}」，保持角色一致并自然回应。",
        card.name.trim()
    )];
    let core = [
        card.system_prompt.trim(),
        card.post_history_instructions.trim(),
    ]
    .into_iter()
    .filter(|text| !text.is_empty())
    .collect::<Vec<_>>()
    .join(" ");
    if !core.is_empty() {
        parts.push(core.chars().take(35).collect());
    }
    let traits = [card.description.trim(), card.personality.trim()]
        .into_iter()
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("；");
    if !traits.is_empty() {
        parts.push(format!(
            "外貌与性格：{}",
            traits.chars().take(100).collect::<String>()
        ));
    }
    if let Some((name, description)) = persona {
        if !description.trim().is_empty() {
            let persona_text = if name.trim().is_empty() {
                description.trim().to_string()
            } else {
                format!("用户{}：{}", name.trim(), description.trim())
            };
            parts.push(persona_text.chars().take(35).collect());
        }
    }
    let mut system = String::new();
    for part in parts {
        let separator = if system.is_empty() { "" } else { "\n" };
        let available =
            MAX_SYSTEM_CHARS.saturating_sub(system.chars().count() + separator.chars().count());
        if available == 0 {
            break;
        }
        system.push_str(separator);
        system.extend(part.chars().take(available));
    }
    // 硬约束：禁止模型代替用户说话（必须完整保留，不受 MAX_SYSTEM_CHARS 限制）
    if !system.contains("严禁代替用户") {
        system.push_str(WRITING_RULES);
    }
    system
}

/// 世界设定 JSON 全量拍平：递归收集所有字符串值。
pub fn world_text(v: &Value) -> String {
    let mut parts = Vec::new();
    collect(v, &mut parts);
    parts.join("\n\n")
}

fn collect(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => {
            if !s.trim().is_empty() {
                out.push(s.trim().to_string());
            }
        }
        Value::Array(items) => {
            for item in items {
                collect(item, out);
            }
        }
        Value::Object(map) => {
            for (key, val) in map {
                match val {
                    Value::String(s) if !s.trim().is_empty() => {
                        out.push(format!("[{}]\n{}", key, s.trim()));
                    }
                    Value::String(_) => {}
                    _ => collect(val, out),
                }
            }
        }
        _ => {}
    }
}

pub fn retrieval_block(hits: &[PromptHit]) -> Option<String> {
    if hits.is_empty() {
        return None;
    }
    let mut block = String::from("[检索到的设定与记忆]\n");
    for hit in hits {
        let title: String = hit.title.chars().take(40).collect();
        let prefix = format!("- [{}] {}：", hit.kind, title);
        let remaining = MAX_RETRIEVAL_CHARS.saturating_sub(block.chars().count());
        if remaining <= prefix.chars().count() {
            break;
        }
        block.push_str(&prefix);
        let available = MAX_RETRIEVAL_CHARS.saturating_sub(block.chars().count() + 1);
        block.extend(hit.content.chars().take(available));
        block.push('\n');
    }
    (block.chars().count() > "[检索到的设定与记忆]\n".chars().count()).then_some(block)
}

/// Assemble only compact system context, retrieved facts, current state, six messages, and input.
pub fn assemble_rag_messages(
    system: &str,
    retrieval: Option<&str>,
    state: Option<&str>,
    history: &[Msg],
    input: &str,
) -> Vec<Msg> {
    let system = truncate(system, MAX_SYSTEM_CHARS);
    let mut retrieval = retrieval
        .filter(|text| !text.trim().is_empty())
        .map(|text| truncate(text, MAX_RETRIEVAL_CHARS));
    let mut state = state
        .filter(|text| !text.trim().is_empty())
        .map(str::to_string);
    let mut history = history
        .iter()
        .filter(|message| message.role == "user" || message.role == "assistant")
        .rev()
        .take(MAX_HISTORY_MESSAGES)
        .cloned()
        .collect::<Vec<_>>();
    history.reverse();
    let mut input = input.to_string();

    while prompt_chars_parts(
        &system,
        retrieval.as_deref(),
        state.as_deref(),
        &history,
        &input,
    ) > MAX_PROMPT_CHARS
        && !history.is_empty()
    {
        history.remove(0);
    }
    trim_optional(&mut state, &system, retrieval.as_deref(), &history, &input);
    trim_optional(&mut retrieval, &system, state.as_deref(), &history, &input);
    if prompt_chars_parts(
        &system,
        retrieval.as_deref(),
        state.as_deref(),
        &history,
        &input,
    ) > MAX_PROMPT_CHARS
    {
        let excess = prompt_chars_parts(
            &system,
            retrieval.as_deref(),
            state.as_deref(),
            &history,
            &input,
        ) - MAX_PROMPT_CHARS;
        input = truncate(&input, input.chars().count().saturating_sub(excess));
    }

    let system = combined_system(&system, retrieval.as_deref(), state.as_deref());
    let mut messages = vec![Msg::system(system)];
    messages.extend(history);
    messages.push(Msg::user(input));
    messages
}

pub fn prompt_chars(messages: &[Msg]) -> usize {
    messages
        .iter()
        .map(|message| message.content.chars().count())
        .sum()
}

pub fn estimate_prompt_tokens(messages: &[Msg]) -> usize {
    estimate_tokens(
        &messages
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn trim_optional(
    value: &mut Option<String>,
    system: &str,
    other: Option<&str>,
    history: &[Msg],
    input: &str,
) {
    let Some(current) = value.as_ref() else {
        return;
    };
    let total = prompt_chars_parts(system, other, Some(current), history, input);
    if total > MAX_PROMPT_CHARS {
        let excess = total - MAX_PROMPT_CHARS;
        *value = Some(truncate(
            current,
            current.chars().count().saturating_sub(excess),
        ));
    }
}

fn prompt_chars_parts(
    system: &str,
    retrieval: Option<&str>,
    state: Option<&str>,
    history: &[Msg],
    input: &str,
) -> usize {
    combined_system(system, retrieval, state).chars().count()
        + history
            .iter()
            .map(|message| message.content.chars().count())
            .sum::<usize>()
        + input.chars().count()
}

fn combined_system(system: &str, retrieval: Option<&str>, state: Option<&str>) -> String {
    let mut combined = system.to_string();
    for block in [retrieval, state]
        .into_iter()
        .flatten()
        .filter(|text| !text.trim().is_empty())
    {
        if !combined.is_empty() {
            combined.push_str("\n\n");
        }
        combined.push_str(block);
    }
    combined
}

fn truncate(text: &str, max_chars: usize) -> String {
    text.chars().take(max_chars).collect()
}

/// 渲染 [当前状态] 块（蓝图 5.2）：顶层键值各占一行，数组用「、」连接；
/// 空对象/无有效字段返回 None（prompt 不被空状态污染）。嵌套对象暂不渲染。
pub fn render_state(v: &Value) -> Option<String> {
    let obj = v.as_object()?;
    let mut lines: Vec<String> = Vec::new();
    for (k, val) in obj {
        match val {
            Value::String(s) if !s.trim().is_empty() => {
                lines.push(format!("{k}：{}", s.trim()));
            }
            Value::Array(a) if !a.is_empty() => {
                let items = a
                    .iter()
                    .map(|x| match x {
                        Value::String(s) => s.trim().to_string(),
                        other => other.to_string(),
                    })
                    .collect::<Vec<_>>()
                    .join("、");
                lines.push(format!("{k}：{items}"));
            }
            Value::Number(n) => lines.push(format!("{k}：{n}")),
            Value::Bool(b) => lines.push(format!("{k}：{b}")),
            _ => {}
        }
    }
    if lines.is_empty() {
        return None;
    }
    Some(format!("[当前状态]\n{}", lines.join("\n")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card() -> Card {
        Card {
            id: "deadbeef".into(),
            name: "苏菲".into(),
            description: "冬堡学院的学生".into(),
            first_mes: "要来串糖葫芦吗？".into(),
            mes_example: "<START>".into(),
            ..Default::default()
        }
    }

    #[test]
    fn system_prompt_is_compact_and_keeps_identity() {
        let s = system_prompt(&card(), "冰糖葫芦是冬堡特产");
        assert!(s.contains("冬堡学院的学生"));
        assert!(s.contains("苏菲"));
        assert!(!s.contains("<START>"));
        assert!(!s.contains("冰糖葫芦是冬堡特产"));
        assert!(s.chars().count() <= MAX_SYSTEM_CHARS);
    }

    #[test]
    fn world_text_flattens_nested_json() {
        let v = serde_json::json!({
            "冬堡": "常年下雪",
            "entries": [{"content": "学院在山巅"}, {"content": "糖葫芦很甜"}],
            "count": 3
        });
        let t = world_text(&v);
        assert!(t.contains("常年下雪"));
        assert!(t.contains("学院在山巅"));
        assert!(t.contains("糖葫芦很甜"));
    }

    #[test]
    fn retrieval_block_is_capped_at_five_hundred_characters() {
        let hits = (0..6)
            .map(|i| PromptHit {
                kind: "设定",
                title: format!("条目{i}"),
                content: "长设定".repeat(300),
                score: i as f64,
            })
            .collect::<Vec<_>>();
        let block = retrieval_block(&hits).unwrap();
        assert!(block.chars().count() <= MAX_RETRIEVAL_CHARS);
    }

    #[test]
    fn final_prompt_keeps_only_recent_six_and_never_exceeds_limit() {
        let history = (0..20)
            .map(|i| Msg::user(format!("历史{i} {}", "旧".repeat(130))))
            .collect::<Vec<_>>();
        let messages = assemble_rag_messages(
            &"系统".repeat(150),
            Some(&"检索".repeat(300)),
            Some(&"状态".repeat(300)),
            &history,
            &"当前".repeat(900),
        );
        assert!(prompt_chars(&messages) <= MAX_PROMPT_CHARS);
        assert!(messages[0]
            .content
            .starts_with(&"系统".repeat(MAX_SYSTEM_CHARS / 2)));
        assert_eq!(
            messages
                .iter()
                .filter(|message| message.role == "system")
                .count(),
            1
        );
        assert!(messages.len() <= MAX_HISTORY_MESSAGES + 2);
        assert!(messages
            .iter()
            .any(|message| message.content.starts_with("当前")));
        assert_eq!(messages.last().unwrap().role, "user");
    }

    #[test]
    fn retrieved_context_and_state_share_the_first_system_message() {
        let messages = assemble_rag_messages(
            "base",
            Some("[检索]\n设定"),
            Some("[当前状态]\n位置：冬堡"),
            &[],
            "hi",
        );
        assert_eq!(messages[0].role, "system");
        assert!(messages[0].content.starts_with("base\n\n[检索]"));
        assert!(messages[0].content.contains("[当前状态]\n位置：冬堡"));
        assert_eq!(
            messages
                .iter()
                .filter(|message| message.role == "system")
                .count(),
            1
        );
    }

    #[test]
    fn prompt_uses_only_the_six_most_recent_history_messages() {
        let history = (0..10)
            .map(|index| Msg::user(format!("history-{index}")))
            .collect::<Vec<_>>();
        let messages = assemble_rag_messages("system", None, None, &history, "latest");
        let retained = messages[1..messages.len() - 1]
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            retained,
            [
                "history-4",
                "history-5",
                "history-6",
                "history-7",
                "history-8",
                "history-9"
            ]
        );
    }

    #[test]
    fn stored_system_messages_are_not_replayed_as_chat_history() {
        let history = vec![
            Msg::system("old full system prompt"),
            Msg::user("previous question"),
            Msg::assistant("previous answer"),
        ];
        let messages = assemble_rag_messages("compact system", None, None, &history, "latest");
        assert!(messages
            .iter()
            .all(|message| message.content != "old full system prompt"));
        assert_eq!(
            messages
                .iter()
                .filter(|message| message.role == "system")
                .count(),
            1
        );
    }

    #[test]
    fn render_state_formats_arrays_strings_numbers() {
        let v = serde_json::json!({"背包": ["木剑", "药水"], "位置": "冬堡学院", "好感度": 5});
        let s = render_state(&v).unwrap();
        assert!(s.starts_with("[当前状态]"));
        assert!(s.contains("背包：木剑、药水"));
        assert!(s.contains("位置：冬堡学院"));
        assert!(s.contains("好感度：5"));
        // 空对象 → None
        assert!(render_state(&serde_json::json!({})).is_none());
        // 全空值 → None
        assert!(render_state(&serde_json::json!({"x": "", "y": []})).is_none());
    }

    #[test]
    fn persona_fits_inside_the_basic_system_budget() {
        let system = minimal_system_prompt(
            &card(),
            Some(("Traveler", &"A curious traveler. ".repeat(20))),
        );
        assert!(system.contains("Traveler"));
        assert!(system.chars().count() <= MAX_SYSTEM_CHARS);
    }

    #[test]
    fn minimal_system_prompt_contains_no_user_impersonation_rule() {
        let system = minimal_system_prompt(&card(), None);
        assert!(
            system.contains("严禁代替用户"),
            "system prompt 必须包含禁止代替用户的硬约束规则"
        );
        assert!(
            system.contains("{{user}}"),
            "规则中应保留 {{user}} 字面量作为示例"
        );
        assert!(
            system.contains("{{char}}"),
            "规则中应保留 {{char}} 字面量作为示例"
        );
    }
}
