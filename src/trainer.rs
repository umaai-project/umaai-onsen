//! 手写训练员（B5 全量版）：守门链 + 旧仓 `HandwrittenEvaluator` 打分表
//!
//! 移植自旧仓 `HandwrittenEvaluator` / `HandwrittenTrainer`（数值口径以旧仓为准）：
//! 4 条守门链（病 → 治病、比赛回合 → 比赛、券 → 入浴、挖掘 → 顺序表 / 工具表）+
//! 训练 / 休息 / 出行 / 友人出行 / 比赛 / PR 全量打分 + 一屏超参（秘汤豁免 / 超回复
//! 调整 / 挖掘价值）。事件选项沿用 A7 贪心（旧仓恒取首项，本仓打分覆盖同一决策点）。

use anyhow::Result;
use rand::rngs::StdRng;
use umaai_core::gamedata::{EventChoice, EventData};
use umasim::game::{Game, PersonType, Trainer};

use crate::config::OnsenConfig;
use crate::game::{Action, OnsenGame};

// ============================================================================
// 超参（一屏；来源：旧仓 HandwrittenEvaluator，勿顺手"对称化"）
// ============================================================================

/// 属性权重 [速 耐 力 根 智]（速力优先）
const STATUS_WEIGHTS: [f64; 5] = [7.0, 8.0, 8.0, 8.0, 6.0];
/// 没带卡的属性权重
const ABSENT_WEIGHT: f64 = 2.0;
/// 训练类型偏好 [速 耐 力 根 智]
const TRAIN_TYPE_BONUS: [f64; 5] = [20.0, 10.0, 30.0, 30.0, 20.0];
/// 没带卡的训练人头阈值（≥ 阈值 +1 才部分解锁）
const ABSENT_HEAD_THRESHOLD: usize = 2;
/// 前期回合阈值：智力训练只在前期考虑
const EARLY_TURN_THRESHOLD: i32 = 12;
/// 控属性预留空间因子
const RESERVE_STATUS_FACTOR: f64 = 40.0;
/// 羁绊基础价值
const JIBAN_VALUE: f64 = 12.0;
/// 体力价值因子（开局 → 终局线性过渡）
const VITAL_FACTOR_START: f64 = 3.5;
const VITAL_FACTOR_END: f64 = 2.0;
/// 小失败 / 大失败惩罚
const SMALL_FAIL_VALUE: f64 = -1500.0;
const BIG_FAIL_VALUE: f64 = -1800.0;
/// 干劲不满时的出行加成
const OUTGOING_BONUS: f64 = 200.0;
/// 终局事件预估属性加成（URA3 + 最终事件 + URA2 + URA1，旧仓沿用）
const FINAL_BONUS: i32 = 45 + 30 + 20 + 20;
/// 技能点权重
const SKILL_WEIGHT: f64 = 0.5;
/// 彩圈加成分值
const SHINING_BONUS: f64 = 35.0;
/// 用券的体力阈值
const VITAL_THRESHOLD: i32 = 55;

/// 挖通一座源泉的奖励分
const DIG_COMPLETE_BONUS: f64 = 800.0;
/// 每点挖掘量的价值系数
const DIG_BASE_VALUE: f64 = 0.5;
/// 生涯比赛基础价值
const RACE_BASE_BONUS: f64 = 450.0;
/// 非目标比赛基础价值
const NON_TARGET_RACE_BONUS: f64 = 180.0;
/// 自选比赛最大额外价值（/ 剩余回合数）
const FREE_RACE_BONUS: f64 = 2000.0;
/// 狄杜斯角色 ID（生涯比赛额外 ×1.8）
const DIDUS_CHARA_ID: u32 = 1063;

/// 友人出行基础价值
const FRIEND_OUTING_BASE_VALUE: f64 = 400.0;
/// 超回复累计消耗分档（高 / 中 / 低）
const SUPER_COST_HIGH: i32 = 200;
const SUPER_COST_MID: i32 = 100;
const SUPER_COST_LOW: i32 = 60;
/// 友人出行的超回复调整（低累计加 / 中减 / 高减）
const FRIEND_OUTING_SUPER_BONUS: f64 = 200.0;
const FRIEND_OUTING_MID_COST_PENALTY: f64 = 150.0;
const FRIEND_OUTING_HIGH_COST_PENALTY: f64 = 300.0;
/// 友人出行的回合修正（每 8 回合一组）
const FRIEND_OUTING_TURN_MODIFIER: [f64; 10] = [
    -400.0, -400.0, -200.0, 200.0, 0.0, 200.0, 400.0, -400.0, 200.0, -400.0,
];
/// 第一年无券 / 无券且有超回复时的 PR 加分
const PR_SCORE: f64 = 100.0;
const PR_SCORE_SUPER: f64 = 300.0;

