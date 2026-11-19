//! 世界书触发引擎：SillyTavern 兼容的关键词匹配与条目激活逻辑。

use crate::types::LoreEntry;

fn keyword_matches(text: &str, key: &str, case_sensitive: bool, whole: bool) -> bool {
    if key.is_empty() {
        return false;
    }
    let (hay, needle) = if case_sensitive {
        (text.to_string(), key.to_string())
    } else {
        (text.to_lowercase(), key.to_lowercase())
    };
    if !whole {
        return hay.contains(&needle);
    }
    hay.match_indices(&needle).any(|(i, _)| {
        let before = hay[..i].chars().next_back();
        let after = hay[i + needle.len()..].chars().next();
        before
            .map(|c| !c.is_alphanumeric() && c != '_')
            .unwrap_or(true)
            && after
                .map(|c| !c.is_alphanumeric() && c != '_')
                .unwrap_or(true)
    })
}

/// 核心触发逻辑：根据聊天历史匹配世界书条目
pub fn activated_entries_internal(entries: &[LoreEntry], history: &[String]) -> Vec<LoreEntry> {
    let mut out: Vec<_> = entries
        .iter()
        .filter(|e| e.enabled)
        .filter(|e| {
            if e.constant {
                return true;
            }
            let depth = e.scan_depth.unwrap_or(history.len());
            let text = history
                .iter()
                .rev()
                .take(depth)
                .rev()
                .cloned()
                .collect::<Vec<_>>()
                .join("\n");
            let primary_keys: Vec<&str> = e
                .triggers
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            let secondary_keys: Vec<&str> = e
                .secondary_triggers
                .split(',')
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .collect();
            let primary = primary_keys
                .iter()
                .any(|k| keyword_matches(&text, k, e.case_sensitive, e.match_whole_words));
            let secondary = !e.selective
                || secondary_keys.is_empty()
                || secondary_keys
                    .iter()
                    .any(|k| keyword_matches(&text, k, e.case_sensitive, e.match_whole_words));
            primary && secondary
        })
        .cloned()
        .collect();
    out.sort_by_key(|e| e.order);
    out
}
