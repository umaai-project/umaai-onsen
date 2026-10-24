use std::sync::Arc;

use anyhow::{Result, anyhow};
use rand::Rng;
use rand::rngs::StdRng;
use umaai_core::gamedata::ActionValue;
use umaai_core::rng::StrategyRng;
use umasim::game::{FriendCardState, Game, PersonType, Trainer};

use crate::game::{Action, OnsenGame};

impl OnsenGame {
    /// 重算温泉效果面板 / 旅馆档 / 挖掘力
    ///
    /// `use_ticket=true` 时面板重算（用券刷新）；否则仅在面板为空（`vital == 0`，尚未算过）时重算
    /// —— 这就是「锁面板」：已挖出的新源泉效果要**用券**才会进面板。
    /// 挖掘力每回合都要重算（属性档位随育成变化）。
    pub fn update_scenario_buff(&mut self, use_ticket: bool) {
        let data = Arc::clone(&self.data);
        if use_ticket || self.onsen_buff.vital == 0 {
            self.onsen_buff = data.sum_effects(self.onsen_state.iter());
        }
        self.dig_count = self.onsen_state.iter().filter(|done| **done).count() as i32;
        // 旅馆档：按已挖源泉数取最高满足档
        self.hotel = data.hotel_rank(self.dig_count);
        // 挖掘力三部分：蓝因子 + 属性档位 + 装备
        let mut blue = [0i32; 3];
        let mut stat_bonus = [0i32; 3];
        let last_rank = data.dig_stat_ranks.len().saturating_sub(1);
        for stat in 0..5 {
            // 属性落在哪一档（阈值表从高到低回退）
            let mut rank = last_rank;
            while rank > 0 && self.base.uma.five_status[stat] < data.dig_stat_ranks[rank] {
                rank -= 1;
            }
            for dig_type in 0..3 {
                blue[dig_type] += self.dig_blue_count[stat] * data.dig_blue_bonus[stat][dig_type];
                // 该地层下第 which 个槽位对应哪个属性
                for (which, slot) in data.dig_stat_bonus_types[dig_type].iter().enumerate() {
                    if *slot == (stat + 1) as i32 {
                        stat_bonus[dig_type] += data.dig_stat_bonus[which][rank];
                    }
                }
            }
        }
        for dig_type in 0..3 {
            let tool = data.dig_tool_level[self.dig_level[dig_type] as usize];
            self.dig_power[dig_type] = blue[dig_type] + stat_bonus[dig_type] + tool;
        }
    }

    /// 各主动作的基础挖掘量（无挖掘窗口 / 无该动作时 `None`）
    ///
    /// 训练按实际人头数（排除理事长与记者）计；PR 以 1 个人头计（固定 11 点）。
    fn dig_base_value(&self, action: &Action) -> Option<i32> {
        match action {
            Action::Train(train) => {
                let mut person_count = 0;
                for person_index in &self.base.distribution[*train as usize] {
                    if *person_index < 0 {
                        continue;
                    }
                    match self.persons[*person_index as usize].person_type {
                        PersonType::Yayoi | PersonType::Reporter => {}
                        _ => person_count += 1,
                    }
                }
                Some(25 + person_count)
            }
            Action::Race => Some(if self.base.uma.is_race_turn(self.base.turn) {
                25
            } else {
                15
            }),
            Action::PR => Some(11),
            Action::Sleep | Action::NormalOuting => Some(15),
            Action::FriendOuting => Some(25),
            Action::Clinic => Some(0),
            _ => None,
        }
    }

    /// 当前挖掘地层（`None` = 无挖掘窗口：开局前两回合 / 决赛期 / 下合宿）
    #[must_use]
    pub fn current_dig_type(&self) -> Option<usize> {
        if self.turn < 2 || self.turn >= 72 || self.base.is_xiahesu() {
            return None;
        }
        let remain = &self.dig_remain[self.current_onsen];
        if remain[0] > 0 {
            Some(0)
        } else if remain[1] > 0 {
            Some(1)
        } else {
            Some(2)
        }
    }

    /// 当前层之后还有剩余的下一个地层
    #[must_use]
    pub fn next_dig_type(&self) -> Option<usize> {
        let current = self.current_dig_type()?;
        let remain = &self.dig_remain[self.current_onsen];
        (current + 1..3).find(|ty| remain[*ty] > 0)
    }

