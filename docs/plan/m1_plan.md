# M1 方案：温泉剧本仓「手写逻辑模拟」

> 目标：在 `umaai-next` 重构后的引擎上重写温泉剧本（**独立剧本仓** `umaai-onsen`），先把
> **手写逻辑模拟**跑通，以验证基础层（契约 / 随机分层 / 注入点 / View）够不够用。
> 决策基线（2026-10-19 拍板）：仓位 `UmaAI_Active/umaai-onsen/`（与 `umaai_next` 并列，path 依赖）；
> M1 两步走 M1a → M1b；手写策略先最小可用，数值对齐后再移植全量；**允许本轮给引擎加带默认实现的钩子**。
> 术语以 [`docs/glossary.md`](../glossary.md) 为准。

***

## 1. 验收口径（M1 完成判据）

1. 固定种子经 `Runner` 跑满 78 回合，无报错、无 panic 分支；同一 `master` 两次运行逐位一致。
2. 手写策略覆盖 5 类决策点：源泉选择、装备升级、入浴用券、训练/比赛/休息/出行/PR、事件选项。
3. 关键机制有单测：温泉训练表五档、训练上下层加成、失败率下降（温泉效果生效期间扣减）、
   挖掘力三部分、跨地层公式、30 点挖掘奖励、超回复分档与 275 保底、温泉券上限与发放回合、
   旅馆档位、五维上限注入。
4. 机制文档（中文）从日文 memo + 旧实现抽取落仓，与旧仓的口径分歧逐条记录（见术语表 §13）。
5. 剧本仓 `cargo check` / `clippy -D warnings` / 测试全绿；引擎仓改动仅限 §4 钩子清单。
6. 引擎仓在其钩子落地后：`fmt` / `clippy` / `test` 三档全绿，且**默认行为不变**（无钩子使用方时数值不位移）。

***

## 2. 阶段拆解

### M1a：跑通（骨架 + 核心机制）

| 步 | 内容 | 产出 | 验收 | 状态 |
|---|---|---|---|---|
| A0 | 引擎钩子落地（§4 的 H1 / H2 / H4 / H5） | 引擎带默认实现的钩子 + 单测 | 引擎三档全绿；无使用方时行为不变（基线不位移） | ✅ 2026-10-19 |
| A1 | 建仓：复制模板、改名 `umaai-onsen`、三档依赖、bin、CI、docs | 可编译空壳 | `cargo check` / `clippy` 绿 | ✅ 2026-10-19 |
| A2 | 数据层：`data/scenario_onsen.json` 自持 + serde schema + 载入校验 | `OnsenData` + 载入器 | 载入/缺字段/长度一致性单测 | ✅ 2026-10-19 |
| A3 | 局面与阶段机：`OnsenGame` + `Stage`(0..4) + 策略流 + 三条轴接线 | 可推进的空局面 | 固定种子跑满 78 回合、两次逐位一致 | ✅ 随 A1 落地 |
| A4 | 训练与挖掘：温泉训练表、上层加成、**失败率下降补实现**、`deyilv`（旅馆）、挖掘力/量/进度/奖励、装备升级、源泉选择 | 核心机制 | 机制单测（见 §1.3 前半，含失败率下降） | ✅ 2026-10-20 |
| A5 | 入浴与超回复：入浴券、温泉效果、锁面板、超回复分档、临时上限 150 | 资源闭环 | 机制单测（票/分档/保底/上限） | ✅ 2026-10-20 |
| A6 | 比赛与基础事件：剧本 `Race`（倍率经 H1）、五维上限注入、继承/切者等引擎既有分支、剧本 5 条固定事件 | 一局可跑完 | 倍率口径单测 + 端到端冒烟 | ✅ 2026-10-24 |
| A7 | 手写训练员（最小版）：守门链（病→治病、比赛回合→比赛、券→入浴、挖掘→顺序表）+ 简化打分表 + 事件选项 | 全决策点覆盖 | 无 dead loop、无 panic、跑满回合 | ✅ 2026-10-24 |
| A8 | 装配与呈现：`build_game`、剧本配置段 `onsen_config.toml`、`OnsenView` 值快照、bin 一局 | 可运行入口 | 端到端 + View 序列化 | ✅ 2026-10-24 |

