mod action;
mod bath;
mod dig;
mod stage;
mod train;
mod traits;

use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use umaai::Config;
use umaai_core::Array5;
use umaai_core::rng::StrategyRng;
use umasim::game::base::{BaseGame, BasePerson};
use umasim::game::{Game, InheritInfo};

use crate::data::{HotelEffect, OnsenData, OnsenEffect};

pub use action::Action;
pub use stage::{BathingInfo, Stage};

/// 剧本资源快照（给前端的纯数据 View）
///
/// 形态 A 五条纪律：单一取数口 / 只放引擎不知道的字段 / 快照语义 / `non_exhaustive` / 不进契约 trait。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct OnsenView {
    /// 温泉券数量
    pub ticket_num: i32,
    /// 温泉效果剩余回合
    pub buff_remain_turn: i32,
    /// 下次入浴是否超回复
    pub is_super_ready: bool,
}

/// 剧本局面
#[derive(Debug, Clone, PartialEq)]
pub struct OnsenGame {
    /// 引擎状态底座（经 `Deref` 取用）
    pub base: BaseGame,
    /// 人头
    pub persons: Vec<BasePerson>,
    /// 剧本自持阶段机
    pub stage: Stage,
    /// 剧本自持策略流（动作结算与机制随机；从规则主种子派生）
    pub strategy: Option<StrategyRng>,
    /// 入浴状态
    pub bathing: BathingInfo,
    /// 已挖通的源泉（索引 0 = 开局即已挖通的基础温泉）
    pub onsen_state: Vec<bool>,
    /// 当前正在挖掘的源泉索引
    pub current_onsen: usize,
    /// 各源泉本地层剩余挖掘量 `[源泉][砂 土 岩]`
    pub dig_remain: Vec<[i32; 3]>,
    /// 各源泉本地层累计挖掘进度 `[源泉][砂 土 岩]`（每 30 点触发一次挖掘奖励）
    pub dig_progress: Vec<[i32; 3]>,
    /// 已挖通源泉数（旅馆档按它取）
    pub dig_count: i32,
    /// 挖掘力 = 蓝因子 + 属性档位 + 装备 `[砂 土 岩]`
    pub dig_power: [i32; 3],
    /// 挖掘装备等级 `[砂 土 岩]`（1–6）
    pub dig_level: [i32; 3],
    /// 蓝因子换算出的因子个数（五维；每 3 点算一个）
    pub dig_blue_count: Array5,
    /// 累计挖掘消耗体力（超回复分档用；判定在 A5 启用）
    pub dig_vital_cost: i32,
    /// 挖掘完成后的待处理标志（装备升级 + 源泉选择）
    pub pending_selection: bool,
    /// 温泉效果面板（已挖源泉效果之和；**锁面板**语义）
    pub onsen_buff: OnsenEffect,
    /// 当前旅馆档
    pub hotel: HotelEffect,
    /// 剧本静态数据（宿主注入；一局内不变，克隆共享）
    pub data: Arc<OnsenData>,
}

impl Deref for OnsenGame {
    type Target = BaseGame;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl DerefMut for OnsenGame {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.base
    }
}

impl OnsenGame {
    /// 建一局（要求宿主已初始化引擎静态数据：`umaai_core::Data::init`）
    ///
    /// `data` 是剧本自持资产（宿主载入后注入）：五维上限、训练基础值表等数值都取自它。
    pub fn newgame(
        uma_id: u32,
        deck_ids: &[u32],
        inherit: InheritInfo,
        race_grades: &[i32],
        data: Arc<OnsenData>,
    ) -> Result<Self> {
        let base = BaseGame::new(
            uma_id,
            deck_ids,
            inherit,
            // 五维上限归剧本：用剧本数据，不用引擎常量
            data.five_status_limit_base,
            race_grades,
        )?;
        let onsen_count = data.onsen_count();
        // 索引 0 为开局即已挖通的基础温泉；其后源泉本地层剩余量取自剧本数据
        let mut onsen_state = vec![false; onsen_count];
        let mut dig_remain = Vec::with_capacity(onsen_count);
        let mut dig_progress = Vec::with_capacity(onsen_count);
        for (i, info) in data.onsen_info.iter().enumerate() {
            onsen_state[i] = i == 0;
            // 基础温泉视为已挖完（剩余量 0），否则挖通时会重复发券
            dig_remain.push(if i == 0 { [0, 0, 0] } else { info.dig_volume });
            dig_progress.push([0, 0, 0]);
        }
        // 蓝因子换算：每 3 点算一个因子（沿用旧实现口径）
        let mut dig_blue_count = [0i32; 5];
        for (i, count) in base.inherit.blue_count.iter().enumerate() {
            dig_blue_count[i] = (*count as f32 / 3.0).ceil() as i32;
        }
        let mut game = Self {
            base,
            persons: Vec::new(),
            stage: Stage::Begin,
            strategy: None,
            bathing: BathingInfo::default(),
            onsen_state,
            current_onsen: 0,
            dig_remain,
            dig_progress,
            dig_count: 0,
            dig_power: [0; 3],
            dig_level: [1; 3],
            dig_blue_count,
            dig_vital_cost: 0,
            pending_selection: false,
            onsen_buff: OnsenEffect::default(),
            hotel: HotelEffect::default(),
            data,
        };
        game.init_persons()?;
        // 温泉效果面板与挖掘力初值（此后锁面板：仅用券或面板为空时重算）
        game.update_scenario_buff(false);
        Ok(game)
    }

    /// 追加一个人头（按当前人数分配人头序号）
    pub fn add_person(&mut self, mut person: BasePerson) {
        person.person_index = self.persons.len() as i32;
        self.persons.push(person);
    }

    /// 取剧本资源快照（**单一取数口**；何时取由宿主决定）
    #[must_use]
    pub fn view(&self) -> OnsenView {
        OnsenView {
            ticket_num: self.bathing.ticket_num,
            buff_remain_turn: self.bathing.buff_remain_turn,
            is_super_ready: self.bathing.is_super_ready,
        }
    }
}

/// 从引擎配置 + 剧本数据装配一局（终端样例外壳与剧本侧测试共用）
///
/// 引擎数据根、剧本数据根与配置都由宿主注入：本函数不碰任何路径。
pub fn build_game(cfg: &Config, data: Arc<OnsenData>) -> Result<OnsenGame> {
    OnsenGame::newgame(
        cfg.sim.uma,
        &cfg.sim.cards,
        cfg.inherit(),
        cfg.race_grades()?,
        data,
    )
}
