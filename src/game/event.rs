//! 剧本事件：生成、结算特例与回合开始事件链（A6 / B1–B4）
//!
//! 本模块只放**剧本自有**的事件机制；通用事件结算（数值入账、友人加成、事件计数）仍走
//! 引擎 `BaseGame::apply_event`，这里在其后补温泉剧本的分支（继承 / 切者 / 女将解锁 /
//! 出行券 / 年末训练等级 / 宴会再发券）与体力消耗带来的超回复判定。
//!
//! 事件来源分三层：① 引擎 `events.story_events`（通用剧本事件）；② 本仓
//! `scenario_onsen.json` 的 `scenario_events`（温泉杯 5 条固定事件）；③ 无固定事件时按
//! 引擎事件分布抽随机事件（支援卡连续事件 / 马娘事件 / 掉心情），与旧温泉实现同口径。

use anyhow::Result;
use rand::Rng;
use rand::seq::IndexedRandom;
use umaai_core::AttributeArray;
use umaai_core::Data;
use umaai_core::gamedata::{ActionValue, EventData};
use umasim::game::{FriendOutState, Game, Person, PersonType, UmaFlagsExt};

use crate::game::OnsenGame;

impl OnsenGame {
    /// 生成当前回合的事件（`Begin` 阶段）
    ///
    /// 优先级：固定回合事件（引擎通用 + 温泉自有）→ 友人出门判定 → 一般随机事件。
    /// `no_event_turns` 内的回合不抽随机事件（固定事件不受影响）。
    pub(crate) fn generate_onsen_events(&self, rng: &mut impl Rng) -> Vec<EventData> {
        let engine = match Data::get() {
            Ok(data) => data,
            Err(_) => return vec![],
        };
        // ① 固定回合事件：引擎通用剧本事件 + 温泉剧本事件
        let mut scenario_events = self.list_turn_events(&engine.events.story_events);
        scenario_events.extend(self.list_turn_events(&self.data.scenario_events));
        if !scenario_events.is_empty() {
            return scenario_events;
        }
        // ② 禁事件回合不再抽随机事件
        if engine.consts.no_event_turns.contains(&self.turn) {
            return vec![];
        }
        // ③ 友人出门（未解锁时按羁绊高低取概率；命中后本回合只发生这一件）
        if self.friend.out_state == FriendOutState::BeforeUnlock {
            let friendship = self
                .friend
                .card_id
                .and_then(|card_id| self.person_index_of_card(card_id))
                .map_or(0, |index| self.persons[index].friendship());
            let prob = if friendship < 60 {
                engine.event_prob("friend_unlock_low")
            } else {
                engine.event_prob("friend_unlock_high")
            }
            .unwrap_or(0.0);
            if rng.random_bool(prob) {
                return vec![engine.events.friend_events["out"].clone()];
            }
        }
        // ④ 一般随机事件：分布分桶 → 支援卡连续事件 / 马娘事件 / 掉心情
        match sample_bucket(&engine.consts.get_event_distribution(), rng) {
            0 => {
                let count = self.base.deck.len().min(self.persons.len());
                if count == 0 {
                    return vec![];
                }
                // 人头下标 ≠ 卡组下标：候选只取可出现的支援卡人头
                let mut person_index = rng.random_range(0..count);
                let mut tries = 0;
                while !self.person_is_available(person_index) && tries < 16 {
                    person_index = rng.random_range(0..count);
                    tries += 1;
                }
                match self.base.generate_card_event(person_index as i32, rng) {
                    Ok(Some(event)) => vec![event],
                    _ => vec![],
                }
            }
            1 => self
                .base
                .random_select_event(&engine.events.uma_events, rng)
                .into_iter()
                .collect(),
            2 => {
                // 掉心情只在第 12 回合之后进入分布
                if self.turn >= 12 {
                    engine
                        .system_event("drop_motivation")
                        .ok()
                        .cloned()
                        .into_iter()
                        .collect()
                } else {
                    vec![]
                }
            }
            _ => vec![],
        }
    }

