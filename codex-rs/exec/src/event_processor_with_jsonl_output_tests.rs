use super::*;
use crate::exec_events::McpServerCancelledEvent;
use crate::exec_events::McpServerFailedEvent;
use crate::exec_events::McpServerInitStartedEvent;
use crate::exec_events::McpServerReadyEvent;
use codex_app_server_protocol::McpServerStartupState;
use codex_app_server_protocol::McpServerStatusUpdatedNotification;
use pretty_assertions::assert_eq;
use tempfile::tempdir;

#[test]
fn failed_turn_does_not_overwrite_output_last_message_file() {
    let tempdir = tempdir().expect("create tempdir");
    let output_path = tempdir.path().join("last-message.txt");
    std::fs::write(&output_path, "keep existing contents").expect("seed output file");

    let mut processor = EventProcessorWithJsonOutput::new(Some(output_path.clone()));

    let collected = processor.collect_thread_events(ServerNotification::ItemCompleted(
        codex_app_server_protocol::ItemCompletedNotification {
            item: ThreadItem::AgentMessage {
                id: "msg-1".to_string(),
                text: "partial answer".to_string(),
                phase: None,
                memory_citation: None,
            },
            thread_id: "thread-1".to_string(),
            turn_id: "turn-1".to_string(),
        },
    ));

    assert_eq!(collected.status, CodexStatus::Running);
    assert_eq!(processor.final_message(), Some("partial answer"));

    let status = processor.process_server_notification(ServerNotification::TurnCompleted(
        codex_app_server_protocol::TurnCompletedNotification {
            thread_id: "thread-1".to_string(),
            turn: codex_app_server_protocol::Turn {
                id: "turn-1".to_string(),
                items: Vec::new(),
                status: TurnStatus::Failed,
                error: Some(codex_app_server_protocol::TurnError {
                    message: "turn failed".to_string(),
                    additional_details: None,
                    codex_error_info: None,
                }),
                started_at: None,
                completed_at: Some(0),
                duration_ms: None,
            },
        },
    ));

    assert_eq!(status, CodexStatus::InitiateShutdown);
    assert_eq!(processor.final_message(), None);

    EventProcessor::print_final_output(&mut processor);

    assert_eq!(
        std::fs::read_to_string(&output_path).expect("read output file"),
        "keep existing contents"
    );
}

#[test]
fn mcp_server_status_updates_are_surfaced_as_thread_events() {
    let mut processor = EventProcessorWithJsonOutput::new(None);

    let starting = processor.collect_thread_events(ServerNotification::McpServerStatusUpdated(
        McpServerStatusUpdatedNotification {
            name: "docs".to_string(),
            status: McpServerStartupState::Starting,
            error: None,
        },
    ));
    assert_eq!(starting.status, CodexStatus::Running);
    assert_eq!(
        starting.events,
        vec![ThreadEvent::McpServerInitStarted(
            McpServerInitStartedEvent {
                name: "docs".to_string(),
            }
        )]
    );

    let ready = processor.collect_thread_events(ServerNotification::McpServerStatusUpdated(
        McpServerStatusUpdatedNotification {
            name: "docs".to_string(),
            status: McpServerStartupState::Ready,
            error: None,
        },
    ));
    assert_eq!(ready.status, CodexStatus::Running);
    assert_eq!(
        ready.events,
        vec![ThreadEvent::McpServerReady(McpServerReadyEvent {
            name: "docs".to_string(),
        })]
    );

    let failed = processor.collect_thread_events(ServerNotification::McpServerStatusUpdated(
        McpServerStatusUpdatedNotification {
            name: "flaky".to_string(),
            status: McpServerStartupState::Failed,
            error: Some("handshake timeout".to_string()),
        },
    ));
    assert_eq!(failed.status, CodexStatus::Running);
    assert_eq!(
        failed.events,
        vec![ThreadEvent::McpServerFailed(McpServerFailedEvent {
            name: "flaky".to_string(),
            error: Some("handshake timeout".to_string()),
        })]
    );

    let cancelled = processor.collect_thread_events(ServerNotification::McpServerStatusUpdated(
        McpServerStatusUpdatedNotification {
            name: "slow".to_string(),
            status: McpServerStartupState::Cancelled,
            error: Some("interrupted before handshake".to_string()),
        },
    ));
    assert_eq!(cancelled.status, CodexStatus::Running);
    assert_eq!(
        cancelled.events,
        vec![ThreadEvent::McpServerCancelled(McpServerCancelledEvent {
            name: "slow".to_string(),
            error: Some("interrupted before handshake".to_string()),
        })]
    );
}
