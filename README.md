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
| [`docs/plan/m1_plan.md`](docs/plan/m1_plan.md) | M1 方案：手写逻辑模拟 |
| [`docs/plan/library_candidates.md`](docs/plan/library_candidates.md) | 可进通用库的机制候选与鉴别结论 |
| [`docs/plan/search_notes.md`](docs/plan/search_notes.md) | 源泉选择的搜索难点与优化方向 |
| [`docs/plan/engine_coupling.md`](docs/plan/engine_coupling.md) | 引擎耦合面与引擎 review 的影响评估（分级、速查表、缓冲措施） |
| [`docs/plan/engine_adapter.md`](docs/plan/engine_adapter.md) | 剧本侧引擎适配层设计（统一接口文件：职责边界 / 能力清单 / 守门） |

## 状态

- 2026-10-19：仓初始化，文档先落（术语表 / M1 方案 / 通用库候选 / 搜索笔记）；代码按 M1 方案 A0 → A8 推进。
