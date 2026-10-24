# 测试一览（umaai-onsen）

> 约定：单元测试（内联，私有细节 / 纯函数）用 `assert`；集成测试（`tests/`）**不用
> `assert`**，用打印 + `Result` 返回值，失败由 `Err` / 非零退出码判定。workspace 化后
> 测试进程的 cwd 在各 crate 目录：数据根 / 基线路径统一经 `onsen_core::testing`
> （`CARGO_MANIFEST_DIR` 上溯两级定位仓根）取，引擎数据与剧本数据默认同一**统一数据目录
> `data/`**（引擎静态数据已拷入仓），可用 `UMAAI_DATA_ROOT` / `UMAAI_ONSEN_DATA_ROOT`
> 分别覆盖。`cargo test` 统一加 `--offline`，Release 档跑测试与 binary。
>
> 最近整理：2026-10-24（workspace 五 crate 化：core / sim / ai / cli / tools；测试计数不变 42 条）。

## 0. 分 crate

| crate | 测试面 |
|---|---|
| `onsen-core` | 内联单测（data / config）+ `tests/mechanics.rs`（机制 24 条） |
| `onsen-sim` | `tests/baseline.rs`（固定种子基线 2 条） |
| `onsen-cli` | 内联单测（manual 展示行）+ `tests/smoke.rs`（冒烟 4 条） |
| `onsen-ai` / `onsen-tools` | 骨架，暂无测试 |

## 1. 单元测试（内联 `#[cfg(test)]`）

| 文件 | 用例 | 守门内容 |
|---|---|---|
| `crates/onsen-core/src/data.rs` | `test_load_repo_data` | 仓内剧本数据可载入并通过形状校验（10 源泉 / 4 旅馆档 / 5 剧本事件） |
| | `test_unknown_field_rejected` | 未知字段严格报错 |
| | `test_hotel_effect_sorted` | 旅馆档必须按 `onsen_count` 升序 |
| | `test_sum_effects_and_hotel` | 已挖源泉效果求和与旅馆档取值 |
| `crates/onsen-core/src/config.rs` | `test_parse_repo_default` | 默认层可解析且与 `OnsenConfig::default()` 同值 |
| | `test_unknown_key_rejected` | 配置未知键严格报错 |
| | `test_overlay_merge` | 覆盖层段缺失整段沿用、段内字段级覆盖 |
| | `test_order_validation` | 顺序表索引越界 / 重复被拒 |
| | `test_year_out_of_range` | `OnsenOrder::year` 越界报错 |
| `crates/onsen-cli/src/manual.rs` | `test_choice_label` | 事件选项组展示行（档名 / 概率 / 数值拼接） |
| | `test_choice_label_empty_value` | 零数值结果只显示档名（交互本体无自动化测试，靠 bin `manual` 人工验证） |

## 2. `crates/onsen-core/tests/mechanics.rs`（机制单测，24 条）

受控局面 `controlled_game()`：清空链接加成、固定蓝因子，就地改写剧本数据后建局，
不依赖宿主配置的卡组内容。