    /// 计算本次动作的挖掘量 `[砂 土 岩]`
    ///
    /// - 当前层挖掘力含**链接加成**（剧本链接角色在其对应地层 +10%）；
    /// - 挖掘量超过当前层剩余量时跨层：当前层取剩余量整数（不取 floor），
    ///   余量再按**下一层挖掘力（不含链接）**折算，并受下一层剩余量封顶。
    #[must_use]
    pub fn calc_dig_value(&self, action: &Action) -> Option<[i32; 3]> {
        let base = self.dig_base_value(action)?;
        let ty = self.current_dig_type()?;
        // 链接加成：剧本链接马娘 + 携带的链接支援卡，命中时该地层 +10%
        let mut link_bonus = [0i32; 3];
        if let Some(link_type) = self.data.link_effect.get(&self.base.uma.chara_id()) {
            link_bonus[(link_type - 1) as usize] += 10;
        }
        for person in self.persons.iter().take(self.base.deck.len()) {
            if let Some(link_type) = self.data.link_effect.get(&person.chara_id) {
                link_bonus[(link_type - 1) as usize] += 10;
            }
        }
        let cur = self.current_onsen;
        let remain = self.dig_remain[cur];
        let current_rate =
            (100.0 + self.dig_power[ty] as f64) * (100.0 + link_bonus[ty] as f64) / 10000.0;
        let dig_value = (base as f64 * current_rate).floor() as i32;
        let mut ret = [0i32; 3];
        if dig_value > remain[ty]
            && let Some(next_ty) = self.next_dig_type()
        {
            ret[ty] = remain[ty];
            let base_remain = base as f64 - remain[ty] as f64 / current_rate;
            let next_rate = (100.0 + self.dig_power[next_ty] as f64) / 100.0;
            let next_value = (base_remain * next_rate).floor() as i32;
            ret[next_ty] = next_value.min(remain[next_ty]);
        } else {
            ret[ty] = dig_value.min(remain[ty]);
        }
        Some(ret)
    }

    /// 执行挖掘：按进度发 30 点挖掘奖励，扣减剩余量，挖通则发券并挂起「升级 + 选源泉」
    ///
    /// 返回是否挖通**当前**源泉。
    pub fn do_dig(&mut self, value: &[i32; 3], rng: &mut impl Rng) -> bool {
        let data = Arc::clone(&self.data);
        let cur = self.current_onsen;
        let mut total_bonus = [0i32; 6];
        for (dig_type, dig_value) in value.iter().copied().enumerate() {
            if dig_value > 0 {
                // 奖励次数 = floor((进度 + 挖掘量) / 30) − floor(进度 / 30)
                let old_count = self.dig_progress[cur][dig_type] / 30;
                let new_count = (self.dig_progress[cur][dig_type] + dig_value) / 30;
                let bonus_count = new_count - old_count;
                if bonus_count > 0 {
                    for (slot, bonus) in total_bonus.iter_mut().zip(&data.dig_bonus[dig_type]) {
                        *slot += bonus * bonus_count;
                    }
                }
                self.dig_progress[cur][dig_type] += dig_value;
            }
            self.dig_remain[cur][dig_type] -= dig_value;
        }
        if total_bonus.iter().any(|slot| *slot != 0) {
            // 体力变化在奖励表末位（负数），供超回复分档累计
            let vital = total_bonus[5];
            total_bonus[5] = 0;
            let value = ActionValue {
                status_pt: total_bonus,
                vital,
                ..Default::default()
            };
            // 挖掘进度奖励回显（青色；`event_verbose` 开启时打印，手动验证用）
            if self.event_verbose {
                let summary = crate::game::value_summary(&value);
                println!("\x1b[36m挖掘进度奖励: {summary}\x1b[0m");
            }
            self.with_vital_overflow(|game| {
                game.uma_mut().add_value(&value);
            });
            // 挖掘奖励的体力变化并入超回复累计与判定
            self.update_super_on_vital_cost(-vital, rng);
        }
        if self.dig_remain[cur].iter().all(|remain| *remain <= 0) {
            self.dig_remain[cur] = [0, 0, 0];
            // 基础温泉开局即算完成：仅在首次真正挖通时发券
            if !self.onsen_state[cur] {
                self.onsen_state[cur] = true;
                self.add_ticket(self.grant_ticket_num());
                self.pending_selection = true;
            }
            true
        } else {
            false
        }
    }

    /// 动作结算后的挖掘（无窗口 / 无挖掘量时跳过）
    pub(crate) fn run_dig_for(&mut self, action: &Action, rng: &mut impl Rng) {
        if let Some(value) = self.calc_dig_value(action) {
            let _ = self.do_dig(&value, rng);
        }
    }

