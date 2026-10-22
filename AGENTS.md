# umaai-onsen 项目规则

## 对话规则与工作规范

1. **语言**：思考和回答使用中文，语气简洁、专业。
2. **避免信息过载**：需要分析大量代码 / 数据并提取结论时，使用子代理。
3. **方案选择**：涉及不在上下文中的领域知识或需要选型时，停下来给出建议交用户选择，不直接生成代码。
4. **提案前审视**：提案前从「做减法」的角度重新考虑一次。

## 项目上下文

- 本仓是 **umaai-next（引擎仓）的温泉剧本**（ごくらく♪ゆこま温泉郷 / scenarioId 12）独立实现仓，
  以 path 依赖引用 `umaai-core` / `umasim` / `umaai` / `umaai-cli`；不 fork 引擎、不在引擎仓加剧本 crate。
- **移植源**：`../umaai-rs-ref` 的 onsen 实现
  （`crates/umasim/src/game/onsen/{mod,game,action}.rs`、`crates/umasim/src/gamedata/onsen.rs`）——**数值口径以它为准**。
- **资料参考**：`../mee-umasim`（Kotlin）**仅用于交叉核对公式**，不作为主参考。
- 引擎契约：`../umaai_next/docs/scenario/contracts.md`（三层 skill / Game 必备方法 / Deref「底座不得与契约同名」）。
- 本仓文档：`docs/plan/m1_plan.md`（M1 分步与验收）、`docs/glossary.md`（术语与口径分歧）、
  `docs/plan/library_candidates.md`、`docs/plan/engine_coupling.md`。

## 代码结构

**形态：单包多 bin，不改 workspace。** 剧本能力全在 lib；模拟 / 在线决策 / 测卡三类入口都是 **thin 外壳**
（bin 只解析参数 + 调 lib + 退出码，逻辑进 lib 子模块）。重依赖用 feature 门控（`file-channel` / `parallel`），
**在对应 bin 真正用上前不要往 `Cargo.toml` 加 feature**。

**lib 内部按功能区拆文件**（一目录一职责）：改机制时先落对应模块，**不要把新逻辑堆回 `lib.rs`**。

| 文件 | 职责 |
|---|---|
| `src/lib.rs` | 模块声明 + 对外重导出 + 常量（**不含实现**） |
| `src/game/mod.rs` | 局面 `OnsenGame`（字段 / `Deref` / 建局 / 快照 `OnsenView`）+ 装配 `build_game` |
| `src/game/stage.rs` | 阶段机 `Stage`（含 `StageId` 投影）/ `BathingInfo` / `stage_body` / 策略流派生 |
| `src/game/action.rs` | `Action` + `Display` + `ActionEnum` + 动作分派 `apply_onsen_action` |
| `src/game/bath.rs` | 入浴（温泉效果应用 / 羁绊 / 临时上限）/ 超回复分档与判定 / PR 本体 / 红点生成 |
| `src/game/train.rs` | 训练结算与温泉训练效果折算（inherent 辅助） |
| `src/game/dig.rs` | 挖掘力 / 挖掘量 / 进度奖励 / 装备升级 / 源泉选择 / 赠券 |
| `src/game/traits.rs` | 契约 impl（`Game` / `TrainingValueSource`，含 roster 与训练覆盖方法） |
| `src/data.rs` | 剧本资产 schema、载入与载入期校验 |

- **契约 trait impl 不可拆文件**：一个 trait 的 `impl` 必须整体落在同一文件（如 `deyilv` / `train_level` /
  `calc_training_value` 属 `impl Game`，只能随它一起放 `traits.rs`，不因「像是训练逻辑」就挪进 `train.rs`）；
  可拆的只有 inherent `impl`。
- **跨模块可见性**：被其它模块调用的方法标 `pub(crate)`；只有同模块内使用的才留私有（模块私有不对其它模块可见）。
- **阶段自持**：内部强类型 `Stage`，对外只投影 `StageId`（显式赋号，**改号 = 旧种子失效**）。
- **不碰引擎**：引擎改动只允许 `docs/plan/m1_plan.md` §4 的钩子清单（带默认实现、非破坏），
  且必须在提交说明点明「默认行为等价」与否。

## 数据与配置

- 剧本资产 `data/scenario_onsen.json` 与剧本配置段 `config/onsen_config.toml` 由本仓自持；
  引擎数据根（`gamedata/`）与剧本数据根分属两处，**都由宿主注入**，本仓不写死路径。

## Rust 编码规范

### 错误处理
1. `Result` 优先；可用 `Result` 的场合不用 `unwrap`。
2. 取 `Option` 内部值时优先 `ok_or_else` 转 `Result`；越界报错，不静默取末档。
3. 仅测试 / 临时代码允许 `unwrap` / `panic`。

### 代码结构
1. **新类型原则**：固定结构的数据建新类型，避免裸 Tuple。
2. **文档化注释**：新增函数 / 结构体须有 rustdoc 注释，函数注释 ≤10 行、文件头 ≤15 行；
   注释必须对应代码现状，历史修改写 `README.md` 状态行或 glossary，不挂在注释上。
3. 变量命名简短；长词可用缩写。
4. **`Deref` 底座不得与契约同名**（`BaseGame` 的公开方法不得与 `Game` 契约方法同名）。

## 测试规范

1. **分层与断言**：单元测试（内联，私有细节 / 纯函数）用 `assert`；**集成测试（`tests/`）不用 `assert`**，
   用打印 + `Result` 返回值供 Agent 检查。
2. 测试工作目录为**仓根**；引擎数据根默认 `../umaai_next/gamedata`，剧本数据根默认 `data`，
   可用 `UMAAI_DATA_ROOT` / `UMAAI_ONSEN_DATA_ROOT` 覆盖。
3. 机制改动需在 `tests/` 补对应机制单测；固定种子基线放 `baselines/`（剧本仓自持）。
4. **Release 档**跑测试与 binary；`cargo test` 统一加 `--offline`。

## 工具使用

- 日常 `cargo check` / `cargo clippy --all-targets -- -D warnings` / `cargo fmt --check` / `cargo test` 加 `--offline`。
- 需要改 registry（`cargo add` / `fetch`）时**先请求提权**。
- **同一 target 目录一次只跑一个 cargo**（并发会互等锁，表现为卡死）。
- 本仓沙箱可能拦全局 cargo 缓存（`D:\Rust\.cargo`），必要时提权执行。

## Git 操作

1. **不自动 `commit` / `push`**：先确认用户已完成全部调整，再进入提交准备。
2. **提交准备**：同步更新 `README.md` 状态行与 `docs/plan/m1_plan.md` §2 状态列，由用户确认后再提交。
3. **提交范围**：涵盖工作树下由 Agent 与用户做的全部修改。
4. 提交说明需点明**默认行为是否等价**（引擎侧）；剧本侧数值位移须在 `docs/glossary.md` 记口径分歧。
5. 遵循 Git 安全协议，避免破坏性操作。
