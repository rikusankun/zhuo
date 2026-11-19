use axum::extract::State;
use axum::Json;
use serde_json::json;

use crate::config::RegexScript;
use crate::state::ServerState;

pub async fn compatibility(State(st): State<ServerState>) -> Json<serde_json::Value> {
    let mem = st.memory.lock().unwrap_or_else(|e| e.into_inner());
    let book = mem
        .get_setting("worldbook")
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str::<zhuo_card::WorldBook>(&s).ok());
    let scripts = mem
        .get_setting("regex_scripts")
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str::<Vec<RegexScript>>(&s).ok())
        .unwrap_or_default();
    Json(json!({"worldbook": book, "regex_scripts": scripts}))
}