    /// 剧本事件结算：先走引擎通用结算，再补温泉特例
    ///
    /// 引擎侧负责数值入账、事件计数与友人卡加成；这里补：
    /// - 体力消耗并入超回复累计；
    /// - 事件羁绊按人头身份回账（卡组下标 ≠ 人头下标）；
    /// - 继承 / 切者 / 女将解锁 / 出行 5 段券 / 年末训练等级 / 宴会再发券。
    pub(crate) fn apply_onsen_event(
        &mut self,
        event: &EventData,
        choice: usize,
        rng: &mut impl Rng,
    ) -> Result<()> {
        let result = self.with_vital_overflow(|game| game.base.apply_event(event, choice, rng));
        if let Some(result) = result {
            // 事件结算即时回显（青色；`event_verbose` 开启时打印，手动验证用）
            if self.event_verbose {
                let summary = crate::game::value_summary(&result.value);
                let text = if summary.is_empty() {
                    event.name.clone()
                } else {
                    format!("{}: {summary}", event.name)
                };
                println!("\x1b[36m事件 {text}\x1b[0m");
            }
            // 体力消耗（负数）按计算值并入超回复分档
            if result.value.vital < 0 {
                self.update_super_on_vital_cost(-result.value.vital, rng);
            }
            if result.value.friendship != 0
                && let Some(person_index) = self.base.event_person_target(event, &self.persons)
            {
                self.base
                    .add_friendship(&mut self.persons, person_index, result.value.friendship);
            }
        }
        match event.id {
            // 继承：属性 + 上限
            4012 | 4013 => {
                let inherit_value = ActionValue {
                    status_pt: self.base.inherit.inherit(rng),
                    ..Default::default()
                };
                let inherit_limit = self.base.inherit.inherit_limit(rng);
                self.uma.add_value(&inherit_value);
                self.uma.five_status_limit.add_eq(&inherit_limit);
            }
            // 大成功：按概率获得切者
            5007 => {
                if rng.random_bool(Data::get()?.event_prob("qiezhe_normal")?) {
                    self.uma.flags.gain_qiezhe();
                }
            }
            // 女将解锁出行，并获得休息心得（每回合 +5 体力）
            809050004 => {
                self.friend.out_state = FriendOutState::AfterUnlock;
                self.uma.flags.refresh_mind = 1;
            }
            // 友人出行第 1–4 段：温泉券 +1 并触发超回复
            809050011..=809050014 => {
                self.add_ticket(1);
                self.add_super();
            }
            // 友人出行第 5 段：按女将稀有度发券并触发超回复
            809050015 => {
                self.add_ticket(self.grant_ticket_num());
                self.add_super();
            }
            // 第 1、2 年末（汤浴会）：全部训练等级 +1（每级 4 次计数）
            400012005 | 400012011 => {
                for count in &mut self.base.train_level_count {
                    *count += 4;
                }
            }
            // 第 3 年末：再发一次券
            400012017 => {
                self.add_ticket(self.grant_ticket_num());
            }
            _ => {}
        }
        Ok(())
    }

    /// 休息心得：每回合开始体力 +5，按持续回合查结束概率
    pub fn update_refresh_mind(&mut self, rng: &mut impl Rng) {
        let t = self.uma.flags.refresh_mind as usize;
        if t == 0 {
            return;
        }
        // 超回复溢出时回体只失效，不把体力压回上限
        self.with_vital_overflow(|game| {
            game.uma.add_value(&ActionValue {
                vital: 5,
                ..Default::default()
            });
        });
        self.uma.flags.refresh_mind += 1;
        let end_prob = Data::get()
            .ok()
            .and_then(|data| data.consts.group_buff_end_prob.get(t.min(6)).copied())
            .unwrap_or(0.0);
        if rng.random_bool(end_prob) {
            self.uma.flags.refresh_mind = 0;
        }
    }

    /// 秘汤汤驹的分身是否可用：「支援卡种类」按全部卡种（7 类，含友人 / 团队）非零类数 ≥ 5
    #[must_use]
    pub(crate) fn deck_can_split(&self) -> bool {
        self.base
            .card_type_count
            .iter()
            .filter(|count| **count > 0)
            .count()
            >= 5
    }

    /// 秘汤汤驹：把已在训练中的人头再追加分配一次（每张卡最多分身一次）
    ///
    /// 只在温泉效果生效、`split > 0` 且卡种覆盖足够时执行；追加位置走引擎
    /// `distribute_person`（不判「不在」）。
    pub fn distribute_extra_supports(&mut self, rng: &mut impl Rng) -> Result<()> {
        if self.bathing.buff_remain_turn == 0
            || self.onsen_buff.split == 0
            || !self.deck_can_split()
        {
            return Ok(());
        }
        let candidates: Vec<usize> = (0..self.base.deck.len().min(self.persons.len()))
            .filter(|index| {
                self.persons[*index].person_type() == PersonType::Card
                    && self.at_trains(*index as i32).iter().any(|at| *at)
            })
            .collect();
        let chosen: Vec<usize> = candidates
            .choose_multiple(rng, self.onsen_buff.split as usize)
            .copied()
            .collect();
        for person_index in chosen {
            self.distribute_person(person_index as i32, false, rng)?;
        }
        Ok(())
    }
}

/// 数值权重加权采样（返回桶下标）
///
/// 与引擎内部 `sample_bucket` 同语义（累积权重取首个 `> 采样值` 的桶），
/// 引擎侧为 `pub(crate)` 不可复用，故在剧本侧保留一份最小实现。
fn sample_bucket(weights: &[f64], rng: &mut impl Rng) -> usize {
    let total: f64 = weights.iter().sum();
    let chosen = rng.random::<f64>() * total;
    let mut acc = 0.0;
    for (index, weight) in weights.iter().enumerate() {
        acc += weight;
        if chosen < acc {
            return index;
        }
    }
    weights.len().saturating_sub(1)
}