### M1b：补齐与对齐

| 步 | 内容 | 状态 |
|---|---|---|
| B1 | 女将全套：首次/点击/解锁/出行 5 段/新年/结局事件、出行 5 段券规则、PR 活动 | ✅ 随 A6 落地 |
| B2 | 年末事件与汤浴会奖励（三档）、结局奖励、训练等级提升分支 | ✅ 随 A6 落地 |
| B3 | 剧本链接（5 名角色 +10%）、分身（split）、装备升级推荐表、休息心得 | ✅ 随 A6 落地 |
| B4 | 固定回合特例：解锁/再选择（含同回合连做两轮）、合宿、决赛期、特殊角色 1063 | ✅ 随 A6 落地 |
| B5 | 手写策略全量移植：legacy `HandwrittenEvaluator` 的 4 条守门链 + 打分表 + 一屏超参（含秘汤豁免、超回复调整、挖掘价值、工具表） | ✅ 2026-10-24 |
| B6 | 固定种子基线 JSON（剧本仓自持）、测试一览、机制文档补齐、与旧仓统计口径对照 | ✅ 2026-10-24 |

> 2026-10-24 状态：**M1 全量完成并收口**。A8 落地剧本配置段（`src/config.rs`，H5 覆盖层语义）与
> `OnsenView` 温泉资源全集快照，bin 可切 `onsen` / `random` / `manual` 训练员端到端跑通；B5 全量打分
> 替换简化打分；B6 落 `src/bench.rs` + `baselines/onsen_default.json` 与
> `docs/mechanism.md` / `docs/tests_overview.md`（含与旧仓统计口径对照）。
> 收尾批次：口径修正（分身全卡种 / 非保科友人卡报错，glossary §13.2 #12 / #13）+
> 手动训练员 `ManualTrainer`（feature `manual`，终端交互人工验证）。

***

## 3. 剧本仓形态

**workspace 五 crate，按终端业务形态划分**（2026-10-24 拍板落地，取代 M1 的「单包多 bin」；
原「升格触发条件」被 M2 到来 + 用户拍板覆盖——搜索训练员与在线协议带来分叉的依赖面，
sim 与 ai 业务不会同时进行、cli 与 tools 也是独立业务）。核心 `onsen-core` 为**宿主稳定面**
（机制 + 训练员 + 装配，下游 Web / Android 只依赖它）；各业务 crate 互不依赖（cli 依赖 ai 供
在线外壳）。M2 前已有的能力按此归位：训练员族在 core、基线裁判在 sim、手动训练员在 cli
（crate 边界替代原 feature `manual` 门控）。

```
umaai-onsen/
├─ Cargo.toml            # workspace 根（members + 统一依赖表；引擎 path 依赖只写一处）
├─ crates/
│  ├─ onsen-core/        # 宿主稳定面：game/* + data + config + trainer（手写）+ testing + 装配
│  │                     #   M2 增：MCTS 训练员 + FlatSearchGame 适配器
│  ├─ onsen-sim/         # 模拟器业务：基线裁判 BaselineRecord（src/bench.rs）+ 基线测试；
│  │                     #   批量跑局 / 测卡测量函数后续落位；单 bin 多子命令待业务落地
│  ├─ onsen-ai/          # 在线决策核心（M2 骨架）：onsen 帧段解析 / Session 接线 / 决策循环
│  ├─ onsen-cli/         # 终端前端：bin `onsen`（跑局，切 onsen / random / manual）+ manual.rs；
│  │                     #   M2 增在线外壳 bin `onsen-session`（thin）
│  └─ onsen-tools/       # 外围工具（骨架）：测卡（引擎 deck / arms 内核 + sim 测量函数）+ 复盘；
│                        #   单 bin 多子命令待业务落地
├─ data/                 # 统一数据目录（唯一发布库，拷仓即跑；2026-10-24 第五批定型）
│  ├─ …引擎静态数据拷贝（events / cardDB / umaDB / constants / text_data_dict / manifest）
│  ├─ scenario_onsen.json    # 剧本资产
│  ├─ ai_config.toml         # 引擎配置默认层（卡组带女将）
│  └─ onsen_config.toml      # 剧本专属配置段（源泉顺序等）
├─ baselines/            # 固定种子基线（剧本仓自持，留仓根）
└─ docs/{glossary.md,mechanism.md,plan/m1_plan.md,tests_overview.md}
```

