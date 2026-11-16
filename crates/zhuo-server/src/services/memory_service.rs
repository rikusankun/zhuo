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
}
