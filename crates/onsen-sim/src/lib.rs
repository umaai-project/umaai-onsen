//! # 温泉模拟器（`onsen-sim`）
//!
//! 模拟器业务 crate：固定种子基线裁判（[`BaselineRecord`]，「变更探测器」），
//! 后续承接批量跑局与测卡测量函数（复用 `onsen-core` 的局面与训练员）。
//! 基线 JSON 由仓自持（仓根 `baselines/`），测试工作目录为 workspace 根。

pub mod bench;

pub use bench::BaselineRecord;
