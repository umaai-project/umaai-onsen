use anyhow::{Result, anyhow};
use rand::rngs::StdRng;
use umaai_core::Data;
use umaai_core::rng::{StrategyRng, StreamTag, derive_seed};
use umasim::game::base::RuleStreams;
use umasim::game::{FriendOutState, Game, StageId, Trainer};

use crate::game::OnsenGame;

/// 剧本阶段（内部强类型；对外只投影 [`StageId`]）
///
/// 纪律：**无数据、无引用、可 `Copy`**；中间状态（挖到哪座泉、手握几张券）放局面字段。
/// 源泉选择 / 装备升级**不是阶段**：它们在回合开始的 `pending` 流程里触发（与旧实现一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Stage {
    /// 回合开始：事件链、发券、解锁与再选择
    #[default]
    Begin,
    /// 人头分配
    Distribute,
    /// 入浴决策（用券 / 不用券）
    Bathing,
    /// 训练 / 比赛等主动作
    Train,
    /// 回合后事件结算
    AfterTrain,
}

impl Stage {
    /// 下一个阶段（`None` = 本回合走完）
    pub(crate) fn next(self) -> Option<Self> {
        match self {
            Self::Begin => Some(Self::Distribute),
            Self::Distribute => Some(Self::Bathing),
            Self::Bathing => Some(Self::Train),
            Self::Train => Some(Self::AfterTrain),
            Self::AfterTrain => None,
        }
    }
}

impl From<Stage> for StageId {
    /// 显式赋号：**号是稳定契约**，改号等于旧种子与基线失效
    fn from(stage: Stage) -> Self {
        Self::new(match stage {
            Stage::Begin => 0,
            Stage::Distribute => 1,
            Stage::Bathing => 2,
            Stage::Train => 3,
            Stage::AfterTrain => 4,
        })
    }
}

/// 入浴状态：券与温泉效果剩余回合
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BathingInfo {
    /// 温泉券数量（上限 3）
    pub ticket_num: i32,
    /// 温泉效果剩余回合
    pub buff_remain_turn: i32,
    /// 本次入浴是否为超回复
    pub is_super: bool,
    /// 下次入浴是否超回复
    pub is_super_ready: bool,
}

impl OnsenGame {
    /// 派生本局面自持的策略流（注入点与回合边界调用）
    pub(crate) fn reset_strategy_stream(&mut self) {
        let turn = self.base.turn as u64;
        self.strategy = self.base.master().map(|master| {
            StrategyRng::new(derive_seed(master, &[turn, StreamTag::Strategy.tag()]))
        });
    }

    /// 单阶段推进
    pub(crate) fn stage_body<T: Trainer<Self>>(
        &mut self,
        trainer: &T,
        decision: &mut StdRng,
        streams: &mut RuleStreams,
        strategy: &mut StrategyRng,
    ) -> Result<()> {
        match self.stage {
            Stage::Begin => {
                // 挖掘完成（上一回合）→ 装备升级 + 源泉选择
                if self.pending_selection && self.turn < 72 {
                    self.handle_pending_selection(trainer, decision, strategy)?;
                }
                // 开局第二回合：基础温泉视为已完成，先走一轮选择流程
                if self.turn == 2 && self.current_onsen == 0 && self.onsen_state[0] {
                    self.pending_selection = true;
                    self.handle_pending_selection(trainer, decision, strategy)?;
                }
                // 新温泉解锁回合（年初）：再次进入选择流程（与上一轮可同回合连做两轮）
                if [24, 48, 65].contains(&self.turn) {
                    self.pending_selection = true;
                    self.handle_pending_selection(trainer, decision, strategy)?;
                }
                // 固定回合事件（引擎通用 + 温泉剧本）+ 友人出门 / 一般随机事件
                let mut events = self.generate_onsen_events(&mut streams.event);
                // 女将新年事件（第 24 回合，已解锁才触发）与结局事件（第 77 回合，训练后结算）
                if self.friend.out_state == FriendOutState::AfterUnlock {
                    if self.turn == 24 {
                        events.push(Data::get()?.events.friend_events["newyear"].clone());
                    } else if self.turn == 77 {
                        self.unresolved_events
                            .push(Data::get()?.events.friend_events["end"].clone());
                    }
                }
                // 抽签（第 48 回合）与育成结束事件（第 77 回合）在训练后结算
                if self.turn == 48 {
                    self.unresolved_events
                        .push(Data::get()?.system_event("ticket")?.clone());
                }
                if self.turn == 77 {
                    self.unresolved_events
                        .push(Data::get()?.system_event("ending")?.clone());
                }
                // 开局 / 合宿送券（2 / 36 / 60 回合）
                if [2, 36, 60].contains(&self.turn) {
                    self.add_ticket(self.grant_ticket_num());
                }
                // 休息心得：每回合开始体力 +5
                self.update_refresh_mind(&mut streams.event);
                for event in &events {
                    self.run_event(event, trainer, decision, &mut streams.event)?;
                }
            }
            Stage::Distribute => {
                // 温泉效果面板 / 旅馆档 / 挖掘力在回合分配前刷新（面板本身锁到用券为止）
                self.update_scenario_buff(false);
                if self.base.uma.is_race_turn(self.base.turn) {
                    self.reset_distribution();
                } else {
                    self.distribute_all(&mut streams.turn_fixed)?;
                    // 温泉效果期间红点发生率提升（`hint_bonus`）
                    self.distribute_hint(&mut streams.turn_fixed)?;
                    // 秘汤分身：已分配人头再追加一次
                    self.distribute_extra_supports(&mut streams.turn_fixed)?;
                }
            }
            Stage::Bathing => {
                // 有券且效果未生效时才决策（否则本阶段空过）
                let actions = self.list_actions()?;
                if !actions.is_empty() {
                    let selection = trainer.select_action(self, &actions, decision)?;
                    let action = actions
                        .get(selection)
                        .ok_or_else(|| anyhow!("训练员返回的选项越界: {selection}"))?;
                    self.apply_action(action, strategy)?;
                }
            }
            Stage::Train => {
                let actions = self.list_actions()?;
                if !actions.is_empty() {
                    let selection = trainer.select_action(self, &actions, decision)?;
                    let action = actions
                        .get(selection)
                        .ok_or_else(|| anyhow!("训练员返回的选项越界: {selection}"))?;
                    self.apply_action(action, strategy)?;
                }
            }
            Stage::AfterTrain => {
                let after_events = std::mem::take(&mut self.unresolved_events);
                for event in &after_events {
                    self.run_event(event, trainer, decision, &mut streams.event)?;
                }
                // 温泉效果回合计数
                if self.bathing.buff_remain_turn > 0 {
                    self.bathing.buff_remain_turn -= 1;
                    if self.bathing.buff_remain_turn == 0 {
                        self.bathing.is_super = false;
                    }
                }
            }
        }
        Ok(())
    }
}
