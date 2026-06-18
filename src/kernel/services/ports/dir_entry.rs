//! 目录加载的传输载荷：`AsyncRuntime::load_dir` 产出，经 `AppMessage::DirLoaded`
//! / `Action::DirLoaded` 传到 `state.apply_dir_loaded` 消费的最小目录项契约。

#[derive(Debug, Clone)]
pub struct DirEntryInfo {
    pub name: String,
    pub is_dir: bool,
}