/// 装备升级推荐表（按当前源泉索引 − 1 取推荐地层；旧实现口径）
const RECOMMENDED_TOOL_TYPE: [usize; 9] = [0, 1, 2, 0, 0, 1, 1, 2, 2];
/// 秘汤汤驹索引（体力豁免与用券豁免）
const SECRET_ONSEN_INDEX: usize = 8;

/// 手写训练员（B5 全量版）
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnsenTrainer {
    /// 剧本配置（源泉选择顺序等；由宿主注入）
    pub config: OnsenConfig,
}

impl OnsenTrainer {
    /// 以剧本配置构造
    pub fn new(config: OnsenConfig) -> Self {
        Self { config }
    }

    /// 属性收益（带上限软约束与终局预估加成）
    fn calc_status_gain(&self, game: &OnsenGame, train_value: &[i32; 6]) -> f64 {
        let remain_turn = (game.max_turn() - game.turn() - 1).max(0) as f64;
        let total_turn = game.max_turn() as f64;
        let reserve =
            RESERVE_STATUS_FACTOR * remain_turn * (1.0 - remain_turn / (total_turn * 2.0));

        let mut total = 0.0;
        for stat in 0..5 {
            let limit = game.uma().five_status_limit[stat];
            let remain = limit - game.uma().five_status[stat] - FINAL_BONUS;
            let gain = train_value[stat];
            let s0 = status_soft_function(-(remain as f64), reserve);
            let s1 = status_soft_function((gain - remain) as f64, reserve);
            let weight = if game.card_type_count[stat] > 0 {
                STATUS_WEIGHTS[stat]
            } else {
                ABSENT_WEIGHT
            };
            total += weight * (s1 - s0);
        }
        total += train_value[5] as f64 * SKILL_WEIGHT;
        total
    }

    /// 训练分值：属性收益 + 体力段 + 失败惩罚 + 彩圈 + 人头 + 挖掘 + 类型偏好
    fn evaluate_training(&self, game: &OnsenGame, train: usize) -> Result<f64> {
        let skip_vital = should_skip_vital_penalty(game);
        let buffs = game.calc_training_buff(train)?;
        let value = game.calc_training_value(&buffs, train)?;
        let mut score = self.calc_status_gain(game, &value.status_pt);
        let fail_rate = game.calc_training_failure_rate(&buffs, train)? as f64;

        // 体力段评估（秘汤挖通后豁免）
        if !skip_vital {
            let vital_factor = calc_vital_factor(game.turn(), game.max_turn());
            let vital_before = vital_evaluation(game.uma().vital, game.uma().max_vital);
            let vital_after = (game.uma().vital + value.vital).clamp(0, game.uma().max_vital);
            score +=
                vital_factor * (vital_evaluation(vital_after, game.uma().max_vital) - vital_before);
        }

        // 失败惩罚（期望值混合；大失败档按 20 点阈值豁免小失败）
        let big_fail_prob = if fail_rate < 20.0 { 0.0 } else { fail_rate };
        let fail_value_avg =
            0.01 * big_fail_prob * BIG_FAIL_VALUE + (1.0 - 0.01 * big_fail_prob) * SMALL_FAIL_VALUE;
        score = 0.01 * fail_rate * fail_value_avg + (1.0 - 0.01 * fail_rate) * score;

        // 彩圈加成
        let shining_count = game.shining_count(train);
        if shining_count > 0 {
            score *= 1.0 + shining_count as f64 * 0.15;
            score += shining_count as f64 * SHINING_BONUS;
        }

        // 人头价值：友人 / 羁绊 / 红点
        let head_count = game.distribution()[train].len();
        let mut has_friend = false;
        for person_index in &game.distribution()[train] {
            if *person_index < 0 {
                continue;
            }
            let Some(person) = game.persons.get(*person_index as usize) else {
                continue;
            };
            match person.person_type {
                PersonType::ScenarioCard | PersonType::OtherFriend => {
                    has_friend = true;
                    score += if person.friendship < 60 { 100.0 } else { 40.0 };
                }
                PersonType::Card => {
                    if person.friendship < 80 {
                        let jiban_add = (80 - person.friendship).min(7);
                        score += jiban_add as f64 * JIBAN_VALUE;
                        if has_friend {
                            score += 2.0 * JIBAN_VALUE;
                        }
                    }
                    if person.is_hint {
                        score += STATUS_WEIGHTS.iter().sum::<f64>() * 1.6;
                    }
                }
                _ => {}
            }
        }

        // 温泉效果期间的训练加成
        if game.bathing.buff_remain_turn > 0 {
            score *= 1.15;
        }

        // 挖掘价值
        score += self.evaluate_dig_value(game, &Action::Train(train as i32));

        // 训练类型偏好
        score += TRAIN_TYPE_BONUS[train];

        // 没带卡的训练：默认劣势，仅极端情况解锁
        if game.card_type_count[train] == 0 {
            let other_max_head = (0..4)
                .map(|t| game.distribution()[t].len())
                .max()
                .unwrap_or(0);
            // 前期智力攒羁绊（其他训练完全没人头）
            if game.turn() < EARLY_TURN_THRESHOLD
                && train == 4
                && head_count >= 3
                && other_max_head <= 1
            {
                score += 120.0;
            }
            // 超多人头部分解锁
            if head_count > ABSENT_HEAD_THRESHOLD {
                score += 80.0;
            }
            // 其他训练完全没人头时被迫选择
            if other_max_head <= 1 && head_count >= 3 {
                score += 80.0;
            }
        }
        Ok(score)
    }

