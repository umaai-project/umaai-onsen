//! 手写训练员（A7 最小版）：守门链 + 简化打分表
//!
//! 目标不是逼近旧仓 `HandwrittenEvaluator` 的全量打分（B5 再移植），而是让 5 类决策点
//! （源泉选择 / 装备升级 / 入浴用券 / 训练·比赛·休息·出行·PR / 事件选项）都有确定性的
//! 手写策略，便于先跑通机制与端到端回归。
//!
//! 守门链（按阶段短路）：
//! 1. 有病 → 治病；
//! 2. 生涯赛回合 → 比赛；
//! 3. 入浴决策 → 超回复就绪或体力偏低时用券；
//! 4. 挖掘选择 → 装备升级推荐表 / 源泉按解锁顺序；
//! 5. 其余主动作 → 简化打分表（训练值 + PT + 失败率惩罚，休息/出行/PR 固定分）。
//!
//! 事件选项：按选项数值（五维 + PT + 体力 + 羁绊 + 干劲）线性加权的贪心。

use anyhow::Result;
use rand::rngs::StdRng;
use umaai_core::gamedata::EventChoice;
use umaai_core::gamedata::EventData;
use umasim::game::{Game, Trainer};

use crate::game::{Action, OnsenGame};

/// 装备升级推荐表（按回合档取推荐地层；旧实现口径）
///
/// 表值 `0/1/2` 分别对应砂 / 土 / 岩；推荐地层满级时退化为当前最低等级的地层。
const RECOMMENDED_TOOL_TYPE: [usize; 9] = [0, 1, 2, 0, 0, 1, 1, 2, 2];

/// 手写训练员（最小版）
#[derive(Debug, Clone, Copy, Default)]
pub struct OnsenTrainer;

impl OnsenTrainer {
    /// 训练/比赛等主动作的简化打分
    fn action_score(game: &OnsenGame, action: &Action) -> Result<f64> {
        let score = match action {
            Action::Train(train) => Self::train_score(game, *train as usize)?,
            Action::Race => {
                if game.uma().is_race_turn(game.turn()) {
                    // 生涯赛：必打
                    1000.0
                } else {
                    // 自选比赛：仅作为低优先候选（A7 不主动选）
                    10.0
                }
            }
            Action::Sleep => {
                let vital = game.uma().vital;
                if vital < 30 {
                    400.0
                } else if vital < 50 {
                    60.0
                } else {
                    0.0
                }
            }
            Action::PR => {
                if (2..72).contains(&game.turn()) {
                    // 券越少越值得 PR（PR 发 1 张券）
                    30.0 + f64::from(3 - game.bathing.ticket_num) * 5.0
                } else {
                    0.0
                }
            }
            Action::NormalOuting => {
                if game.uma().motivation < 5 {
                    50.0
                } else {
                    5.0
                }
            }
            // 友人出行：解锁后尽量走完 5 段（每段发券 + 超回复）
            Action::FriendOuting => 40.0,
            // 兜底：非主动作（用券 / 挖掘 / 升级）不走打分表
            _ => 0.0,
        };
        Ok(score)
    }

    /// 单个训练项的简化分值：五维和 + PT 折半 + 体力项 − 失败率惩罚
    fn train_score(game: &OnsenGame, train: usize) -> Result<f64> {
        let buffs = game.calc_training_buff(train)?;
        let value = game.calc_training_value(&buffs, train)?;
        let mut score = value.status_pt[..5]
            .iter()
            .map(|point| f64::from(*point))
            .sum::<f64>();
        score += f64::from(value.status_pt[5]) * 0.5;
        // 体力项：消耗为负，按 0.2 折算（体力越省越优）
        score += f64::from(value.vital) * 0.2;
        let failure = game.calc_training_failure_rate(&buffs, train)? as f64 / 100.0;
        score -= failure * 40.0;
        Ok(score)
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

    /// 取最大分对应的下标（同分取靠前者）
    fn argmax(scores: &[f64]) -> usize {
        let mut best = 0;
        for (index, score) in scores.iter().enumerate() {
            if *score > scores[best] {
                best = index;
            }
        }
        best
    }

    /// 入浴决策：超回复就绪、体力偏低或用券后能刷新面板时用券
    fn choose_ticket(game: &OnsenGame, actions: &[Action]) -> usize {
        let use_ticket = game.bathing.is_super_ready || game.uma().vital < 70;
        actions
            .iter()
            .position(|action| *action == Action::UseTicket(use_ticket))
            .unwrap_or(0)
    }

    /// 装备升级：按推荐表取地层，推荐项已满级时取等级最低的一项
    fn choose_upgrade(game: &OnsenGame, actions: &[Action]) -> usize {
        let recommended = RECOMMENDED_TOOL_TYPE[game.turn().unsigned_abs().min(8) as usize];
        let mut best: Option<(usize, i32)> = None;
        for (index, action) in actions.iter().enumerate() {
            let Action::Upgrade(dig_type) = action else {
                continue;
            };
            if *dig_type == recommended && game.dig_level[*dig_type] < 6 {
                return index;
            }
            let level = game.dig_level[*dig_type];
            if best.is_none_or(|(_, best_level)| level < best_level) {
                best = Some((index, level));
            }
        }
        best.map_or(0, |(index, _)| index)
    }
}

impl Trainer<OnsenGame> for OnsenTrainer {
    fn select_action(
        &self,
        game: &OnsenGame,
        actions: &[Action],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        if actions.is_empty() {
            return Ok(0);
        }
        // 1. 入浴决策：候选只有用券 / 不用券
        if actions
            .iter()
            .all(|action| matches!(action, Action::UseTicket(_)))
        {
            return Ok(Self::choose_ticket(game, actions));
        }
        // 2. 挖掘完成后的选择流程：候选只有源泉或只有装备
        if actions
            .iter()
            .any(|action| matches!(action, Action::Upgrade(_)))
        {
            return Ok(Self::choose_upgrade(game, actions));
        }
        if actions
            .iter()
            .any(|action| matches!(action, Action::Dig(_)))
        {
            // 源泉按解锁顺序的列表顺序取第一个（旧仓顺序表在 B5 接入）
            return Ok(0);
        }
        // 3. 守门：有病先治病
        if let Some(index) = actions.iter().position(|action| *action == Action::Clinic) {
            return Ok(index);
        }
        // 4. 守门：生涯赛回合必比赛
        if game.uma().is_race_turn(game.turn())
            && let Some(index) = actions.iter().position(|action| *action == Action::Race)
        {
            return Ok(index);
        }
        // 5. 简化打分
        let mut scores = Vec::with_capacity(actions.len());
        for action in actions {
            scores.push(Self::action_score(game, action)?);
        }
        Ok(Self::argmax(&scores))
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
        let scores: Vec<f64> = choices
            .iter()
            .map(|choice| Self::choice_score(choice))
            .collect();
        Ok(Self::argmax(&scores))
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
