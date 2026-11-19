//! jieba 中文分词工具：搜索级分词、去噪、FTS5 MATCH 查询语法生成。

use crate::config::STOPWORDS;
use jieba_rs::Jieba;
use std::sync::OnceLock;

fn jieba() -> &'static Jieba {
    static JIEBA: OnceLock<Jieba> = OnceLock::new();
    JIEBA.get_or_init(Jieba::new)
}

fn is_noise(token: &str) -> bool {
    if token.chars().all(|c| {
        c.is_whitespace()
            || c.is_ascii_punctuation()
            || matches!(c as u32,
            0x3000..=0x303F | 0xFF00..=0xFFEF | 0x2018..=0x201D | 0x2026 | 0x2014)
    }) {
        return true;
    }
    STOPWORDS.contains(&token)
}

/// jieba 搜索级分词 + 去噪，输出空格分隔的 token 流（FTS5 unicode61 以空格切词）。
pub fn seg(text: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for t in jieba().cut_for_search(text, true) {
        let t = t.trim();
        if !t.is_empty() && !is_noise(t) && !out.contains(&t) {
            out.push(t);
        }
    }
    out.join(" ")
}

/// 把分好的 token 流拼成 FTS5 MATCH 查询语法（OR 语义，bm25 排序自动权衡相关度）。
pub fn fts5_query(text: &str) -> String {
    let tokens: Vec<String> = seg(text)
        .split(' ')
        .filter(|s| !s.is_empty())
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect();
    tokens.join(" OR ")
}
