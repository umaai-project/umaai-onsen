use anyhow::{Result, anyhow};
use rand::Rng;
use rand::rngs::StdRng;
use umaai_core::Data;
use umaai_core::gamedata::{ActionValue, EventData};
use umasim::game::base::BasePerson;
use umasim::game::{
    CardTrainingEffect, Game, Person, PersonType, StageId, SupportCard, Trainer,
    TrainingBasicTable, TrainingValueSource, Uma,
};

use crate::MAX_TURN;
use crate::game::stage::Stage;
use crate::game::{Action, OnsenGame};

impl Game for OnsenGame {
    type Person = BasePerson;
    type Action = Action;

    fn turn(&self) -> i32 {
        self.base.turn
    }

    fn max_turn(&self) -> i32 {
        MAX_TURN
    }

    fn rule_master(&self) -> Option<u64> {
        self.base.master()
    }

    fn set_rule_master(&mut self, master: u64) {
        // 引擎两轴 + 剧本自持的策略轴一起重置（底座入口叫 init_rule_streams，不与契约同名）
        self.base.init_rule_streams(master);
        self.reset_strategy_stream();
    }

    fn stage(&self) -> StageId {
        self.stage.into()
    }

    fn next(&mut self) -> bool {
        if let Some(stage) = self.stage.next() {
            self.stage = stage;
        } else if self.turn < self.max_turn() {
            self.turn += 1;
            self.stage = Stage::Begin;
            // 新回合：两条引擎流与自持策略流按 `(master, 新回合)` 重新派生（CRN 配对来源）
            self.base.reset_rule_streams();
            self.reset_strategy_stream();
            return self.check_free_race();
        } else {
            return false;
        }
        true
    }

    fn run_stage<T: Trainer<Self>>(&mut self, trainer: &T, decision: &mut StdRng) -> Result<()> {
        // 三条轴各就各位：局面流 / 事件流从底座取出，策略流本局面自持，决策流只给 Trainer
        let mut streams = self.base.take_rule_streams()?;
        let Some(mut strategy) = self.strategy else {
            return Err(anyhow!(
                "策略流缺失：主种子已注入但策略流未派生，应先调用 set_rule_master"
            ));
        };
        let res = self.stage_body(trainer, decision, &mut streams, &mut strategy);
        self.base.put_rule_streams(streams);
        self.strategy = Some(strategy);
        res
    }

    fn list_actions(&self) -> Result<Vec<Self::Action>> {
        match self.stage {
            Stage::Bathing => {
                if self.bathing.ticket_num > 0
                    && self.bathing.buff_remain_turn == 0
                    && self.turn >= 2
                {
                    Ok(vec![Action::UseTicket(true), Action::UseTicket(false)])
                } else {
                    Ok(vec![])
                }
            }
            Stage::Train => {
                if self.base.uma.is_race_turn(self.base.turn) {
                    return Ok(vec![Action::Race]);
                }
                let mut actions: Vec<Action> = (0..5).map(Action::Train).collect();
                if self.base.is_xiahesu() {
                    actions.push(Action::Race);
                    actions.push(Action::Sleep);
                    return Ok(actions);
                }
                if (2..72).contains(&self.turn) {
                    actions.push(Action::PR);
                }
                actions.push(Action::Sleep);
                actions.push(Action::NormalOuting);
                // 自选比赛：沿用旧温泉窗口（turn > 13 起）；等级表为 0 的回合无赛事
                if self.turn > 13 && self.base.race_grade().is_some() {
                    actions.push(Action::Race);
                }
                if self.base.uma.flags.ill != umaai_core::Ill::NONE {
                    actions.push(Action::Clinic);
                }
                if self.base.friend.out_state == umasim::game::FriendOutState::AfterUnlock
                    && self.turn < 72
                    && !self.base.friend.out_used.iter().all(|used| *used)
                {
                    actions.push(Action::FriendOuting);
                }
                Ok(actions)
            }
            _ => Ok(vec![]),
        }
    }

    fn generate_events(&self, _rng: &mut impl Rng) -> Vec<EventData> {
        // A6 起：剧本固定事件（温泉杯开始 / 年末大盛況 / 宴会）+ 通用随机事件链
        Vec::new()
    }

    fn apply_event(&mut self, event: &EventData, choice: usize, rng: &mut impl Rng) -> Result<()> {
        // A6 起：剧本事件分支（继承 / 切者 / 友人解锁等由引擎既有分支处理，剧本只补自己的）
        let _ = self.base.apply_event(event, choice, rng);
        Ok(())
    }

    fn persons(&self) -> &[Self::Person] {
        &self.persons
    }

    fn persons_mut(&mut self) -> &mut [Self::Person] {
        &mut self.persons
    }

