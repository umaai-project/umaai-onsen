# 可进通用库的机制候选清单（已按用户批注重整）

> 用途：温泉剧本仓实现过程中暴露的「重复件 / 缺口件」，逐项列出**候选机制、归属建议、复用面、现状、结论**。
> 归属判据沿用引擎方案：**进 `umaai-core` 需「跨算法复用且与游戏、与算法无关」**；带行为、带契约的进 `umasim`；
> 引擎改动一律**带默认实现、非破坏**。来源：温泉移植考古（2026-10-19）；批注定稿 2026-10-19。

***

## 0. 统一口径（适用于本文所有 scale / 倍率类参数）

1. **`scale` 一律表示「实际系数」**（已经乘算完毕、可直接用于数值），不是面板值。
2. 支援卡面板加成（`saihou` 赛后、`hint_prob_increase` 等）是**卡与卡之间加算**得到的面板值；
   一部分剧本机制会再对面板值做**倍乘**（如红点率翻倍、生涯赛倍率），之后才得到实际系数。
3. 因此接口上的默认值取「面板值换算后的系数」，剧本提升时传乘算后的结果：
   - 比赛：默认 `scale = (100 + uma.race_bonus) / 100`（`race_bonus` 即 `saihou` 累加）；
     温泉生涯赛传 `scale = 默认 × career_race_multiplier × (1 + career_race_bonus/100)`（决赛期不乘剧本项）。
   - 红点：默认 `scale = 1.0`（`distribute_hint` 内部已含卡面 `hint_prob_increase` 加算）；
     温泉骏闪古泉传 `scale = 2.0`（**发生率**翻倍，不是次数翻倍；见 #4）。

4. **hint 有三种互不相同的机制，实现与接口都不得混用**（2026-10-19 用户提示）：
   - **发生率提升**（`hint_prob_increase` 面板值 / 剧本 `hint_bonus` 加算 → 概率）：决定「这张卡本回合是否出红点」，
     温泉骏闪古泉的 `hint_bonus = 100` 属于这一类（旧实现：`base_hint_rate × (100 + hint_prob_increase + hint_bonus)/100`）。
   - **发生次数**（`hint_count_bonus`）：同一次触发里**重复出 N 个红点事件**（1 + 卡面 `hint_count_bonus`），与概率无关。
   - **红点等级**（`hint_level`）：单个红点提供的技能等级，受 `max_hint_per_card` 单卡上限约束。
   → 因此 A3 的参数语义必须钉死为**发生率系数**（乘在概率上），不得用它表达「次数」或「等级」。

***

## A. 契约钩子（`umasim`，全部带默认实现）

