//! 世界书注入：JSON 扁平化、检索块渲染、角色状态渲染。

use serde_json::Value;
use zhuo_memory::PromptHit;

pub const MAX_RETRIEVAL_CHARS: usize = 500;

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

/// 渲染 [当前状态] 块：顶层键值各占一行，数组用「、」连接；
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
    fn render_state_formats_arrays_strings_numbers() {
        let v = serde_json::json!({"背包": ["木剑", "药水"], "位置": "冬堡学院", "好感度": 5});
        let s = render_state(&v).unwrap();
        assert!(s.starts_with("[当前状态]"));
        assert!(s.contains("背包：木剑、药水"));
        assert!(s.contains("位置：冬堡学院"));
        assert!(s.contains("好感度：5"));
        assert!(render_state(&serde_json::json!({})).is_none());
        assert!(render_state(&serde_json::json!({"x": "", "y": []})).is_none());
    }
}
