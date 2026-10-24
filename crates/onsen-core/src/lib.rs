//! # 温泉剧本机制本体（`onsen-core`）
//!
//! 引擎仓 `umaai-next` 的温泉剧本（ごくらく♪ゆこま温泉郷 / scenarioId 12）实现，
//! 是本 workspace 的**宿主稳定面**：机制 + 训练员 + 装配都在这里。
//! 本 crate 只依赖引擎稳定面（`umaai-core` / `umasim` / `umaai`），**不改引擎一行**。
//!
//! ## 结构
//!
//! - 局面 [`OnsenGame`]：引擎状态底座 [`BaseGame`] + 人头 + 温泉资源 + 私有阶段机；
//! - 动作 [`Action`]：训练 / 比赛 / 休息 / 出行 / 治病 / PR / 入浴 / 源泉选择 / 装备升级；
//! - 训练员 [`OnsenTrainer`]（手写策略；M2 增搜索训练员与适配器）；手动训练员在 `onsen-cli`；
//! - 配置 [`OnsenConfig`]：剧本专属段（源泉选择顺序等），由宿主注入；
//! - 数据 [`OnsenData`]：剧本资产（源泉 / 挖掘 / 事件表），由宿主注入根目录。
//!
//! ## 纪律（见引擎仓 `docs/scenario/`）
//!
//! - **阶段自持**：内部强类型枚举，对外只投影 [`StageId`]（显式赋号，改号＝旧种子失效）；
//! - **三条随机轴**：局面流 / 事件流从 [`BaseGame`] 取出，策略流本局面自持，决策流只给 `Trainer`；
//! - **主种子必注入**：`set_rule_master` 是唯一注入点，未注入就推进即报错；
//! - **数据与配置由宿主注入**：本 crate 不写死路径。
//!
//! 当前进度：M1a A1–A8 与 M1b B1–B6 已完成（机制文档见 `docs/mechanism.md`，测试一览见
//! `docs/tests_overview.md`，固定种子基线见 `baselines/`、裁判在 `onsen-sim`）。

pub mod config;
pub mod data;
pub mod game;
pub mod testing;
pub mod trainer;

pub use config::{OnsenConfig, OnsenOrder};
pub use data::{HotelEffect, OnsenData, OnsenEffect, OnsenInfo, SuperEffect};
pub use game::{Action, BathingInfo, OnsenGame, OnsenView, Stage, build_game};
pub use trainer::OnsenTrainer;

/// 育成最大回合（0 基，78 回合）
pub const MAX_TURN: i32 = 77;