    /// 挖掘价值：挖掘量 × 系数；即将挖通当前源泉时再加完成奖励
    fn evaluate_dig_value(&self, game: &OnsenGame, action: &Action) -> f64 {
        let Some(dig_value) = game.calc_dig_value(action) else {
            return 0.0;
        };
        let total_dig: i32 = dig_value.iter().sum();
        let mut score = total_dig as f64 * DIG_BASE_VALUE;
        let remain = &game.dig_remain[game.current_onsen];
        let total_remain: i32 = remain.iter().sum();
        if total_dig >= total_remain && total_remain > 0 {
            score += DIG_COMPLETE_BONUS;
        }
        score
    }

    /// 休息分值：体力段评估（+50）+ 挖掘价值（秘汤挖通后体力不再重要）
    fn evaluate_sleep(&self, game: &OnsenGame, skip_vital: bool) -> f64 {
        let mut value = if skip_vital {
            -100.0
        } else {
            let vital_factor = calc_vital_factor(game.turn(), game.max_turn());
            let vital_before = vital_evaluation(game.uma().vital, game.uma().max_vital);
            let vital_after = (game.uma().vital + 50).min(game.uma().max_vital);
            vital_factor * (vital_evaluation(vital_after, game.uma().max_vital) - vital_before)
        };
        value += self.evaluate_dig_value(game, &Action::Sleep);
        value
    }

    /// 普通出行分值：体力段评估（+10）+ 干劲补 + 挖掘价值
    fn evaluate_normal_outing(&self, game: &OnsenGame, skip_vital: bool) -> f64 {
        let mut value = if skip_vital {
            -50.0
        } else {
            let vital_factor = calc_vital_factor(game.turn(), game.max_turn());
            let vital_before = vital_evaluation(game.uma().vital, game.uma().max_vital);
            let vital_after = (game.uma().vital + 10).min(game.uma().max_vital);
            vital_factor * (vital_evaluation(vital_after, game.uma().max_vital) - vital_before)
        };
        if game.uma().motivation < 5 {
            value += OUTGOING_BONUS;
        }
        value += self.evaluate_dig_value(game, &Action::NormalOuting);
        value
    }

    /// 友人出行分值：基础 + 回合修正 + 干劲补 + 超回复调整 + 挖掘价值
    fn evaluate_friend_outing(&self, game: &OnsenGame, skip_vital: bool) -> f64 {
        let mut value =
            FRIEND_OUTING_BASE_VALUE + FRIEND_OUTING_TURN_MODIFIER[(game.turn() / 8) as usize];
        if game.uma().motivation < 5 {
            value += OUTGOING_BONUS;
        }
        if !skip_vital {
            value += self.calc_friend_outing_super_adjustment(game);
        }
        value += self.evaluate_dig_value(game, &Action::FriendOuting);
        value
    }

    /// 友人出行的超回复调整：低累计加（快速获得）、中高累计减（避免浪费自然触发）
    fn calc_friend_outing_super_adjustment(&self, game: &OnsenGame) -> f64 {
        if game.bathing.is_super_ready {
            return 0.0;
        }
        let vital_cost = game.dig_vital_cost;
        if vital_cost >= SUPER_COST_HIGH {
            -FRIEND_OUTING_HIGH_COST_PENALTY
        } else if vital_cost >= SUPER_COST_MID {
            -FRIEND_OUTING_MID_COST_PENALTY
        } else if vital_cost < SUPER_COST_LOW {
            FRIEND_OUTING_SUPER_BONUS
        } else {
            0.0
        }
    }

