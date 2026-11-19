//! World-book / lore-entry business logic: extracted from routes to keep
//! HTTP handlers thin and domain logic testable without Axum types.

use crate::config::{ChatLoreEntry, RegexScript};
use zhuo_card::{Card, WorldBook};
use zhuo_memory::LoreEntry;

/**
 * Build the flattened list of LoreEntry rows that will be indexed into
 * SQLite for a given card. Combines card fields, embedded world book,
 * imported world book, raw world text, client lore entries, and legacy world
 * info.
 */
pub fn card_lore_entries(
    card: &Card,
    world: &str,
    worldbook: Option<&WorldBook>,
    client_entries: &[ChatLoreEntry],
    legacy_world_info: &[String],
    history: Option<&[String]>,
) -> Vec<LoreEntry> {
    let mut entries = Vec::new();

    for (field, title, content) in [
        ("description", "角色背景", card.description.as_str()),
        ("personality", "角色性格", card.personality.as_str()),
        ("scenario", "故事场景", card.scenario.as_str()),
        ("system_prompt", "角色指令", card.system_prompt.as_str()),
        ("first_mes", "开场白", card.first_mes.as_str()),
        ("mes_example", "示例对话", card.mes_example.as_str()),
        (
            "post_history_instructions",
            "历史后指令",
            card.post_history_instructions.as_str(),
        ),
        ("creator_notes", "创作者注释", card.creator_notes.as_str()),
    ] {
        push_lore_entry(
            &mut entries,
            format!("card:{field}"),
            title.into(),
            card.name.clone(),
            content,
            false,
        );
    }
    for (index, greeting) in card.alternate_greetings.iter().enumerate() {
        push_lore_entry(
            &mut entries,
            format!("card:alternate_greeting:{index}"),
            format!("替代开场白 {}", index + 1),
            card.name.clone(),
            greeting,
            false,
        );
    }
    if !card.tags.is_empty() {
        push_lore_entry(
            &mut entries,
            "card:tags".into(),
            "角色标签".into(),
            card.name.clone(),
            &card.tags.join(" "),
            false,
        );
    }

    if let Some(book) = &card.character_book {
        append_worldbook(&mut entries, "card-book", book, history);
    }
    if let Some(book) = worldbook {
        append_worldbook(&mut entries, "imported-book", book, history);
    }
    for (index, content) in world.split("\n\n").enumerate() {
        push_lore_entry(
            &mut entries,
            format!("world:{index}"),
            format!("世界设定 {}", index + 1),
            card.name.clone(),
            content,
            false,
        );
    }
    for (index, entry) in client_entries.iter().enumerate() {
        if !entry.enabled {
            continue;
        }
        if let Some(history) = history {
            let latest = history.last().map(|s| s.to_lowercase()).unwrap_or_default();
            if !entry
                .keys
                .split([',', '，'])
                .map(str::trim)
                .any(|key| !key.is_empty() && latest.contains(&key.to_lowercase()))
            {
                continue;
            }
        }
        push_lore_entry(
            &mut entries,
            format!("client-lore:{index}:{}", entry.id),
            "自定义世界书".into(),
            entry.keys.clone(),
            &entry.content,
            history.is_some(),
        );
    }
    for (index, content) in legacy_world_info.iter().enumerate() {
        push_lore_entry(
            &mut entries,
            format!("legacy-world-info:{index}"),
            "旧版世界信息".into(),
            card.name.clone(),
            content,
            false,
        );
    }
    entries
}

fn push_lore_entry(
    out: &mut Vec<LoreEntry>,
    source_key: String,
    title: String,
    triggers: String,
    content: &str,
    constant: bool,
) {
    if !content.trim().is_empty() {
        out.push(LoreEntry {
            source_key,
            title,
            triggers,
            secondary_triggers: String::new(),
            content: content.trim().to_string(),
            constant,
            enabled: true,
            selective: false,
            case_sensitive: false,
            match_whole_words: true,
            order: 0,
            scan_depth: None,
        });
    }
}

fn append_worldbook(
    out: &mut Vec<LoreEntry>,
    source: &str,
    book: &WorldBook,
    history: Option<&[String]>,
) {
    let active = history.map(|messages| zhuo_card::activated_entries(book, messages));
    for (index, entry) in book
        .entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.enabled)
    {
        if active
            .as_ref()
            .is_some_and(|entries| !entries.contains(entry))
        {
            continue;
        }
        out.push(LoreEntry {
            source_key: format!("{source}:{index}:{}", entry.id),
            title: if entry.id.is_empty() {
                format!("{} 条目 {}", book.name, index + 1)
            } else {
                format!("{} {}", book.name, entry.id)
            },
            triggers: entry.keys.join(","),
            secondary_triggers: entry.secondary_keys.join(","),
            content: entry.content.clone(),
            constant: entry.constant || history.is_some(),
            enabled: entry.enabled,
            selective: entry.selective,
            case_sensitive: entry.case_sensitive,
            match_whole_words: entry.match_whole_words,
            order: entry.order,
            scan_depth: entry.scan_depth,
        });
    }
}

/// Apply output regex scripts to model output. placement == 2 means
/// "output stage"; prompt_only scripts are skipped here.
pub fn apply_output_regex(input: &str, scripts: &[RegexScript]) -> String {
    scripts
        .iter()
        .filter(|s| {
            !s.disabled
                && !s.prompt_only
                && !s.find_regex.trim().is_empty()
                && (s.placement.is_empty() || s.placement.contains(&2))
        })
        .fold(input.to_string(), |text, script| {
            let raw = script.find_regex.trim();
            let (pattern, flags) = if raw.starts_with('/') && raw[1..].rfind('/').is_some() {
                let end = raw.rfind('/').unwrap();
                (&raw[1..end], &raw[end + 1..])
            } else {
                (raw, "")
            };
            let mut builder = regex::RegexBuilder::new(pattern);
            builder
                .case_insensitive(flags.contains('i'))
                .multi_line(flags.contains('m'))
                .dot_matches_new_line(flags.contains('s'));
            let Ok(re) = builder.build() else { return text };
            let replacement = script
                .replace_string
                .replace("{{match}}", "$0")
                .replace("$&", "$0");
            if flags.contains('g') {
                re.replace_all(&text, replacement.as_str()).to_string()
            } else {
                re.replace(&text, replacement.as_str()).to_string()
            }
        })
}
