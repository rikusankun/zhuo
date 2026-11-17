use anyhow::Result;
use zhuo_memory::{MemoryHit, MemoryStore};

use crate::state::ServerState;

pub enum MemoryQuery {
    Recent {
        total: usize,
        unprocessed: usize,
        memories: Vec<MemoryHit>,
    },
    Search(Vec<MemoryHit>),
}

pub enum SummaryOutcome {
    Disabled,
    Busy,
    Skipped,
    Completed(String),
}

pub fn add_memory(
    store: &MemoryStore,
    card: &str,
    summary: &str,
    keywords: &str,
    source_round: i64,
) -> Result<(i64, usize)> {
    let id = store.add_memory(card, summary, keywords, source_round)?;
    let total = store.count_memories(card)?;
    Ok((id, total))
}

pub fn query_memories(
    store: &MemoryStore,
    card: &str,
    query: &str,
    limit: usize,
) -> Result<MemoryQuery> {
    if query.is_empty() {
        return Ok(MemoryQuery::Recent {
            total: store.count_memories(card)?,
            unprocessed: store.count_unprocessed(card)?,
            memories: store.list_memories(card, limit)?,
        });
    }

    Ok(MemoryQuery::Search(store.search(card, query, limit)?))
}

pub fn commit_extracted_memory(
    store: &MemoryStore,
    card: &str,
    summary: &str,
    upto_message_id: i64,
) -> Result<bool> {
    store.commit_memory_batch(card, summary, upto_message_id)
}

/// 手动编辑某条记忆的内容（按卡隔离）。
pub fn update_memory(store: &MemoryStore, card: &str, id: i64, content: &str) -> Result<usize> {
    store.update_memory(card, id, content)
}

/// 手动删除某条记忆（按卡隔离）。
pub fn delete_memory(store: &MemoryStore, card: &str, id: i64) -> Result<usize> {
    store.delete_memory(card, id)
}

pub async fn summarize_card(st: &ServerState, card: &str) -> Result<SummaryOutcome> {
    if !st.summary_enabled() {
        return Ok(SummaryOutcome::Disabled);
    }

    let key = st.card_key(card);
    let Some(_guard) = st.extraction_guard(&key) else {
        return Ok(SummaryOutcome::Busy);
    };

    let result = crate::services::summarize::run_extraction(st, &key).await;
    result.map(|summary| match summary {
        Some(summary) => SummaryOutcome::Completed(summary),
        None => SummaryOutcome::Skipped,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_memory_returns_id_and_updated_total() {
        let store = MemoryStore::open_in_memory().unwrap();

        let (id, total) = add_memory(&store, "苏菲", "喜欢糖葫芦", "零食", 1).unwrap();

        assert!(id > 0);
        assert_eq!(total, 1);
    }

    #[test]
    fn add_memory_returns_validation_error_without_writing() {
        let store = MemoryStore::open_in_memory().unwrap();

        assert!(add_memory(&store, "苏菲", " ", "", 0).is_err());
        assert_eq!(store.count_memories("苏菲").unwrap(), 0);
    }

    #[test]
    fn update_memory_is_isolated_by_card_and_recall_uses_new_content() {
        let store = MemoryStore::open_in_memory().unwrap();
        let (id, _) = add_memory(&store, "苏菲", "旧内容", "旧", 1).unwrap();
        add_memory(&store, "冬堡", "冬堡内容", "冬", 1).unwrap();

        assert_eq!(
            update_memory(&store, "苏菲", id, "新内容：山楂").unwrap(),
            1
        );
        // 他角色同 id 不应被改动
        assert_eq!(update_memory(&store, "冬堡", id, "不该生效").unwrap(), 0);

        let sophie = store.list_memories("苏菲", 10).unwrap();
        assert_eq!(sophie[0].summary, "新内容：山楂");
        let dongbao = store.list_memories("冬堡", 10).unwrap();
        assert_eq!(dongbao[0].summary, "冬堡内容");

        // 下次召回使用新内容
        let hits = store.search_prompt_context("苏菲", "山楂").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, "新内容：山楂");
    }

    #[test]
    fn delete_memory_removes_entry_and_is_isolated_by_card() {
        let store = MemoryStore::open_in_memory().unwrap();
        let (sophie_id, _) = add_memory(&store, "苏菲", "苏菲记忆", "苏菲", 1).unwrap();
        let (dongbao_id, _) = add_memory(&store, "冬堡", "冬堡记忆", "冬堡", 1).unwrap();

        assert_eq!(delete_memory(&store, "苏菲", sophie_id).unwrap(), 1);
        // 跨角色误删同 id：不应生效
        assert_eq!(delete_memory(&store, "冬堡", sophie_id).unwrap(), 0);

        assert!(store.list_memories("苏菲", 10).unwrap().is_empty());
        let remaining = store.list_memories("冬堡", 10).unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, dongbao_id);
    }

    #[test]
    fn manually_added_memory_can_be_recalled() {
        let store = MemoryStore::open_in_memory().unwrap();
        let (id, _) = add_memory(&store, "苏菲", "苏菲的秘密基地在冬堡图书馆顶层", "", 0).unwrap();
        assert!(id > 0);

        let hits = store.search_prompt_context("苏菲", "图书馆").unwrap();
        assert!(!hits.is_empty());
        assert!(hits.iter().any(|h| h.content.contains("图书馆顶层")));
        // 他角色看不到
        assert!(store
            .search_prompt_context("冬堡", "图书馆")
            .unwrap()
            .is_empty());
    }
}