    /// 比赛分值：生涯赛吃剧本倍率与源泉加成；自选赛按剩余次数与窗口折算
    ///
    /// 注：源泉加成直接读锁面板（旧仓口径）；实际结算只在效果生效期间计入（见
    /// `Action::Race`），本打分的偏差仅影响排序、不影响结算数值。
    fn evaluate_race(&self, game: &OnsenGame) -> f64 {
        let mut value = if game.uma().is_race_turn(game.turn()) {
            let career_bonus = (100 + game.onsen_buff.career_race_bonus) as f64 / 100.0;
            let mut race_value = RACE_BASE_BONUS * game.data.career_race_multiplier * career_bonus;
            if game.uma().chara_id() == DIDUS_CHARA_ID {
                race_value *= 1.8;
            }
            race_value
        } else if let Some(free) = game.uma().find_free_race(game.turn()) {
            let count = game.uma().count_free_race(free);
            let remain_turn = (free.end_turn as i32 - game.turn() + 1).max(1);
            if count < free.count {
                let race_grade = game.base.race_grade().unwrap_or(0);
                let grade_ok = free.grade.is_none_or(|grade| race_grade <= grade as i32);
                if grade_ok {
                    RACE_BASE_BONUS * (free.count - count) as f64
                        + FREE_RACE_BONUS / remain_turn as f64
                } else {
                    NON_TARGET_RACE_BONUS
                }
            } else {
                NON_TARGET_RACE_BONUS
            }
        } else {
            NON_TARGET_RACE_BONUS
        };
        value += self.evaluate_dig_value(game, &Action::Race);
        value
    }

    /// PR 分值：第一年累计消耗越多越值得；无券时再加一档
    fn evaluate_pr(&self, game: &OnsenGame) -> f64 {
        let mut value = if game.turn() < 24 {
            game.dig_vital_cost as f64 * 1.2
        } else {
            -1000.0
        };
        if game.bathing.ticket_num == 0 {
            value += if game.bathing.is_super_ready {
                PR_SCORE_SUPER
            } else {
                PR_SCORE
            };
        }
        value += self.evaluate_dig_value(game, &Action::PR);
        value
    }

    /// 用券判定：超回复就绪必用；券 ≥2、体力低于阈值或已挖秘汤时用
    fn should_use_ticket(&self, game: &OnsenGame) -> bool {
        if game.bathing.ticket_num == 0 || game.bathing.buff_remain_turn > 0 {
            return false;
        }
        if game.bathing.is_super_ready {
            return true;
        }
        game.bathing.ticket_num >= 2
            || game.uma().vital < VITAL_THRESHOLD
            || should_skip_vital_penalty(game)
    }

    /// 源泉选择：按年份顺序表取第一个在候选里的；偏离表时取候选首项（旧口径）
    fn select_onsen_index(&self, game: &OnsenGame, actions: &[Action]) -> Result<usize> {
        let year = (game.turn() / 24 + 1).min(3);
        let order = self.config.onsen_order.year(year)?;
        for want in order {
            if let Some(index) = actions
                .iter()
                .position(|action| matches!(action, Action::Dig(dig) if dig == want))
            {
                return Ok(index);
            }
        }
        Ok(0)
    }

    /// 装备升级：按当前源泉取推荐地层；不在候选时取首项（旧口径）
    fn select_upgrade_index(&self, game: &OnsenGame, actions: &[Action]) -> usize {
        let index = game
            .current_onsen
            .saturating_sub(1)
            .min(RECOMMENDED_TOOL_TYPE.len() - 1);
        let recommended = RECOMMENDED_TOOL_TYPE[index];
        actions
            .iter()
            .position(
                |action| matches!(action, Action::Upgrade(dig_type) if *dig_type == recommended),
            )
            .unwrap_or(0)
    }

    /// 事件选项打分：五维 + PT + 体力 + 羁绊 + 干劲（贪心取最大）
    fn choice_score(choice: &[EventChoice]) -> f64 {
        let mut score = 0.0;
        for row in choice {
            let value = &row.value;
            score += value.status_pt[..5]
                .iter()
                .map(|point| f64::from(*point))
                .sum::<f64>();
            score += f64::from(value.status_pt[5]) * 0.3;
            score += f64::from(value.vital) * 0.5;
            score += f64::from(value.friendship) * 0.2;
            score += f64::from(value.motivation) * 10.0;
        }
        score
    }
}

impl Default for OnsenTrainer {
    /// 默认层配置（与 `config/onsen_config.toml` 同值；未注入配置的宿主 / 测试兜底）
    fn default() -> Self {
        Self::new(OnsenConfig::default())
    }
}

