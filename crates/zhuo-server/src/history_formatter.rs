//! 对话历史格式化：RAG 消息组装、长度预算控制、状态/检索裁剪。

use zhuo_llm::{estimate_tokens, Msg};

use crate::lore_injector::MAX_RETRIEVAL_CHARS;
use crate::system_prompt::MAX_SYSTEM_CHARS;

pub const MAX_PROMPT_CHARS: usize = 1500;
pub const MAX_HISTORY_MESSAGES: usize = 6;

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
