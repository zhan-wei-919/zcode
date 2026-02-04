use super::*;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct MockOps {
    calls: Mutex<Vec<&'static str>>,
}

impl TerminalOps for MockOps {
    fn setup(&self) -> std::io::Result<()> {
        self.calls.lock().unwrap().push("setup");
        Ok(())
    }

    fn restore(&self) -> std::io::Result<()> {
        self.calls.lock().unwrap().push("restore");
        Ok(())
    }
}

#[test]
fn terminal_guard_restores_on_drop() {
    let ops = Arc::new(MockOps::default());
    {
        let _guard = TerminalGuard::with_ops(ops.clone()).unwrap();
    }

    assert_eq!(&*ops.calls.lock().unwrap(), &["setup", "restore"]);
}

#[test]
fn terminal_restorer_is_idempotent() {
    let ops = Arc::new(MockOps::default());
    let guard = TerminalGuard::with_ops(ops.clone()).unwrap();
    let restorer = guard.restorer();

    restorer.restore().unwrap();
    restorer.restore().unwrap();
    drop(guard);

    assert_eq!(&*ops.calls.lock().unwrap(), &["setup", "restore"]);
}
