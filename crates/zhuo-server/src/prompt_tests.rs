use crate::history_formatter::{
    assemble_rag_messages, MAX_HISTORY_MESSAGES, MAX_PROMPT_CHARS,
};
use crate::system_prompt::{
    MAX_SYSTEM_CHARS, minimal_system_prompt,
};
use zhuo_card::Card;
use zhuo_llm::Msg;

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
fn final_prompt_keeps_only_recent_six_and_never_exceeds_limit() {
    let history = (0..20)
        .map(|i| Msg::user(format!("历史{i} {}", "旧".repeat(130))))
        .collect::<Vec<_>>();
    let messages = assemble_rag_messages(
        &"系统".repeat(250),
        Some(&"检索".repeat(300)),
        Some(&"状态".repeat(300)),
        &history,
        &"当前".repeat(900),
    );
    assert!(crate::history_formatter::prompt_chars(&messages) <= MAX_PROMPT_CHARS);
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
fn assemble_rag_messages_keeps_writing_rules_under_long_persona() {
    let long_card = Card {
        id: "longcard".into(),
        name: "苏菲".into(),
        description: "冬堡学院的三年级学生，擅长制作冰糖葫芦，性格安静内向，喜欢在图书馆阅读古籍，讨厌嘈杂的集市，每天傍晚都会去集市门口的老约翰铺子买一份蜂蜜，养了一只名叫雪球的三花猫，猫只吃蜂蜜不吃猫粮，冬堡集市是她每周必去的地方，她还在学院剧团担任幕后道具管理，对舞台灯光有独特见解。".repeat(3),
        first_mes: "要来串糖葫芦吗？".into(),
        mes_example: "<START>".into(),
        system_prompt: "你是苏菲，冬堡学院的学生。你性格安静，擅长制作冰糖葫芦，养了一只猫叫雪球。你讨厌嘈杂，喜欢图书馆，每天傍晚去老约翰铺子买蜂蜜。".repeat(5),
        post_history_instructions: "记住用户的偏好，不要重复已经说过的话。".into(),
        ..Default::default()
    };
    let persona_desc =
        "一位好奇的旅人，四处游历，喜欢记录所见所闻，对冬堡的传说和民俗有浓厚兴趣。".repeat(10);
    let messages = assemble_rag_messages(
        &minimal_system_prompt(&long_card, Some(("Traveler", &persona_desc))),
        None,
        None,
        &[],
        "你好",
    );
    let system_content = &messages[0].content;
    assert!(
        system_content.contains("[写作规则]"),
        "最终 system 必须包含写作规则"
    );
    assert!(
        system_content.contains("严禁代替用户说话、行动或做决定"),
        "最终 system 必须完整保留禁止代替用户的硬约束"
    );
    assert!(
        system_content.contains("不要使用 {{user}}、{{char}} 等占位符"),
        "最终 system 必须保留占位符规则"
    );
    assert!(
        system_content.contains("冬堡学院的三年级学生"),
        "最终 system 必须保留角色描述的关键信息"
    );
    assert!(
        system_content.chars().count() <= MAX_SYSTEM_CHARS,
        "system 长度 {} 超过上限 {}",
        system_content.chars().count(),
        MAX_SYSTEM_CHARS
    );
    assert!(
        crate::history_formatter::prompt_chars(&messages) <= MAX_PROMPT_CHARS,
        "总 prompt 长度 {} 超过上限 {}",
        crate::history_formatter::prompt_chars(&messages),
        MAX_PROMPT_CHARS
    );
}

#[test]
fn assemble_rag_messages_writing_rules_survives_all_persona_sizes() {
    let base_card = |extra_desc: &str| Card {
        id: "sizecard".into(),
        name: "苏菲".into(),
        description: format!("冬堡学院的学生，{}", extra_desc),
        first_mes: "要来串糖葫芦吗？".into(),
        mes_example: "<START>".into(),
        system_prompt: "你是苏菲，冬堡学院的学生。".into(),
        post_history_instructions: "".into(),
        ..Default::default()
    };

    let cases = [
        ("short", "喜欢糖葫芦。"),
        ("medium", &"喜欢糖葫芦，每天傍晚去集市买蜂蜜，养了一只叫雪球的猫。".repeat(5)),
        ("long", &"喜欢糖葫芦，每天傍晚去集市买蜂蜜，养了一只叫雪球的三花猫，猫只吃蜂蜜不吃猫粮，冬堡集市是她每周必去的地方，她还在学院剧团担任幕后道具管理，对舞台灯光有独特见解，经常在图书馆阅读古籍，讨厌嘈杂的环境，性格安静内向。".repeat(10)),
    ];

    for (name, extra_desc) in cases {
        let card = base_card(extra_desc);
        let messages =
            assemble_rag_messages(&minimal_system_prompt(&card, None), None, None, &[], "你好");
        let system_content = &messages[0].content;
        assert!(
            system_content.contains("[写作规则]"),
            "[{name}] 最终 system 必须包含写作规则"
        );
        assert!(
            system_content.contains("严禁代替用户说话、行动或做决定"),
            "[{name}] 最终 system 必须完整保留禁止代替用户的硬约束"
        );
        assert!(
            system_content.chars().count() <= MAX_SYSTEM_CHARS,
            "[{name}] system 长度 {} 超过上限 {}",
            system_content.chars().count(),
            MAX_SYSTEM_CHARS
        );
    }
}
