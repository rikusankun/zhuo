//! 紧凑 Prompt 组装：角色核心信息、检索片段、角色状态和最近历史均有硬长度限制。

pub use crate::history_formatter::{
    assemble_rag_messages, estimate_prompt_tokens, prompt_chars,
};
pub use crate::history_formatter::MAX_PROMPT_CHARS;
pub use crate::lore_injector::{retrieval_block, render_state, world_text};
pub use crate::system_prompt::{minimal_system_prompt, system_prompt};