| # | 候选机制 | 现状 / 痛点 | 结论 |
|---|---|---|---|
| A1 | `BaseGame::push_race_event(scale: f64, pt_rate: f64)`：统一「取 `race_career` / `race_g{grade}` + 决赛期 G1 兜底 + 缩放 + `set_race`」 | 现逻辑锁在 `BaseAction::do_race` 内；剧本要自算倍率就得复制事件名与分段逻辑 | **M1 落地**。`scale` 语义见 §0；默认调用方传 `((100+race_bonus)/100, 1.0)`，行为与现状逐位一致。`pt_rate` 供 PT 单独缩放（温泉 1063 特例 PT ×0.45） |
| A2 | `Game::default_after_train(train, rng)`：训练成功后的 AfterTrain 阶段通用处理（羁绊、红点事件、加练、友人点击、理事长 / 记者点击） | 约 100 行只在 `BasicGame` 里；温泉要自写一份，拉面也会要 | **M1 落地**。`BasicGame` 数值必须不变（等价重构） |
| A3 | 红点**发生率系数**：给 `distribute_hint` 加 `scale` 参数（`fn distribute_hint(&mut self, scale: f64, rng)`） | `distribute_hint` 只有「基础率 × 卡面 `hint_prob_increase`」，温泉骏闪古泉 `hint_bonus = 100` 无处注入 | **M1 落地**。`scale` 语义＝**发生率系数**（乘在概率上），默认 `1.0`；温泉传 `2.0`（旧实现等价于 `+100%` 加到 `hint_prob_increase` 面板值）。**明确不用于**表达发生次数（`hint_count_bonus`）与红点等级（`hint_level`），三者见 §0 #4 |
| A4 | `Game::add_friendship(person_index, value)` **提到契约层**（爱娇 +2、上限 100、按 `deck_index_of` 同步卡面） | 旧仓是**各局面各自实现**（`BasicGame` / `OnsenGame` / `RamenState` 各一份），trait 里从来没有；新仓只在 `BasicGame` 保留一份 → 通用层这份在移植时漏了 | **M1 落地**。逻辑沿用新仓 `BasicGame::add_friendship`，`BasicGame` 改为用契约默认实现，数值不变 |
| A5 | 剧本事件分派钩子 `Game::on_special_event(event_id, rng) -> bool` | 旧仓与新仓都在 `apply_event` 里硬编码 id `match`（4012/4013/5007/809050004…） | **改为「剧本自持为主」**（用户批注）：剧本要自己决定是否沿用初始特殊事件，且有自己的特殊事件 → `apply_event` 由剧本实现；引擎不加快照式钩子。**可选轻量件**：把「初始特殊事件」处理抽成 `BaseGame` 上的可复用方法供剧本调用，避免复制 |
| A6 | 回合固定事件表（`newyear` / `end` / `ticket` / `ending` 的硬编码回合）可配置 | 现 `BasicGame::stage_body` 内写死 `turn == 24 / 48 / 77`；温泉年末事件在 24/48/72 | **随 A5 处理**：剧本自持回合事件表；引擎不为此加配置项（避免把剧本节奏塞进通用配置） |
| A7 | 友人（`ScenarioCard`）流程钩子：解锁概率键、出行段数、事件表来源 | `BasicGame` 内一份（`friend_unlock_low/high`、`out_used[5]`、`friend_events`） | **延后**（用户批注）：各剧本友人出现回合与事件效果可能不同、部分友人有特殊效果要处理；等第二个真实消费方再抽 |
| A8 | 基础动作候选集 `Game::base_action_candidates()`：比赛回合唯一、合宿特例、生病加治病、友人出行条件 | 这套条件在 `BasicGame::list_actions` 里，温泉几乎相同（只多 PR + 顺序表） | **后续评估**：属「通用剧本机制」但剧本可能选择性使用；M1b 看重复量再定 |
| A9 | `Game::default_do_train(...)`：失败判定 → 数值 → 训练计数的主体 provided | 与 A2 组合后，剧本只需补「挖掘 / 超回复」等特效 | **与 A2 二选一或组合**，M1b 定（M1 先按「剧本自写 do_train + 复用 A2」实现） |
| A10 | leaf 估值 seam：`Evaluator<G>` / `LeafEvaluator` 关联类型（配 `SUPPORTS_TRUNCATED_LEAF`） | 门禁已在（`max_depth > 0` 硬拒未声明者），估值器属延后项 | **M2**（见搜索笔记） |
| A11 | 搜索「option / 分段」支持：把搜索边界从终局改到剧本决策点（选泉周期） | 无；旧仓只能靠 `max_depth` 与终局 rollout | **先测量后定**（见搜索笔记 §4） |
| A13 | hint **发生次数**（`1 + hint_count_bonus`）与**红点等级**（`hint_level`，受 `max_hint_per_card` 限制）的可复用件 | 旧仓温泉 `do_hint` 与拉面 `action.rs` 各写一份（含次数循环、等级上限、`has_friendship` 时清零羁绊、属性/技能二选一）；新仓只在 `BasicGame::do_train` 内联 | **候选**：温泉 M1 需要（超回复红点循环走它），拉面已有第二份实现 → 建议 M1b 抽成 `BaseGame` 侧可复用方法（不占契约方法名）。**注意它只处理「次数 / 等级」，发生率仍由 A3 负责** |
| A12 | **`apply_event` 的 `scale` / `vital_scale`（讨论项）** | 现 `BaseGame::apply_event` 内部对友人事件应用 `event_bonus` / `vital_bonus`；剧本若自写 `apply_event` 就会丢掉这段，或需复制 | **待讨论**。两个形态：①`BaseGame::apply_event(event, choice, scale, vital_scale, rng)`（默认 1.0，友人加成仍在内部）；②另出可复用件 `apply_event_value(...)` 供剧本组合。倾向 ①，与 A1 的 `scale` 口径统一 |

***

## B. 通用纯函数 / 数据件（`umaai-core` 候选）

