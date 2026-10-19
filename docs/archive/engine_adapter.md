> **已存档（2026-10-19），暂不参考。**
> 评估结论：收益主要限于「引擎改名/换路径」类改动，而当前 onsen 直接触达引擎的代码仅 47 行，
> 引入适配层要多维护一层影子 API；在「重构优先、基线随重构重录」的定位下，性价比不明显。
> **重新启用条件**：引擎 review 后机制代码触达引擎的行数显著增长（建议阈值 ≥150 行），
> 或同一处（比赛结算 / 随机派生 / 事件队列）在连续两轮重构里反复改动。
> 关联：[`../plan/engine_coupling.md`](../plan/engine_coupling.md)（耦合面盘点与影响分级，仍在维护）。

# 剧本侧引擎适配层设计（`src/adapter.rs`）

> 定位：**重构优先**前提下，让引擎 review / 机制重写只打在剧本仓的一个文件上。
> 本文说明这层"统一接口文件"的职责边界、能力清单、能挡与不能挡的改动、落地形态与成本。
> 关联：[`engine_coupling.md`](engine_coupling.md)（耦合面盘点与影响分级）。

***

## 1. 一句话

把「和引擎对话」的代码从剧本机制里全部抽出来，收进 `src/adapter.rs`；
机制代码（挖掘 / 入浴 / 超回复 / 阶段机 / 手写策略）只认剧本语言，不认引擎字段。

判据（沿用引擎的归属判据改写）：

> **这行代码是在"和引擎对话"（读底座状态、调引擎入口、取引擎表），还是在"执行温泉规则"？**
> 前者进适配层，后者留在机制层。

***

## 2. 为什么值得做（在"重构优先"下更值）

| 场景 | 没有适配层 | 有适配层 |
|---|---|---|
| 引擎改名 / 换路径（A 级） | 散落 47 行（A4 后预计破百）逐处改 | 只改适配层一处 |
| 引擎给入口**增删参数**（A 级) | 每个调用点都要改 | 适配层内部消化，对外语义签名不变 |
| 引擎**口径**变化（C 级） | 口径散在机制代码里，容易漏改 | 口径在适配层显式写一次（如"自选比赛窗口 = 旧温泉的 `turn > 13`"），review 时一眼可见 |
| 引擎随机派生口径变（E 级） | 派生散落，改造时到处找 | 派生只在适配层的 2~3 个方法里 |
| 阅读剧本机制 | 满屏 `self.base.uma.flags.ill` | 读 `self.is_sick()`、`self.can_use_ticket()` |

额外收益：适配层的方法清单就是**引擎稳定面的真实使用面**，可直接作为引擎 review 的输入
（哪些契约真有人用、哪些没人用 → 反哺 A5 / A6 / A10 / A11 的取舍）。

**它不解决的事**：数值位移（B 级）与随机序列变化（E 级）依然要重录基线——适配层只是让"改哪、为什么改"清楚，不是防震器。

***

## 3. 职责边界

### 3.1 进适配层

1. **底座状态读写**：回合、马娘（五维 / 体力 / 干劲 / flags）、卡组与设施计数、人头分布、友人状态、事件队列。
2. **引擎入口调用**：规则流取出-放回与回合边界派生、比赛事件构造、训练后默认处理、羁绊出口、事件结算。
3. **引擎表读取**：`consts` / `events`（系统事件、友人事件）/ `umaDB` 的取用封装——含"取不到时怎么办"的口径。
4. **宿主注入换算**：`Config` + 剧本配置段 → 剧本自持的运行参数结构。
5. **口径常量**：如自选比赛窗口、温泉效果回合数、券上限——放这里集中可见（便于 review 与后续调整）。

### 3.2 不进适配层（剧本资产）

- 机制算法：挖掘力 / 挖掘量 / 进度奖励、入浴与超回复、旅馆档位、分身、PR、源泉选择策略；
- 阶段机与动作枚举，View 快照，数据 schema，手写训练员，基线测试。

### 3.3 与 `Deref` 的关系

引擎刻意让具体局面经 `Deref` 触达底座（`OnsenGame: Deref<Target = BaseGame>`）。
适配层**不屏蔽** `Deref`，而是约定：**只有 `adapter.rs` 直接用 `self.base.*` / `Deref` 路径**，
其余模块一律走适配层方法。这条可以用一条静态检查守住（见 §6）。

***

