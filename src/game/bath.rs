use anyhow::{Result, anyhow};
use rand::Rng;
use umaai_core::Data;
use umasim::game::{Game, Person, PersonType, hint_attr_event, hint_skill_event};

use crate::game::OnsenGame;

impl OnsenGame {
    /// 获得超回复（已就绪时不重复获得）
    ///
    /// 触发时清空累计体力消耗（下一次超回复从 0 重新累计）。
    pub fn add_super(&mut self) {
        if !self.bathing.is_super_ready {
            self.bathing.is_super_ready = true;
            self.dig_vital_cost = 0;
        }
    }

    /// 超回复触发概率：按累计体力消耗每 50 一点一档查表；累计 ≥275 保底 1.0
    #[must_use]
    pub fn calc_super_prob(&self, vital_cost: i32) -> f64 {
        let cost = self.dig_vital_cost + vital_cost;
        if cost >= 275 {
            1.0
        } else {
            let rank = ((cost as f64 / 50.0).floor() as usize).min(self.data.super_probs.len() - 1);
            self.data.super_probs[rank] as f64 / 100.0
        }
    }

    /// 一次体力消耗后的超回复判定（已就绪 / 开局前两回合不判定）
    ///
    /// 判定先于累计：概率按「已有消耗 + 本次消耗」查档，之后才把本次消耗累加。
    pub(crate) fn update_super_on_vital_cost(&mut self, vital_cost: i32, rng: &mut impl Rng) {
        if self.bathing.is_super_ready || self.turn < 2 {
            return;
        }
        let prob = self.calc_super_prob(vital_cost);
        self.dig_vital_cost += vital_cost;
        if rng.random_bool(prob) {
            self.add_super();
        }
    }

    /// 执行 PR：入账 PR 数值 + 发 1 张温泉券
    ///
    /// 返回体力消耗（供超回复判定，取计算值而非实际值）。
    pub(crate) fn do_pr(&mut self, _rng: &mut impl Rng) -> Result<i32> {
        let value = self.data.pr_base_value.clone();
        let vital_cost = -value.vital;
        self.uma_mut().add_value(&value);
        self.add_ticket(1);
        Ok(vital_cost)
    }

    /// 使用温泉券入浴
    ///
    /// 顺序：刷新温泉效果面板（解锁面板）→ 羁绊最低的 N 人入账 → 体力（超回复时用临时上限）
    /// → 干劲 → 超回复额外效果（技能点 / 红点）→ 扣券并让效果持续 2 回合。
    pub(crate) fn do_use_ticket(&mut self, rng: &mut impl Rng) -> Result<()> {
        if self.bathing.ticket_num <= 0 {
            return Err(anyhow!("没有温泉券"));
        }
        if self.bathing.buff_remain_turn > 0 {
            return Err(anyhow!("温泉效果尚未结束"));
        }
        // 1. 用券刷新温泉效果面板（已挖源泉的效果在此才进面板）
        self.update_scenario_buff(true);
        // 2. 羁绊最低的 N 人各 +value（满 100 的不再加）
        let friendship_count = self.onsen_buff.friendship_count.max(0) as usize;
        let friendship_value = self.onsen_buff.friendship;
        if friendship_count > 0 && friendship_value != 0 {
            let card_count = self.base.deck.len().min(self.persons.len());
            let mut order: Vec<usize> = (0..card_count).collect();
            order.sort_by_key(|i| self.persons[*i].friendship());
            for i in order.into_iter().take(friendship_count) {
                if self.persons[i].friendship() < 100 {
                    self.base
                        .add_friendship(&mut self.persons, i, friendship_value);
                }
            }
        }
        // 3. 体力恢复：超回复时体力上限取临时上限（可能高于常规上限）
        let super_vital = self.data.super_effect.vital;
        let super_pt = self.data.super_effect.pt;
        let (max_hp, recovery) = if self.bathing.is_super_ready {
            (
                self.data.super_effect.temp_max_vital,
                self.onsen_buff.vital + super_vital,
            )
        } else {
            (self.base.uma.max_vital, self.onsen_buff.vital)
        };
        self.uma_mut().vital = (self.base.uma.vital + recovery).min(max_hp);
        // 4. 干劲
        let motivation = self.onsen_buff.motivation;
        if motivation > 0 {
            self.uma_mut().motivation = (self.base.uma.motivation + motivation).min(5);
        }
        // 5. 超回复额外效果：技能点 + 红点（次数 = 旅馆档 hint + 2）
        if self.bathing.is_super_ready {
            self.uma_mut().skill_pt += super_pt;
            let hint_count = self.hotel.hint + 2;
            for _ in 0..hint_count {
                let cards: Vec<usize> = (0..self.base.deck.len())
                    .filter(|i| self.persons[*i].person_type() == PersonType::Card)
                    .collect();
                if cards.is_empty() {
                    break;
                }
                let person_index = cards[rng.random_range(0..cards.len())];
                self.do_hint(person_index, false, rng)?;
            }
            self.bathing.is_super = true;
            // 传说秘泉（索引 9）挖通时超回复状态保持，可再次触发
            if !self.onsen_state.get(9).copied().unwrap_or(false) {
                self.bathing.is_super_ready = false;
            }
        }
        // 6. 扣券，效果持续 2 回合
        self.bathing.ticket_num -= 1;
        self.bathing.buff_remain_turn = 2;
        Ok(())
    }

    /// 生成一次红点事件（可选是否带羁绊）
    ///
    /// 单卡红点等级取「卡面等级 + 1」，受 5 级与单卡累计上限双重截断；卡面 `hint_count_bonus`
    /// 决定判定次数。等级为 0（或判定落在属性档）时只给属性红点，不累计技能红点等级。
    pub(crate) fn do_hint(
        &mut self,
        person_index: usize,
        has_friendship: bool,
        rng: &mut impl Rng,
    ) -> Result<()> {
        let data = Data::get()?;
        let attr_prob = data.event_prob("hint_attr")?;
        let max_hint_per_card = data.consts.max_hint_per_card;
        let card_count = self.base.deck.len();
        let (hint_level, hint_count) = if person_index < card_count {
            let card = &self.base.deck[person_index];
            let level = (1 + card.card_value().hint_level)
                .min(5)
                .min(max_hint_per_card - card.total_hints);
            (level, 1 + card.effect.hint_count_bonus)
        } else {
            (1, 1)
        };
        for _ in 0..hint_count {
            let mut event = if rng.random_bool(attr_prob) || hint_level == 0 {
                // 红点提供属性；超过单卡上限时也只能提供属性
                hint_attr_event(
                    self.persons[person_index].train_type() as usize,
                    person_index,
                )?
            } else {
                self.base.deck[person_index].total_hints += hint_level;
                hint_skill_event(hint_level, person_index)
            };
            if !has_friendship
                && let Some(choice) = event.choices.first_mut().and_then(|row| row.first_mut())
            {
                choice.value.friendship = 0;
            }
            self.unresolved_events.push(event);
        }
        Ok(())
    }
}