    /// 获得温泉券（上限 3）
    pub fn add_ticket(&mut self, num: i32) {
        self.bathing.ticket_num = (self.bathing.ticket_num + num).min(3);
    }

    /// 本次挖通源泉的赠券数（按友人卡稀有度；开局 / 决赛期为定值）
    #[must_use]
    pub fn grant_ticket_num(&self) -> i32 {
        if self.turn == 2 {
            // 开局均为 2 张
            2
        } else if self.turn == 72 {
            // 决赛期：SSR 2 张，其余 1 张
            if self.base.friend.card_state == FriendCardState::SSR {
                2
            } else {
                1
            }
        } else {
            match self.base.friend.card_state {
                FriendCardState::SSR => 2,
                FriendCardState::R => 1,
                _ => 0,
            }
        }
    }

    /// 列出可选的源泉（未挖通且已解锁）
    #[must_use]
    pub fn list_actions_onsen_select(&self) -> Vec<Action> {
        self.onsen_state
            .iter()
            .enumerate()
            .filter(|(i, done)| !**done && self.turn >= self.data.onsen_info[*i].unlock_turn)
            .map(|(i, _)| Action::Dig(i))
            .collect()
    }

    /// 执行源泉选择
    fn do_select_onsen<T: Trainer<Self>>(
        &mut self,
        trainer: &T,
        decision: &mut StdRng,
        strategy: &mut StrategyRng,
    ) -> Result<()> {
        let actions = self.list_actions_onsen_select();
        if actions.is_empty() {
            return Err(anyhow!(
                "没有可选源泉（数据或程序错误），当前回合 {}",
                self.turn
            ));
        }
        let selection = trainer.select_action(self, &actions, decision)?;
        let action = actions
            .get(selection)
            .ok_or_else(|| anyhow!("训练员返回的选项越界: {selection}"))?;
        self.apply_onsen_action(action, strategy)
    }

    /// 选择要挖掘的源泉（解锁校验）
    pub fn do_select_dig(&mut self, onsen_index: usize) -> Result<()> {
        let info = self
            .data
            .onsen_info
            .get(onsen_index)
            .ok_or_else(|| anyhow!("源泉索引越界: {onsen_index}"))?;
        if self.onsen_state[onsen_index] {
            return Err(anyhow!("该源泉已被挖掘完成: {}", info.name));
        }
        if self.turn < info.unlock_turn {
            return Err(anyhow!("该源泉尚未解锁: {}", info.name));
        }
        self.current_onsen = onsen_index;
        Ok(())
    }

    /// 可升级的装备地层列表（等级 < 6）
    #[must_use]
    pub fn get_upgradeable_equipment(&self) -> Vec<usize> {
        (0..3)
            .filter(|dig_type| self.dig_level[*dig_type] < 6)
            .collect()
    }

    /// 升级挖掘装备（上限 6 级）
    pub fn do_upgrade_equipment(&mut self, dig_type: usize) -> Result<()> {
        if dig_type >= 3 {
            return Err(anyhow!("装备类型越界: {dig_type}"));
        }
        if self.dig_level[dig_type] >= 6 {
            return Err(anyhow!("装备已达最高等级: {dig_type}"));
        }
        self.dig_level[dig_type] += 1;
        // 装备变动后挖掘力立即重算
        self.update_scenario_buff(false);
        Ok(())
    }

    /// 挖掘完成 / 新温泉解锁后的选择流程：先选源泉，再升级装备
    pub(crate) fn handle_pending_selection<T: Trainer<Self>>(
        &mut self,
        trainer: &T,
        decision: &mut StdRng,
        strategy: &mut StrategyRng,
    ) -> Result<()> {
        if !self.pending_selection {
            return Ok(());
        }
        // 1. 源泉选择（无解锁可选源泉时跳过：全部挖完等退化情形）
        if !self.list_actions_onsen_select().is_empty() {
            self.do_select_onsen(trainer, decision, strategy)?;
        }
        // 2. 装备升级（无不满级装备时跳过）
        let upgradeable = self.get_upgradeable_equipment();
        if !upgradeable.is_empty() {
            let actions: Vec<Action> = upgradeable.iter().map(|t| Action::Upgrade(*t)).collect();
            let selection = trainer.select_action(self, &actions, decision)?;
            let action = actions
                .get(selection)
                .ok_or_else(|| anyhow!("训练员返回的选项越界: {selection}"))?;
            self.apply_onsen_action(action, strategy)?;
        }
        // 3. 重置标志
        self.pending_selection = false;
        Ok(())
    }
}