- **重依赖各归其位**（crate 边界替代 feature 门控）：inquire 只在 `onsen-cli`；rayon（搜索 /
  测卡并行）、notify（在线文件通道）待对应业务落地时再进对应 crate；Web / Android 宿主只取
  `onsen-core`（将来连同搜索件），不碰终端与工具面。
- **测试定位**：workspace 化后测试进程 cwd 在 crate 目录，数据根 / 基线路径统一经
  `onsen_core::testing`（`CARGO_MANIFEST_DIR` 上溯两级）取；引擎 `umaai-core::testing` 的
  `env!` 固化引擎仓路径，跨仓不可用。
- **统一数据目录**（2026-10-24 第五批定型）：`data/` 自持引擎静态数据拷贝 + 剧本资产 +
  双配置默认层，bin 数据根为代码级常量（不暴露命令行）；引擎覆盖层约定保留（工作目录同名
  文件存在才合并）。引擎数据升级时重拷并重录基线。
- **数据与配置由宿主注入**：引擎数据根（`gamedata/`）与剧本数据各自独立；剧本侧只接根目录参数。
- **阶段自持**：内部强类型枚举，对外只投影 `StageId`（显式赋号 0..4，改号＝旧种子失效）。
- **三轴随机**：局面流 / 事件流取自 `BaseGame`，策略流剧本自持（每回合按 `(master, turn)` 派生）。
- 数据来源用 `umaai_core::DataSource`（目录 / 内存两源），便于测试注入内存数据。

***

## 4. 引擎钩子清单（M1 的引擎改动，全部带默认实现、非破坏）

> 明细与批注结论见 [`library_candidates.md`](library_candidates.md) §A / §B / §5。
> **`scale` 统一语义**：一律表示**实际系数**（可直接用于数值），不是面板值；支援卡面板加成（`saihou`、`hint_prob_increase`）
> 卡间加算成面板值，剧本机制可再倍乘，之后才得到 `scale`。

| # | 钩子 | 现痛点 | 形态 | 默认行为 |
|---|---|---|---|---|
| H1 | 比赛事件构造（= A1） | 剧本要自算倍率却仍要复制「取 `race_career` / `race_g{grade}` + 决赛期 G1 兜底 + `set_race`」 | `BaseGame::push_race_event(&mut self, scale: f64, pt_rate: f64) -> Result<()>` | 等价现 `BaseAction::do_race`（`scale = (100+race_bonus)/100`、`pt_rate = 1.0`）；温泉生涯赛传乘算后的 `scale`、1063 传 `pt_rate = 0.45` |
| H2 | 训练后默认处理（= A2） | 羁绊 / 红点 / 加练 / 友人点击 / 理事长·记者点击约 100 行只在 `BasicGame`，剧本要复制 | `BaseGame::default_after_train(persons, train, rng)`：AfterTrain 阶段触发内容的默认档（剧本可调用，也可写自己的实现） | `BasicGame` 改调本方法，数值不变（等价重构） |
| H4 | 羁绊出口（= A4） | 旧仓各局面各写一份、trait 从来没有；新仓只在 `BasicGame` 留一份（通用层这份移植时漏了） | `BaseGame::add_friendship(persons, person_index, value)`（爱娇 +2、上限 100、按 `card_id` 同步卡面）；`Person` 补 `set_friendship` | `BasicGame` 改为转调，数值不变 |
| H5 | 配置覆盖层 helper（= B5） | 引擎 `Config` 已有「严格解析 + 段缺失整段沿用」语义，但剧本配置段不进引擎、剧本仓要重写一遍 | `umaai::config` 出泛型 helper（`OverlayConfig` trait + `parse_with_overlay`），引擎 `Config` 改用它 | 引擎 `Config` 行为不变（等价重构），剧本配置段复用同一套语义 |