    fn init_persons(&mut self) -> Result<()> {
        // 卡片人头 + 理事长 + 记者（剧本友人在 A6 由事件追加）
        let persons: Vec<BasePerson> = self
            .base
            .deck
            .iter()
            .map(BasePerson::try_from)
            .collect::<Result<Vec<_>>>()?;
        for person in persons {
            self.add_person(person);
        }
        self.add_person(BasePerson::yayoi());
        self.add_person(BasePerson::reporter());
        Ok(())
    }

    fn distribution(&self) -> &Vec<Vec<i32>> {
        &self.base.distribution
    }

    fn distribution_mut(&mut self) -> &mut Vec<Vec<i32>> {
        &mut self.base.distribution
    }

    /// 红点分配：在引擎基础公式上叠加温泉面板的 `hint_bonus`（发生率加成）
    ///
    /// 引擎默认实现明确不含剧本修正，故此处整体覆写（发生率与卡面加成口径与引擎一致）。
    fn distribute_hint(&mut self, rng: &mut impl Rng) -> Result<()> {
        let base_hint_rate = Data::get()?.consts.base_hint_rate / 100.0;
        // 人头下标 ≠ 卡组下标：预抽 (card_id, 含剧本加成的 hint 概率加成)，循环内按 card_id 反查
        let hint_bonus = self.onsen_buff.hint_bonus;
        let hint_probs: Vec<(u32, i32)> = self
            .deck()
            .iter()
            .map(|card| {
                (
                    card.card_id,
                    card.card_value().hint_prob_increase + hint_bonus,
                )
            })
            .collect();
        for person in self.persons_mut() {
            if person.person_type() == PersonType::Card {
                let bonus = person
                    .card_id()
                    .and_then(|cid| hint_probs.iter().find(|(id, _)| *id == cid))
                    .map_or(0, |(_, bonus)| *bonus);
                let hint_prob = base_hint_rate * ((100 + bonus) as f64 / 100.0);
                person.set_hint(rng.random_bool(hint_prob));
            }
        }
        Ok(())
    }

    fn absent_rate_drop(&self) -> i32 {
        self.base.absent_rate_drop
    }

    /// 得意率：卡片累计效果 + 当前旅馆档加成
    fn deyilv(&mut self, person_index: i32) -> f32 {
        if person_index >= 0 && (person_index as usize) < self.deck.len() {
            let eff = self.deck[person_index as usize].calc_training_effect(self, 0);
            let deyilv = eff.deyilv;
            self.deck[person_index as usize].effect = eff;
            self.deck[person_index as usize].is_locked = true;
            deyilv + self.hotel.deyilv as f32
        } else {
            0.0
        }
    }

    fn has_group_buff(&self) -> bool {
        self.base.friend.group_buff_turn > 0
    }

    fn train_level(&self, train: usize) -> usize {
        if self.base.is_xiahesu() {
            // 下合宿：训练一律按最高档 5
            5
        } else {
            (self.base.train_level_count[train] as usize / 4 + 1).clamp(1, 5)
        }
    }

    /// 训练值：下层（卡片）与上层（温泉效果）分开算，各自逐槽截 100
    ///
    /// - 下层：卡片 buffs 的引擎默认值，逐槽截 100；
    /// - 上限层：把温泉效果并入 buffs 后重算总值的差值，逐槽截 100；
    /// - 卡片无友情加成时温泉友情也不生效（沿用旧实现口径）。
    fn calc_training_value(&self, buffs: &CardTrainingEffect, train: usize) -> Result<ActionValue> {
        let mut base_value = self.default_calc_training_value(buffs, train)?;
        for slot in base_value.status_pt.iter_mut() {
            *slot = (*slot).min(100);
        }
        if self.bathing.buff_remain_turn == 0 {
            return Ok(base_value);
        }
        let mut onsen_buff = self.onsen_training_effect(train);
        if buffs.youqing == 0.0 {
            onsen_buff.youqing = 0.0;
        }
        let total_buff = buffs.add(&onsen_buff);
        let mut total_value = self.default_calc_training_value(&total_buff, train)?;
        for (slot, base) in total_value.status_pt.iter_mut().zip(base_value.status_pt) {
            let upper = (*slot - base).min(100);
            *slot = base + upper;
        }
        Ok(total_value)
    }

    fn uma(&self) -> &Uma {
        &self.base.uma
    }

    fn uma_mut(&mut self) -> &mut Uma {
        &mut self.base.uma
    }

    fn deck(&self) -> &Vec<SupportCard> {
        &self.base.deck
    }
}

impl TrainingValueSource for OnsenGame {
    /// 训练基础值表归剧本数据（`scenario_onsen.json`）
    fn training_basic_table(&self) -> &TrainingBasicTable {
        self.data.training_table()
    }
}
