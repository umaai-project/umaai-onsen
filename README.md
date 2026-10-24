# umaai-onsen（温泉剧本仓）

`umaai-next` 引擎仓的温泉剧本（ごくらく♪ゆこま温泉郷 / scenarioId 12）独立实现仓。

- **形态**：独立仓，以 path 依赖引用引擎仓 `umaai-core` / `umasim` / `umaai`（稳定期换 git tag）；
  不 fork 引擎、不在引擎仓加剧本 crate。
- **交付物**：lib 为主（剧本类型 + 装配 API + View 快照），bin 为辅（主线自测外壳）。
- **数据**：剧本资产（`scenario_onsen.json`）与剧本配置段由本仓自持，根目录由宿主注入。

## 文档

| 文档 | 内容 |
|---|---|
| [`docs/glossary.md`](docs/glossary.md) | 术语单一事实源（含口径决定记录） |
| [`docs/mechanism.md`](docs/mechanism.md) | 机制文档（公式 / 流程 / 实现落点 / 与旧仓统计口径对照） |
| [`docs/tests_overview.md`](docs/tests_overview.md) | 测试一览（单元 / 机制 / 冒烟 / 基线） |
| [`docs/plan/m1_plan.md`](docs/plan/m1_plan.md) | M1 方案：手写逻辑模拟 |
| [`docs/plan/library_candidates.md`](docs/plan/library_candidates.md) | 可进通用库的机制候选与鉴别结论 |
| [`docs/plan/search_notes.md`](docs/plan/search_notes.md) | 源泉选择的搜索难点与优化方向 |
| [`docs/plan/engine_coupling.md`](docs/plan/engine_coupling.md) | 引擎耦合面与引擎 review 的影响评估（分级、速查表、缓冲措施） |

## 状态

- 2026-10-19：仓初始化，文档先落（术语表 / M1 方案 / 通用库候选 / 搜索笔记）；代码按 M1 方案 A0 → A8 推进。
- 2026-10-20：M1a A1–A5 落地（骨架 / 数据层 / 阶段机 / 训练与挖掘 / 入浴与超回复）；
  lib 按功能区拆为 `game/{mod,stage,action,bath,train,dig,traits}.rs`，`lib.rs` 只留声明与重导出。
- 2026-10-24：M1a A6 与 M1b B1–B4 剧本机制落地（比赛倍率 / 事件链与特例 / 女将与出行券 /
  年末与宴会 / 链接 / 分身 / 休息心得 / 固定回合特例），新增 `game/event.rs`；
  A7 手写训练员最小版（守门链 + 简化打分 + 事件贪心）落在 `src/trainer.rs`。
- 2026-10-24（第二批）：A8 装配与呈现（`src/config.rs` 剧本配置段走 H5 覆盖层语义、
  `OnsenView` 扩为温泉资源全集快照、bin 可切 `onsen` / `random` 训练员端到端跑通）；
  B5 手写策略全量移植（4 条守门链 + 打分表 + 一屏超参，替换简化打分）；
  B6 固定种子基线（`src/bench.rs` + `baselines/`）与机制文档 / 测试一览落仓。
  M1 全量完成。
- 2026-10-24（第三批，口径修正）：与旧仓对照后拍板三处——分身门槛按全部 7 类卡种统计
  （glossary §13.2 #12）、非保科友人卡建局报错（#13，仓根 `ai_config.toml` 覆盖层把默认
  卡组的塔可换成女将）、支援卡连续事件候选集仅 `Card` 类型（引擎侧记 open_items #31，
  暂不实施）；基线随之重录（64083 → 70897）。
- 2026-10-24（第四批，M1 收尾）：手动训练员 `ManualTrainer`（`src/manual.rs`，inquire 终端
  交互，feature `manual` 门控、默认开启）——每次决策前打印局面面板、玩家手选动作与事件
  选项，用于人工验证剧本机制；bin 训练员可切 `onsen` / `random` / `manual`。M1 全部收口。