**不做（本批已定）**：A3 红点分配参数 → 基础公式恒不含剧本修正，剧本改算法时**整体覆写 `distribute_hint`**；A5 事件分派钩子与 A6 回合事件表 → `apply_event` 与回合事件表**由剧本自持**；引擎可选提供「初始特殊事件」可复用件。
A7 友人流程、B2 阈值分档、B3 累计进位、B4 形状校验 → 不做通用件。
**待讨论**：A12 `apply_event` 的 `scale` / `vital_scale` 形态（与 H1 的 `scale` 口径统一，倾向给 `BaseGame::apply_event` 加参数，默认 1.0）。

纪律：每个钩子一条 changelog 记录、补单测、CI 三档全绿；默认行为等价意味着**引擎基线不应位移**，提交说明里点明。

***

## 5. 验证重构的观察点（M1 的真实目的）

| # | 观察点 | 判据 |
|---|---|---|
| 1 | 阶段自持 / `StageId` | 温泉 5 阶段 + 2 个非回合决策点无需向通用枚举硬套（对照 `issues.md#186`） |
| 2 | 三随机轴 | 超回复抽签、PR 客人、分身、事件分布能各归其轴，且每回合派生即可保证配对 |
| 3 | override seam | 训练值上下层正好用 `default_calc_training_value` 复用下层计算 |
| 4 | 注入点 | 五维上限、比赛等级表、数据根、剧本配置全部由外部注入，引擎不硬编码 |
| 5 | 契约缺口的收敛方式 | 4 个钩子（H1 / H2 / H4 / H5）是否够用；hint 三机制（发生率 / 次数 / 等级）是否都找到合适落点；是否需要 `ScenarioRules` / `ScenarioLifecycle`（open_items #10） |
| 6 | View DTO 形态 A | 温泉资源（源泉状态 / 挖掘 / 券 / 效果）能否只靠单一取数口交付给前端 |

***

## 6. M1 不做的事

- 不接搜索 / MCTS（`FlatSearchGame` 留 seam，M2 再实现）；不移植旧仓外挂 CRN（引擎已删，改用回合派生）。
- 不做在线协议（`onsen` 帧段解析 / 回写）与帧驱动会话接线（M2+）。
- 不移植旧训练管线耦合件（`extract_nn_features` 1121 维布局）。
- 不追求与旧仓逐位一致：引擎随机分层已是语义升级，可比的是机制口径与统计量。

***

## 7. 与旧仓的行为差异（预期内位移，比对时注意）

| # | 差异 | 影响 |
|---|---|---|
| 1 | 随机流按引擎三轴重排（旧温泉是唯一未改造剧本） | 同种子数值整体位移；不与旧仓逐位比较，只比统计量 |
| 2 | 基础温泉「失败率下降 20」**补实现**（旧仓该字段未生效） | 失败率整体下降，长局数值上移（2026-10-19 拍板） |
| 3 | 训练后羁绊按引擎修正口径（支援卡 +7 / 理事长·记者 +5 / 爱娇统一 +2）；旧温泉为 +7 / +2 | 羁绊与闪彩节奏略变 |
| 4 | 汤浴会只实现大成功档（数据里的单选项事件） | 不出现「ふつう」档 |
| 5 | 保留项：特殊角色 1063 特例、`link_chara_id` 不使用、超回复红点次数写死 `hotel.hint + 2` | 与旧仓一致 |
| 6 | 非保科友人卡**建局报错**（旧仓降级为 `OtherFriend` 继续跑）；分身门槛「支援卡种类」按全部 7 类卡种统计（修正 A6 期误收窄） | 带非女将友人卡的卡组直接被拒（宿主以覆盖层注入女将卡组）；分身门槛与旧仓一致（2026-10-24 拍板，glossary §13.2 #12 / #13） |

**失败率下降的实现口径**（无需引擎钩子）：`calc_training_buff` 仍只算卡片（供训练值上下层使用）；
在 `do_train` 内把「卡片 buffs + 温泉效果的 `fail_rate_drop`」合并后传 `calc_training_failure_rate`，
合并仅在温泉效果生效期间（`buff_remain_turn > 0`）进行；引擎的失败率公式不动。
