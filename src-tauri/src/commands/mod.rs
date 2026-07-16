//! Tauri 命令集合（按领域拆分）。
//!
//! 历史上这些命令全部位于 `commands.rs` 单文件中，现已按功能领域拆分到各子模块。
//! 本文件仅做模块声明与重新导出，保持 `commands::xxx` 的访问路径不变，
//! 使 `main.rs` 的 `generate_handler![...]` 注册与 `localhost_api` / `agent` / `bot`
//! 对内部 helper 的引用完全无需改动。

mod shared;
mod stats;
mod timeline;
mod memory;
mod report;
mod ask;
mod category;
mod recording;
mod avatar;
mod config;
mod ai;
mod updater;
mod integration;
mod system;
mod work_journal;
mod work_journal_import;

// 所有 pub command + DTO（main.rs generate_handler 的 commands::xxx 不变）
pub use stats::*;
pub use timeline::*;
pub use memory::*;
pub use report::*;
pub use ask::*;
pub use category::*;
pub use recording::*;
pub use avatar::*;
pub use config::*;
pub use ai::*;
pub use updater::*;
pub use integration::*;
pub use system::*;
pub use work_journal::*;
pub use work_journal_import::*;

// 被 main.rs / localhost_api / agent / bot 直接调用的 pub(crate) helper
// 通过子模块再次 re-export，保持 `commands::xxx_inner` / `commands::xxx` 路径不断。
pub(crate) use shared::{
    filter_activities_by_privacy, load_filtered_activities_in_range,
    persist_app_config, parse_temporal_range, resolve_single_date,
};
pub(crate) use system::apply_dock_visibility;
