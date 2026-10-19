//! # 温泉剧本仓（`umaai-onsen`）
//!
//! 引擎仓 `umaai-next` 的温泉剧本（ごくらく♪ゆこま温泉郷 / scenarioId 12）实现。
//! 本 crate 只依赖引擎稳定面（`umaai-core` / `umasim` / `umaai`），**不改引擎一行**。
//!
//! ## 结构
//!
//! - 局面 [`OnsenGame`]：引擎状态底座 [`BaseGame`] + 人头 + 温泉资源 + 私有阶段机；
//! - 动作 [`Action`]：训练 / 比赛 / 休息 / 出行 / 治病 / PR / 入浴 / 源泉选择 / 装备升级；
//! - 配置 [`OnsenConfig`]：剧本专属段（源泉选择顺序等），由宿主注入；
//! - 数据 [`OnsenData`]：剧本资产（源泉 / 挖掘 / 事件表），由宿主注入根目录。
//!
//! ## 纪律（见引擎仓 `docs/scenario/`）
//!
//! - **阶段自持**：内部强类型枚举，对外只投影 [`StageId`]（显式赋号，改号＝旧种子失效）；
//! - **三条随机轴**：局面流 / 事件流从 [`BaseGame`] 取出，策略流本局面自持，决策流只给 `Trainer`；
//! - **主种子必注入**：`set_rule_master` 是唯一注入点，未注入就推进即报错；
//! - **数据与配置由宿主注入**：本 crate 不写死路径。
//!
//! 当前进度：M1a A1（骨架）/ A2（数据层）已完成；训练上层加成、挖掘、入浴与超回复按 A4/A5 推进。

use std::fmt::Display;
use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use anyhow::{Result, anyhow};
use rand::Rng;
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};
use umaai::Config;
use umaai_core::Data;
use umaai_core::gamedata::EventData;
use umaai_core::rng::{StrategyRng, StreamTag, derive_seed};
use umasim::game::base::{BaseAction, BaseGame, BasePerson, RuleStreams};
use umasim::game::{
    ActionEnum, Game, InheritInfo, StageId, SupportCard, Trainer, TrainingBasicTable,
    TrainingValueSource, Uma,
};

pub mod data;

pub use data::{HotelEffect, OnsenData, OnsenEffect, OnsenInfo, SuperEffect};