impl Trainer<OnsenGame> for OnsenTrainer {
    fn select_action(
        &self,
        game: &OnsenGame,
        actions: &[Action],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        if actions.len() <= 1 {
            return Ok(0);
        }
        // 守门 ③：入浴决策（候选只有用券 / 不用券）
        if actions
            .iter()
            .any(|action| matches!(action, Action::UseTicket(_)))
        {
            let use_ticket = self.should_use_ticket(game);
            return Ok(actions
                .iter()
                .position(|action| *action == Action::UseTicket(use_ticket))
                .unwrap_or(0));
        }
        // 守门 ④：装备升级（推荐表按当前源泉取）
        if actions
            .iter()
            .any(|action| matches!(action, Action::Upgrade(_)))
        {
            return Ok(self.select_upgrade_index(game, actions));
        }
        if actions
            .iter()
            .any(|action| matches!(action, Action::Dig(_)))
        {
            return self.select_onsen_index(game, actions);
        }
        // 守门 ①：有病先治病
        if !game.uma().flags.ill.is_empty()
            && let Some(index) = actions.iter().position(|action| *action == Action::Clinic)
        {
            return Ok(index);
        }
        // 守门 ②：生涯赛回合必比赛
        if game.uma().is_race_turn(game.turn())
            && let Some(index) = actions.iter().position(|action| *action == Action::Race)
        {
            return Ok(index);
        }
        // 打分表（全 NaN 时退化为首项；旧仓的随机兜底为不可达分支，略）
        let skip_vital = should_skip_vital_penalty(game);
        let mut best = 0;
        let mut best_value = f64::NEG_INFINITY;
        for (index, action) in actions.iter().enumerate() {
            let value = match action {
                Action::Train(train) => self.evaluate_training(game, *train as usize)?,
                Action::Sleep => self.evaluate_sleep(game, skip_vital),
                Action::NormalOuting => self.evaluate_normal_outing(game, skip_vital),
                Action::FriendOuting => self.evaluate_friend_outing(game, skip_vital),
                Action::Race => self.evaluate_race(game),
                Action::PR => self.evaluate_pr(game),
                _ => -1000.0,
            };
            if value > best_value {
                best_value = value;
                best = index;
            }
        }
        Ok(best)
    }

    fn select_choice(
        &self,
        _game: &OnsenGame,
        choices: &[Vec<EventChoice>],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        if choices.is_empty() {
            return Ok(0);
        }
        let mut best = 0;
        for (index, choice) in choices.iter().enumerate() {
            if index == 0 || Self::choice_score(choice) > Self::choice_score(&choices[best]) {
                best = index;
            }
        }
        Ok(best)
    }

    fn select_event_choice(
        &self,
        game: &OnsenGame,
        _event: &EventData,
        choices: &[Vec<EventChoice>],
        rng: &mut StdRng,
    ) -> Result<usize> {
        self.select_choice(game, choices, rng)
    }
}

/// 属性上限软约束（越接近上限，收益折减越快）
fn status_soft_function(x: f64, reserve: f64) -> f64 {
    if reserve <= 0.0 {
        return x.min(0.0);
    }
    let reserve_inv_x2 = 1.0 / (2.0 * reserve);
    if x >= 0.0 {
        0.0
    } else if x > -reserve {
        -x * x * reserve_inv_x2 * 0.5
    } else {
        x + 0.25 * reserve
    }
}

/// 体力分段评估（50 / 70 / 上限三段，斜率递减）
fn vital_evaluation(vital: i32, max_vital: i32) -> f64 {
    if vital <= 50 {
        2.0 * vital as f64
    } else if vital <= 70 {
        1.5 * (vital - 50) as f64 + vital_evaluation(50, max_vital)
    } else if vital <= max_vital {
        (vital - 70) as f64 + vital_evaluation(70, max_vital)
    } else {
        vital_evaluation(max_vital, max_vital)
    }
}

/// 当前回合的体力价值因子（开局 3.5 线性过渡到终局 2.0）
fn calc_vital_factor(turn: i32, max_turn: i32) -> f64 {
    VITAL_FACTOR_START + (turn as f64 / max_turn as f64) * (VITAL_FACTOR_END - VITAL_FACTOR_START)
}

/// 秘汤豁免：挖通秘汤汤驹后温泉回复可覆盖体力消耗，体力管理不再重要
fn should_skip_vital_penalty(game: &OnsenGame) -> bool {
    game.onsen_state
        .get(SECRET_ONSEN_INDEX)
        .copied()
        .unwrap_or(false)
}
