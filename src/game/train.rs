use anyhow::{Result, anyhow};
use rand::Rng;
use umaai_core::{Data, Ill};
use umasim::game::{CardTrainingEffect, Game};

use crate::game::OnsenGame;

impl OnsenGame {
    /// 训练结算：失败判定 → 数值入账 → 通用后处理
    ///
    /// 返回 `(是否成功, 体力消耗)`；体力消耗供调用方做超回复判定（按计算值而非实际值）。
    /// **失败率口径**：卡片 buffs 与「温泉效果生效期间的 `fail_rate_drop`」合并后交给引擎
    /// （旧实现该字段未生效，此处按 M1 口径补实现）。
    pub(crate) fn do_train(&mut self, train: usize, rng: &mut impl Rng) -> Result<(bool, i32)> {
        if train >= 5 {
            return Err(anyhow!("训练等级越界: {train}"));
        }
        let vital_before = self.base.uma.vital;
        let buffs = self.calc_training_buff(train)?;
        let failure_buffs = if self.bathing.buff_remain_turn > 0 {
            buffs.add(&self.onsen_training_effect(train))
        } else {
            buffs.clone()
        };
        let failure_rate = self.calc_training_failure_rate(&failure_buffs, train)? / 100.0;
        if rng.random_bool(failure_rate as f64) {
            // 再判定一次，仍失败即大失败（生病 + 不擅长训练同置）
            if rng.random_bool(failure_rate as f64) {
                self.apply_event(
                    &Data::get()?.system_event("training_fail_low")?.clone(),
                    0,
                    rng,
                )?;
                self.uma_mut().flags.ill.insert(Ill::BAD_TRAINER);
                self.uma_mut().flags.bad_trainer = true;
            } else {
                self.apply_event(&Data::get()?.system_event("training_fail")?.clone(), 0, rng)?;
            }
            let vital_cost = (vital_before - self.base.uma.vital).max(0);
            return Ok((false, vital_cost));
        }
        let value = self.calc_training_value(&buffs, train)?;
        // 以计算值（而非实际值）计算超回复消耗的体力
        let vital_cost = -value.vital;
        self.uma_mut().add_value(&value);
        self.base.train_level_count[train] += 1;
        // 通用后处理（羁绊 / 红点 / 加练 / 友人点击）走引擎可复用件
        self.base
            .default_after_train(&mut self.persons, train, rng)?;
        Ok((true, vital_cost))
    }

    /// 温泉效果折算成训练 buff（并入卡片 buffs 后交给引擎训练值公式）
    ///
    /// 即旧实现的 `OnsenEffect::to_training_effect`：只取友情 / 训练 / 体力消耗下降 / 失败率下降。
    #[must_use]
    pub(crate) fn onsen_training_effect(&self, train: usize) -> CardTrainingEffect {
        CardTrainingEffect {
            youqing: self.onsen_buff.youqing[train] as f32,
            xunlian: self.onsen_buff.xunlian,
            vital_cost_drop: self.onsen_buff.vital_cost_drop as f32,
            fail_rate_drop: self.onsen_buff.fail_rate_drop as f32,
            ..Default::default()
        }
    }
}