/// 育成最大回合（0 基，78 回合）
pub const MAX_TURN: i32 = 77;

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
    fn next(self) -> Option<Self> {
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
        let mut game = Self {
            base: BaseGame::new(
                uma_id,
                deck_ids,
                inherit,
                // 五维上限归剧本：用剧本数据，不用引擎常量
                data.five_status_limit_base,
                race_grades,
            )?,
            persons: Vec::new(),
            stage: Stage::Begin,
            strategy: None,
            bathing: BathingInfo::default(),
            data,
        };
        game.init_persons()?;
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

    /// 训练结算（M1a 骨架版：失败判定 → 数值入账 → 通用后处理）
    ///
    /// A4 起补：温泉训练上下层加成、挖掘、失败率下降、超回复判定。
    fn do_train(&mut self, train: usize, rng: &mut impl Rng) -> Result<()> {
        if train >= 5 {
            return Err(anyhow!("训练等级越界: {train}"));
        }
        let buffs = self.calc_training_buff(train)?;
        let failure_rate = self.calc_training_failure_rate(&buffs, train)? / 100.0;
        if rng.random_bool(failure_rate as f64) {
            // 再判定一次，仍失败即大失败
            if rng.random_bool(failure_rate as f64) {
                self.apply_event(
                    &Data::get()?.system_event("training_fail_low")?.clone(),
                    0,
                    rng,
                )?;
                self.uma_mut().flags.bad_trainer = true;
            } else {
                self.apply_event(&Data::get()?.system_event("training_fail")?.clone(), 0, rng)?;
            }
            return Ok(());
        }
        let value = self.calc_training_value(&buffs, train)?;
        self.uma_mut().add_value(&value);
        self.base.train_level_count[train] += 1;
        // 通用后处理（羁绊 / 红点 / 加练 / 友人点击）走引擎可复用件
        self.base
            .default_after_train(&mut self.persons, train, rng)?;
        Ok(())
    }

    /// 派生本局面自持的策略流（注入点与回合边界调用）
    fn reset_strategy_stream(&mut self) {
        let turn = self.base.turn as u64;
        self.strategy = self.base.master().map(|master| {
            StrategyRng::new(derive_seed(master, &[turn, StreamTag::Strategy.tag()]))
        });
    }

    /// 单阶段推进
    fn stage_body<T: Trainer<Self>>(
        &mut self,
        trainer: &T,
        decision: &mut StdRng,
        streams: &mut RuleStreams,
        strategy: &mut StrategyRng,
    ) -> Result<()> {
        match self.stage {
            Stage::Begin => {
                // A4/A6 起：解锁与再选择、事件链、发券、育成结束事件
                let _ = (&mut streams.event, &mut streams.turn_fixed);
            }
            Stage::Distribute => {
                if self.base.uma.is_race_turn(self.base.turn) {
                    self.reset_distribution();
                } else {
                    self.distribute_all(&mut streams.turn_fixed)?;
                    // 温泉效果期间红点发生率提升由 A4 传入对应系数
                    self.distribute_hint(&mut streams.turn_fixed)?;
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

impl TrainingValueSource for OnsenGame {
    /// 训练基础值表归剧本数据（`scenario_onsen.json`）
    fn training_basic_table(&self) -> &TrainingBasicTable {
        self.data.training_table()
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
    fn apply_onsen_action(&mut self, action: &Action, rng: &mut impl Rng) -> Result<()> {
        match action {
            Action::Train(train) => self.do_train(*train as usize, rng),
            Action::Race => {
                // 生涯赛剧本倍率与源泉加成由 A6 补；此处用引擎默认系数
                let scale = (100 + self.base.uma.race_bonus) as f64 / 100.0;
                self.base.push_race_event(scale, 1.0)
            }
            Action::Sleep => BaseAction::do_sleep(&mut self.base, rng),
            Action::NormalOuting => BaseAction::do_normal_outing(&mut self.base, rng),
            Action::Clinic => BaseAction::do_clinic(&mut self.base, rng),
            Action::FriendOuting => BaseAction::do_friend_outing(&mut self.base, rng),
            // A5 起：PR 的活动数值、入浴券发放与超回复判定
            Action::PR => {
                let _ = rng;
                Ok(())
            }
            // A5 起：使用温泉券（效果叠加 / 超回复 / 临时上限）
            Action::UseTicket(use_ticket) => {
                if *use_ticket {
                    if self.bathing.ticket_num <= 0 {
                        return Err(anyhow!("没有温泉券"));
                    }
                    if self.bathing.buff_remain_turn > 0 {
                        return Err(anyhow!("温泉效果尚未结束"));
                    }
                    self.bathing.ticket_num -= 1;
                    self.bathing.buff_remain_turn = 2;
                    self.bathing.is_super = self.bathing.is_super_ready;
                    self.bathing.is_super_ready = false;
                }
                Ok(())
            }
            // A4 起：选择源泉（解锁校验 / 剩余量初始化）
            Action::Dig(_index) => Ok(()),
            // A4 起：升级挖掘装备（等级上限 6）
            Action::Upgrade(_dig_type) => Ok(()),
        }
    }
}

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

    fn absent_rate_drop(&self) -> i32 {
        self.base.absent_rate_drop
    }

    fn deyilv(&mut self, person_index: i32) -> f32 {
        // A4 起：叠加旅馆等级的得意率加成
        if person_index >= 0 && (person_index as usize) < self.deck.len() {
            let eff = self.deck[person_index as usize].calc_training_effect(self, 0);
            let deyilv = eff.deyilv;
            self.deck[person_index as usize].effect = eff;
            self.deck[person_index as usize].is_locked = true;
            deyilv
        } else {
            0.0
        }
    }

    fn has_group_buff(&self) -> bool {
        self.base.friend.group_buff_turn > 0
    }

    fn train_level(&self, train: usize) -> usize {
        (self.base.train_level_count[train] as usize / 4 + 1).clamp(1, 5)
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