| 用例 | 守门内容 |
|---|---|
| `training_table_five_levels` | 训练基础值表 5 × 5 档且等级单调不降 |
| `dig_power_three_parts` | 挖掘力三部分（蓝因子 / 属性档位 / 装备）各自生效且与独立重算一致 |
| `dig_value_cross_stratum` | 跨地层：当前层取剩余量整数（不 floor），余量按下一层挖掘力折算 |
| `dig_value_link_bonus_current_layer_only` | 链接加成只作用于当前层，跨层下一层不带 |
| `dig_bonus_every_30_points` | 30 点挖掘奖励每跨一段发一次、跨两段发两次 |
| `hotel_rank_feeds_deyilv` | 旅馆档按已挖源泉数取最高满足档，得意率含旅馆加成 |
| `training_upper_layer_and_fail_rate` | 训练上下层分开截 100；温泉 `fail_rate_drop` 并入后失败率不升 |
| `source_select_and_upgrade_guards` | 源泉未解锁 / 已挖通被拒；装备满级被拒 |
| `ticket_cap_and_grant_turns` | 温泉券上限 3；36 回合开始按友人卡档次发放 |
| `super_prob_tiers_and_floor` | 超回复分档每 50 点一档查表；275 保底 |
| `super_ready_resets_cost` | 触发即就绪并清零累计；已就绪不重复触发 |
| `use_ticket_applies_super_and_cap` | 普通入浴羁绊取最低 3 人 / 体力 / 干劲；超回复临时上限 150 + PT + 红点 |
| `pr_value_ticket_and_dig_row` | PR 入账配置数值、发 1 券、挖掘表按 11 点、消耗并入超回复判定 |
| `hint_bonus_override` | 红点 `hint_bonus` 覆写后 64 个种子全部与独立复现一致 |
| `full_game_reaches_end_turn` | 固定种子跑满 78 回合且至少挖通一座源泉 |
| `race_scenario_multiplier` | 生涯赛吃「剧本倍率 × 源泉加成」、自选赛只吃赛后加成、1063 ×2.7 / PT×0.45 |
| `scenario_fixed_events_by_turn` | 剧本 5 条固定事件在 2 / 24 / 48 / 72 / 77 回合进入事件链 |
| `event_special_cases` | 女将解锁 / 出行 5 段券 / 年末训练等级 / 宴会再发券 |
| `refresh_mind_heals_each_turn` | 休息心得每回合 +5 体力，层数推进或结束 |
| `super_overflow_vital_semantics` | 超上限体力：回体只失效 / 消耗不压回 / 真实上限照常增长 / 临时上限独立 |
| `split_extra_distribution` | 秘汤分身追加分配（≤ split，卡种按全部 7 类统计），效果关闭后不追加 |
| `non_okami_friend_card_rejected` | 非保科友人卡（chara ≠ 9050）建局即报错 |
| `handwritten_trainer_guards` | 守门链：入浴 / 治病 / 生涯赛 / 源泉顺序表 / 装备推荐表 |
| `handwritten_trainer_full_game_deterministic` | 手写训练员端到端跑满回合且同种子逐位一致 |

## 3. `crates/onsen-cli/tests/smoke.rs`（冒烟与可复现性，4 条）

| 用例 | 守门内容 |
|---|---|
| `smoke_runs_full_turn_count` | 经 `Runner` 跑满 78 回合（阶段数 78 × 5 = 390） |
| `smoke_is_deterministic` | 同种子两次运行逐位一致 |
| `smoke_direct_drive_without_runner` | 直连驱动（不经 Runner）同样跑满 |
| `smoke_view_snapshot_roundtrip` | `OnsenView` 快照可 JSON 序列化往返（A8 形态 A 取数口） |

## 4. `crates/onsen-sim/tests/baseline.rs`（固定种子基线，2 条）

| 用例 | 守门内容 |
|---|---|
| `baseline_fixed_seed_matches_record` | 手写训练员 + 统一数据目录默认配置（`data/ai_config.toml` 默认层：卡组塔可 → 女将）跑一局，与 `baselines/onsen_default.json` 逐字段一致；设 `UMAAI_ONSEN_UPDATE_BASELINE=1` 重写基线 |
| `baseline_reproducible` | 同种子两次直连驱动逐位一致 |

## 5. 端到端入口（非测试）

- bin：`cargo run --release --bin onsen -- [训练员]`（onsen-cli crate，从仓根发起），
  训练员 `onsen`（手写策略，缺省）/ `random`（引擎随机基线）/ `manual`（终端交互手动验证
  剧本机制，每次决策前打印局面面板）；终局打印评分 / 五维 / PT。
- 基线与 bin / Runner 路径同种子同序（规则主种子 + 决策流同源播种），逐位可比。
