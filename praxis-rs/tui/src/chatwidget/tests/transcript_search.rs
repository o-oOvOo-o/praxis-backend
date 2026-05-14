use super::*;
use crate::history_cell::HistoryCell;
use crate::history_cell::PlainHistoryCell;
use pretty_assertions::assert_eq;
use ratatui::text::Line;
use std::sync::Arc;

fn committed_cell(text: &str) -> Arc<dyn HistoryCell> {
    Arc::new(PlainHistoryCell::new(vec![Line::from(text.to_string())])) as Arc<dyn HistoryCell>
}

#[tokio::test]
async fn transcript_search_can_target_live_tail_results() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let transcript_cells = vec![committed_cell("alpha")];
    chat.active_cell = Some(Box::new(PlainHistoryCell::new(vec![Line::from(
        "tail beta",
    )])));

    chat.open_transcript_search();
    let overlay_state = chat
        .set_transcript_search_query("beta", 80, &transcript_cells)
        .expect("search state");

    assert_eq!(overlay_state.current_chunk, Some(1));
    assert_eq!(overlay_state.highlight_cell, None);
    assert_eq!(overlay_state.status.result_count, 1);
    assert_eq!(overlay_state.status.current_ordinal, Some(1));
    assert!(
        overlay_state
            .status
            .current_target
            .expect("current target")
            .is_live_tail
    );
}

#[tokio::test]
async fn transcript_search_next_wraps_across_committed_matches() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let transcript_cells = vec![committed_cell("beta one"), committed_cell("beta two")];

    chat.open_transcript_search();
    let first = chat
        .set_transcript_search_query("beta", 80, &transcript_cells)
        .expect("first state");
    assert_eq!(first.current_chunk, Some(0));
    assert_eq!(first.status.current_ordinal, Some(1));

    let second = chat
        .transcript_search_next(80, &transcript_cells)
        .expect("second state");
    assert_eq!(second.current_chunk, Some(1));
    assert_eq!(second.highlight_cell, Some(1));
    assert_eq!(second.status.current_ordinal, Some(2));
    assert!(!second.status.wrapped);

    let wrapped = chat
        .transcript_search_next(80, &transcript_cells)
        .expect("wrapped state");
    assert_eq!(wrapped.current_chunk, Some(0));
    assert_eq!(wrapped.status.current_ordinal, Some(1));
    assert!(wrapped.status.wrapped);
}

#[tokio::test]
async fn transcript_search_refresh_picks_up_new_active_tail_matches() {
    let (mut chat, _rx, _op_rx) = make_chatwidget_manual(/*model_override*/ None).await;
    let transcript_cells = vec![committed_cell("alpha")];

    chat.open_transcript_search();
    let initial = chat
        .set_transcript_search_query("beta", 80, &transcript_cells)
        .expect("initial state");
    assert_eq!(initial.status.result_count, 0);

    chat.active_cell = Some(Box::new(PlainHistoryCell::new(vec![Line::from(
        "beta appears",
    )])));
    let refreshed = chat
        .refresh_transcript_search(80, &transcript_cells)
        .expect("refreshed state");

    assert_eq!(refreshed.current_chunk, Some(1));
    assert_eq!(refreshed.status.result_count, 1);
    assert_eq!(chat.transcript_search_query(), "beta");
    assert_eq!(
        chat.transcript_search_status()
            .expect("status")
            .current_ordinal,
        Some(1)
    );
}
