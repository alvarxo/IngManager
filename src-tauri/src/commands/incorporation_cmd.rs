use tauri::State;

use crate::error::AppResult;
use crate::incorporation::{
    apply_incorporation, preview_incorporation, IncorporationApplyInput, IncorporationApplyResult,
    IncorporationPreview, IncorporationPreviewInput,
};
use crate::state::AppState;

#[tauri::command]
pub async fn preview_document_incorporation(
    state: State<'_, AppState>,
    input: IncorporationPreviewInput,
) -> AppResult<IncorporationPreview> {
    preview_incorporation(&state.pool, input).await
}

#[tauri::command]
pub async fn apply_document_incorporation(
    state: State<'_, AppState>,
    input: IncorporationApplyInput,
) -> AppResult<IncorporationApplyResult> {
    apply_incorporation(&state.pool, input).await
}