## 4. 能力清单（以当前 onsen 实际用到的 16 个底座成员为基线）

形态：`pub(crate) trait BaseExt for OnsenGame`（扩展 trait，不新增类型、不引入包装）

| 分类 | 方法（示意） | 背后引擎调用 | 说明 |
|---|---|---|---|
| 回合 | `turn()` / `is_summer_camp()` | `base.turn` / `base.is_xiahesu()` | 机制层只问"第几回合""是不是合宿" |
| 马娘 | `uma()` / `uma_mut()` / `is_sick()` / `is_aijiao()` | `base.uma*` | 五维读写仍走引擎 `Uma`（值类型） |
| 卡组 / 设施 | `deck_len()` / `card(slot)` / `train_count(train)` | `base.deck` / `base.train_level_count` | 得意率计算所需的卡面读取 |
| 人头 | `distribution_of(train)` / `all_persons()` / `persons_mut()` | `base.distribution` / `self.persons` | 挖掘基础值要吃人头数 |
| 友人 | `friend_unlocked()` / `friend_out_remaining()` | `base.friend` | 女将出行门控 |
| 比赛 | `race_grade_here()` / `self_race_available()` / `push_race(scale, pt_rate)` | `base.race_grade()` / `push_race_event` | **`self_race_available` 里写死旧窗口口径** |
| 事件 | `push_event(e)` / `drain_events()` / `settle_event(e, choice, rng)` / `after_train_default(train, rng)` | `base.unresolved_events` / `apply_event` / `default_after_train` | 机制层不碰事件队列字段 |
| 随机轴 | `master()` / `init_master(m)` / `reset_round_streams()` / `take_streams()` / `put_streams(s)` | `base.*_rule_streams` | **派生口径只此一处** |
| 引擎表 | `consts()` / `system_event(key)` / `event_prob(key)` / `friend_event(key)` | `Data::get()?` | 收口"取不到怎么办" |

规模预期：**18~22 个方法**。超过这个量级说明把机制也包进来了，需要重新划线。

***

## 5. 落地形态（onsen 现状改造）

```
src/
├─ adapter.rs     # 引擎唯一接触面：BaseExt + 引擎表读取 + 口径常量
├─ config.rs      # 宿主注入：引擎 Config + onsen_config.toml → OnsenRuntimeConfig
├─ data.rs        # 剧本资产（已有）
├─ stage.rs       # 阶段机与动作（剧本语言）
├─ dig.rs / bathing.rs / train.rs / hotel.rs   # A4/A5 机制（只调 adapter）
└─ lib.rs         # 局面与契约实现；只做装配与转发
```

- 现有 47 行引擎触达按三类归位：底座状态 → `BaseExt`；引擎表 → 表函数；宿主注入 → `config.rs`。
- **口径集中**：`self_race_available()` 写死"旧温泉窗口 `turn > 13`（比通用规则晚一回合）"，
  `race_scale(...)` 写死"生涯赛 ×1.33 ×（1+源泉加成）、决赛期不乘、1063 特例 PT ×0.45"——
  review 时这两处是最可能被讨论/推翻的地方，集中后改名改值都是一处。
- 契约实现（`impl Game for OnsenGame`）保留在 `lib.rs`，但方法体只做转发。

***

## 6. 守门与验收

1. **静态边界检查**：一条测试读 `src/` 下机制文件，断言不出现 `self.base.` / `self.deck` / `self.uma` 等直连写法
   （允许 `adapter.rs` 白名单）。成本低，能防"越层"回潮。
2. **契约面冒烟**：覆盖 22 个必备方法调用路径的一条测试，引擎改签名时第一时间定位受影响点。
3. **适配层单测**：口径方法（自选比赛窗口、比赛系数组合）直接用构造局面断言，
   把"口径"从机制里解脱出来单独可测。
4. 刷新 [`engine_coupling.md`](engine_coupling.md) §1 的计数（引擎触达行数 / 底座成员清单）。

***

## 7. 成本与时点

- 成本：约半天——抽 47 行 + 21 个方法 + 三条守门，机制代码只改调用形式，不改逻辑。
- **时点：A4 之前**。A4 会新增大量机制代码（挖掘 / 入浴 / 训练上下层），先立适配层可避免这批代码再次直连引擎；
  事后返工要重读并改写这些机制代码，成本翻倍。
- 不做也不会坏，只是 review 期间改动面会随机制增长而扩大——属于"早做便宜、晚做费劲"的投入。
