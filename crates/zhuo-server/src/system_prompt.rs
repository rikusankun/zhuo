//! 系统提示词生成：角色核心人设、写作规则、用户画像。

use zhuo_card::Card;

pub const MAX_SYSTEM_CHARS: usize = 500;

const WRITING_RULES: &str = "[写作规则]\n只描写你自己（角色）的语言、动作、心理与环境。\n严禁代替用户说话、行动或做决定；写到该由用户反应的地方就停下，把选择权交回用户。\n输出中不要使用 {{user}}、{{char}} 等占位符，直接写名字。";

pub fn system_prompt(card: &Card, _world: &str) -> String {
    minimal_system_prompt(card, None)
}

pub fn minimal_system_prompt(card: &Card, persona: Option<(&str, &str)>) -> String {
    let mut parts: Vec<String> = Vec::new();
    parts.push(WRITING_RULES.to_string());

    parts.push(format!(
        "扮演「{}」，保持角色一致并自然回应。",
        card.name.trim()
    ));

    let core = [
        card.system_prompt.trim(),
        card.post_history_instructions.trim(),
    ]
    .into_iter()
    .filter(|text| !text.is_empty())
    .collect::<Vec<_>>()
    .join(" ");
    if !core.is_empty() {
        parts.push(format!(
            "核心设定：{}",
            core.chars().take(120).collect::<String>()
        ));
    }

    let traits = [card.description.trim(), card.personality.trim()]
        .into_iter()
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("；");
    if !traits.is_empty() {
        parts.push(format!(
            "外貌与性格：{}",
            traits.chars().take(180).collect::<String>()
        ));
    }

    if let Some((name, description)) = persona {
        if !description.trim().is_empty() {
            let persona_text = if name.trim().is_empty() {
                description.trim().to_string()
            } else {
                format!("用户{}：{}", name.trim(), description.trim())
            };
            parts.push(format!(
                "用户画像：{}",
                persona_text.chars().take(120).collect::<String>()
            ));
        }
    }

    parts.join("\n")
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
        assert!(
            system.starts_with("[写作规则]"),
            "写作规则必须前置到 system 最开头"
        );
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
}