| # | 候选机制 | 现状 / 痛点 | 结论 |
|---|---|---|---|
| B1 | 带权分桶采样 `sample_bucket` 提为公共原语 | 现为 `umasim::game::traits` 私有函数（与 `WeightedIndex` 逐位一致）；更细粒度的层级抽签也会需要 | **可**（小、纯函数） |
| B2 | 「阈值表分档」查表原语（阈值 → 档位 → 值，含越界夹取） | 温泉 `super_probs`、属性档位、失败率阈值都是同一个 `while` 递减循环 | **存疑，暂不做**（用户批注）：各剧本阈值表不同，抽出来收益不抵抽象成本；剧本内用 3 行循环即可 |
| B3 | 「累计进度按阈值进位」原语 | 温泉挖掘每 30 点奖励一次（29→65 触发两次） | **剧本专属机制**，不进通用库 |
| B4 | 数据表形状校验 | 旧仓 `try_into().expect()` 运行期 panic | **不做通用件**（用户批注）：剧本用 Rust 基本写法加固即可 |
| B5 | 剧本配置的「严格解析 + 覆盖层合并」复用件 | 引擎 `Config` 已有该语义（未知键报错、段缺失整段沿用），剧本段不进引擎 | **落地**（用户批注「给引擎加一个 pub helper」）：在 `umaai::config` 加泛型 helper（`OverlayConfig` trait + `parse_with_overlay`），引擎 `Config` 改用它，剧本配置段复用同一套语义 |

***

## C. 基建 / 工具面（`umaai` / `umaai-cli` / `umaai-tests` / `umaai-tools`）

| # | 候选机制 | 现状 / 痛点 | 结论 |
|---|---|---|---|
| C1 | baseline / `Checker` 复用 | 剧本仓想复用固定种子基线判定，需依赖 `umaai-tests` | **可接受**（用户批注）：不抽新 crate，**尽可能 pub 化** `umaai-tests` 的 `Checker` / baseline 件；剧本仓按 `rev` 钉住依赖。M1b 做一次 pub 面梳理 |
| C2 | 剧本特化呈现原语 | `umaai-cli::render` 已就位；温泉面板是首个真实消费者 | M1 用最小实现，M1b 归纳（不进稳定面） |
| C3 | 剧本仓能否直接依赖 `umaai-tools`（batch / compare / probe）做测量 | 现为样例面「不承诺稳定、按 `rev` 钉住」 | **待你定**：钉 `rev` 直接用 / 复制内核 / 提升为稳定面 |
| C4 | 「记录每次剧本决策点的候选与后续结果」成通用件 | 无；属测量设施 | **剧本仓自持**，先不通用化 |

***

## D. 不进通用库（留剧本仓）

| # | 机制 | 理由 |
|---|---|---|
| D1 | 挖掘 / 地层 / 装备 / 挖掘力构成 | 与温泉玩法强绑定 |
| D2 | 温泉券 / 温泉效果 / 超回复 / 旅馆 / 分身 | 同上 |
| D3 | 源泉选择顺序表（`onsen_order`）与工具推荐表 | 策略超参，属剧本侧调参面 |
| D4 | `onsen` 帧段解析与回写 | 剧本专属协议段 |
| D5 | 剧本事件表（400012xxx）与温泉常量 | 剧本数据，不进引擎数据根 |
| D6 | 累计进度进位、阈值表分档、数据形状校验 | 见 B2–B4 结论：抽象成本高于收益 |

***

## 5. 决定记录（按落地时点）

| 时点 | 项 | 说明 |
|---|---|---|
| **M1** | A1 / A2 / A3 / A4 + B5 | 五个引擎改动，全部带默认实现、`BasicGame` 数值不变；对应 M1 方案 §4 的 H1–H4 + 配置 helper |
| **M1** | A5 / A6 | 不做引擎钩子：`apply_event` 与回合事件表由剧本自持；引擎侧可选提供「初始特殊事件」可复用件 |
| **M1** | C1 | `umaai-tests` pub 面梳理（不抽新 crate） |
| **M1b** | A8 / A9 / C2 | 按重复代码量评估；A9 与 A2 二选一 |
| **M2** | A10 / A11 / C3 | 随搜索形态一起定 |
| **待定** | A12 | `apply_event` 的 scale / vital_scale 形态（讨论项） |
| **M1b** | A13 | hint 次数 / 等级可复用件（发生率仍归 A3） |
| **不做** | A7 / B2 / B3 / B4 / C4 / D1–D6 | 见上表理由 |
