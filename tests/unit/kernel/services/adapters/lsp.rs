use super::*;
use crate::kernel::services::ports::{AsyncExecutor, BoxFuture};
use crate::kernel::services::KernelServiceHost;
use std::sync::mpsc::TryRecvError;
use std::sync::Arc;

#[test]
fn file_url_roundtrip_unix() {
    #[cfg(unix)]
    {
        let path = PathBuf::from("/tmp/zcode test.rs");
        let url = lsp_types::Url::from_file_path(&path).unwrap();
        let back = url.to_file_path().unwrap();
        assert_eq!(back, path);
    }
}

#[test]
fn lsp_restart_backoff_blocks_ensure_started() {
    struct NoopExecutor;

    impl AsyncExecutor for NoopExecutor {
        fn spawn(&self, _task: BoxFuture) {}
    }

    let host = KernelServiceHost::new(Arc::new(NoopExecutor));
    let mut service = LspService::new(PathBuf::from("."), host.context());

    service.schedule_restart_backoff();
    assert_eq!(service.restart_attempts, 1);
    assert!(service.restart_backoff_until.is_some());
    assert!(!service.ensure_started());
    assert!(service.restart_backoff_until.is_some());
}

#[test]
fn lsp_restart_backoff_is_capped() {
    struct NoopExecutor;

    impl AsyncExecutor for NoopExecutor {
        fn spawn(&self, _task: BoxFuture) {}
    }

    let host = KernelServiceHost::new(Arc::new(NoopExecutor));
    let mut service = LspService::new(PathBuf::from("."), host.context());

    for _ in 0..16 {
        service.schedule_restart_backoff();
    }

    let until = service.restart_backoff_until.expect("backoff set");
    let delay = until.saturating_duration_since(Instant::now());
    assert!(delay <= Duration::from_secs(5));
}

#[test]
fn semantic_tokens_error_does_not_clear_previous_highlight() {
    struct NoopExecutor;

    impl AsyncExecutor for NoopExecutor {
        fn spawn(&self, _task: BoxFuture) {}
    }

    let mut host = KernelServiceHost::new(Arc::new(NoopExecutor));
    let ctx = host.context();

    handle_response(
        LspRequestKind::SemanticTokens {
            path: PathBuf::from("a.rs"),
            version: 42,
        },
        Response {
            id: RequestId::from(1),
            result: None,
            error: Some(lsp_server::ResponseError {
                code: 1,
                message: "stub error".to_string(),
                data: None,
            }),
        },
        &ctx,
    );

    assert!(matches!(host.try_recv(), Err(TryRecvError::Empty)));
}

#[test]
fn semantic_tokens_null_result_does_not_clear_previous_highlight() {
    struct NoopExecutor;

    impl AsyncExecutor for NoopExecutor {
        fn spawn(&self, _task: BoxFuture) {}
    }

    let mut host = KernelServiceHost::new(Arc::new(NoopExecutor));
    let ctx = host.context();

    handle_response(
        LspRequestKind::SemanticTokens {
            path: PathBuf::from("a.rs"),
            version: 42,
        },
        Response {
            id: RequestId::from(1),
            result: Some(serde_json::Value::Null),
            error: None,
        },
        &ctx,
    );

    assert!(matches!(host.try_recv(), Err(TryRecvError::Empty)));
}
