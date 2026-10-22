use std::fmt::Display;

use anyhow::Result;
use rand::Rng;
use umasim::game::ActionEnum;
use umasim::game::base::BaseAction;

use crate::game::OnsenGame;

/// 剧本动作
///
/// `apply` 只负责把动作落到局面上；取哪条随机流由 `run_stage` 决定（策略流）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// 训练（0-4：速 / 耐 / 力 / 根 / 智）
    Train(i32),
    /// 比赛
    Race,
    /// 休息
    Sleep,
    /// 普通出行
    NormalOuting,
    /// 治病
    Clinic,
    /// 友人出行
    FriendOuting,
    /// PR 活动
    PR,
    /// 使用 / 不使用温泉券
    UseTicket(bool),
    /// 选择源泉（索引 0 基）
    Dig(usize),
    /// 升级挖掘装备（地层 0 砂 / 1 土 / 2 岩）
    Upgrade(usize),
}

impl Display for Action {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Train(i) => write!(f, "训练{i}"),
            Self::Race => write!(f, "比赛"),
            Self::Sleep => write!(f, "休息"),
            Self::NormalOuting => write!(f, "普通出行"),
            Self::Clinic => write!(f, "治病"),
            Self::FriendOuting => write!(f, "友人出行"),
            Self::PR => write!(f, "PR"),
            Self::UseTicket(true) => write!(f, "使用温泉券"),
            Self::UseTicket(false) => write!(f, "不使用温泉券"),
            Self::Dig(i) => write!(f, "选择源泉{i}"),
            Self::Upgrade(i) => write!(f, "升级装备{i}"),
        }
    }
}

impl ActionEnum for Action {
    type Game = OnsenGame;

    fn apply(&self, game: &mut Self::Game, rng: &mut impl Rng) -> Result<()> {
        game.apply_onsen_action(self, rng)
    }
}

impl OnsenGame {
    /// 动作结算总入口（策略流由调用方传入）
    pub(crate) fn apply_onsen_action(&mut self, action: &Action, rng: &mut impl Rng) -> Result<()> {
        match action {
            Action::Train(train) => {
                let (success, vital_cost) = self.do_train(*train as usize, rng)?;
                // 失败由事件链处理，不入挖掘；成功才判超回复并入挖掘
                if success {
                    if vital_cost > 0 {
                        self.update_super_on_vital_cost(vital_cost, rng);
                    }
                    self.run_dig_for(action, rng);
                }
                Ok(())
            }
            Action::Race => {
                // 生涯赛剧本倍率与源泉加成由 A6 补；此处用引擎默认系数
                let scale = (100 + self.base.uma.race_bonus) as f64 / 100.0;
                self.base.push_race_event(scale, 1.0)?;
                // 目标比赛 25 点、自选比赛 15 点
                self.run_dig_for(action, rng);
                Ok(())
            }
            Action::Sleep => {
                BaseAction::do_sleep(&mut self.base, rng)?;
                self.run_dig_for(action, rng);
                Ok(())
            }
            Action::NormalOuting => {
                BaseAction::do_normal_outing(&mut self.base, rng)?;
                self.run_dig_for(action, rng);
                Ok(())
            }
            Action::Clinic => {
                BaseAction::do_clinic(&mut self.base, rng)?;
                // 治病基础挖掘量为 0（仍需走一遍以结算进度）
                self.run_dig_for(action, rng);
                Ok(())
            }
            Action::FriendOuting => {
                BaseAction::do_friend_outing(&mut self.base, rng)?;
                self.run_dig_for(action, rng);
                Ok(())
            }
            Action::PR => {
                let vital_cost = self.do_pr(rng)?;
                self.update_super_on_vital_cost(vital_cost, rng);
                // PR 的挖掘表行为 11 点
                self.run_dig_for(action, rng);
                Ok(())
            }
            // 使用温泉券入浴（不用券时本阶段空过）
            Action::UseTicket(use_ticket) => {
                if *use_ticket {
                    self.do_use_ticket(rng)?;
                }
                Ok(())
            }
            Action::Dig(index) => self.do_select_dig(*index),
            Action::Upgrade(dig_type) => self.do_upgrade_equipment(*dig_type),
        }
    }
}
