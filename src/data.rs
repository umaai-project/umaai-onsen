//! 剧本资产（`scenario_onsen.json`）：schema、载入与校验
//!
//! 剧本数据**不进引擎仓**：根目录由宿主注入（终端 / Web / Android 各按自家布局），
//! 本模块只负责「从给定位置读 + 校验」，不碰任何写死路径。
//!
//! 字段口径见 [`docs/glossary.md`](../docs/glossary.md)；旧实现对应
//! `umaai-rs-ref/crates/umasim/src/gamedata/onsen.rs`，这里按新仓约定重整（未知字段严格报错、
//! 载入期校验代替运行期 `expect`）。

use std::path::Path;

use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use umaai_core::Array5;
use umaai_core::gamedata::{ActionValue, EventData};

/// 剧本数据文件名（放在宿主注入的剧本数据根下）
pub const ONSEN_DATA_FILE: &str = "scenario_onsen.json";

/// 温泉剧本全部静态数据
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnsenData {
    /// 剧本 ID（12）
    pub scenario_id: i32,
    /// 女将首次点击事件的真实 story id（帧侧归一用，模拟不读）
    #[serde(default)]
    pub friend_first_event: u32,
    /// 五维上限基值（剧本独立；不含继承）
    pub five_status_limit_base: Array5,
    /// 链接角色 ID（历史字段，逻辑只用 `link_effect`）
    #[serde(default)]
    pub link_chara_id: Vec<u32>,
    /// 链接角色 → 对应地层（1 砂 / 2 土 / 3 岩）：命中时该地层挖掘力 +10%
    #[serde(default)]
    pub link_effect: std::collections::HashMap<u32, u32>,
    /// PR 活动的基础数值
    pub pr_base_value: ActionValue,
    /// 生涯比赛属性倍率（决赛期不乘）
    pub career_race_multiplier: f64,
    /// 训练基础值表：`[训练 0-4][等级 0-4] -> [速 耐 力 根 智 PT 体力消耗]`
    pub training_basic_value: Vec<Vec<[i32; 7]>>,
    /// 源泉列表（索引 0 为开局即已挖的基础温泉）
    pub onsen_info: Vec<OnsenInfo>,
    /// 超回复效果
    pub super_effect: SuperEffect,
    /// 旅馆等级档（按已挖源泉数取最高满足档）
    pub hotel_effect: Vec<HotelEffect>,
    /// 挖掘装备名（砂 / 土 / 岩）
    pub dig_tool_name: Vec<String>,
    /// 装备等级 → 挖掘力加成（等级 1–6 对应索引 1–6）
    pub dig_tool_level: Vec<i32>,
    /// 蓝因子（速耐力根智 × 地层）对挖掘力的加成
    pub dig_blue_bonus: Vec<[i32; 3]>,
    /// 属性档位阈值（五维落在哪一档）
    pub dig_stat_ranks: Vec<i32>,
    /// 属性档位加成（表行 × 档位）
    pub dig_stat_bonus: Vec<[i32; 8]>,
    /// 地层 → 属性 → 表行映射（值为属性下标 + 1）
    pub dig_stat_bonus_types: Vec<[i32; 3]>,
    /// 挖掘奖励：`[速 耐 力 根 智 体力]`
    pub dig_bonus: Vec<[i32; 6]>,
    /// 超回复概率档（按累计体力消耗每 50 一点一档）
    pub super_probs: Vec<i32>,
    /// 剧本固定回合事件
    #[serde(default)]
    pub scenario_events: Vec<EventData>,
}

/// 一座源泉
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnsenInfo {
    /// 源泉 id（1 基；配置与代码用索引 = id - 1）
    pub id: i32,
    /// 名称
    pub name: String,
    /// 解锁回合（0 基）
    pub unlock_turn: i32,
    /// 本地层需求量 `[砂, 土, 岩]`（0 表示无该地层）
    pub dig_volume: [i32; 3],
    /// 入浴效果
    pub effect: OnsenEffect,
}

/// 源泉效果（同一结构既表示单座源泉，也表示「已挖源泉效果之和」）
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnsenEffect {
    /// 入浴即时回复体力
    #[serde(default)]
    pub vital: i32,
    /// 入浴时羁绊最低的 N 人各加多少（当前数据与人头数都固定为 3 / 10）
    #[serde(default)]
    pub friendship: i32,
    /// 羁绊加成人数
    #[serde(default)]
    pub friendship_count: i32,
    /// 干劲
    #[serde(default)]
    pub motivation: i32,
    /// 训练效果（百分数，作用于上层）
    #[serde(default)]
    pub xunlian: i32,
    /// 失败率下降（百分点；温泉效果期间计入失败率）
    #[serde(default)]
    pub fail_rate_drop: i32,
    /// 友情加成 `[速 耐 力 根 智]`（百分数，作用于上层）
    #[serde(default)]
    pub youqing: [i32; 5],
    /// 生涯比赛加成（百分数；仅生涯指定比赛）
    #[serde(default)]
    pub career_race_bonus: i32,
    /// 红点发生率提升（百分数）
    #[serde(default)]
    pub hint_bonus: i32,
    /// 体力消耗下降（百分数）
    #[serde(default)]
    pub vital_cost_drop: i32,
    /// 秘汤汤驹的分身人数
    #[serde(default)]
    pub split: i32,
}

impl OnsenEffect {
    /// 累加另一份效果（已挖源泉效果求和）
    pub fn add_eq(&mut self, other: &Self) {
        self.vital += other.vital;
        self.friendship += other.friendship;
        self.friendship_count += other.friendship_count;
        self.motivation += other.motivation;
        self.xunlian += other.xunlian;
        self.fail_rate_drop += other.fail_rate_drop;
        for (a, b) in self.youqing.iter_mut().zip(other.youqing) {
            *a += b;
        }
        self.career_race_bonus += other.career_race_bonus;
        self.hint_bonus += other.hint_bonus;
        self.vital_cost_drop += other.vital_cost_drop;
        self.split += other.split;
    }
}

/// 超回复效果
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SuperEffect {
    /// 额外恢复体力
    pub vital: i32,
    /// 技能点
    pub pt: i32,
    /// 红点等级（历史字段：实现按 `hotel.hint + 2` 次数发放）
    pub hint: i32,
    /// 临时体力上限
    pub temp_max_vital: i32,
}

/// 旅馆等级档
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HotelEffect {
    /// 达到该档所需的已挖源泉数
    pub onsen_count: i32,
    /// 得意率加成
    pub deyilv: i32,
    /// 超回复时额外红点次数基数
    pub hint: i32,
}

impl OnsenData {
    /// 从剧本数据根载入（`<root>/scenario_onsen.json`）
    pub fn load(root: impl AsRef<Path>) -> Result<Self> {
        let path = root.as_ref().join(ONSEN_DATA_FILE);
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("读取剧本数据失败: {}", path.display()))?;
        Self::parse(&text).with_context(|| format!("剧本数据来源: {}", path.display()))
    }

    /// 从 JSON 文本解析并校验（内存 / 内嵌场景复用）
    pub fn parse(text: &str) -> Result<Self> {
        let data: Self = serde_json::from_str(text).context("解析剧本数据失败")?;
        data.validate()?;
        Ok(data)
    }

    /// 载入期校验：形状与索引一致性（把旧实现的运行期 `expect` 前移到这里）
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.scenario_id == 12,
            "scenario_id 应为 12，实际 {}",
            self.scenario_id
        );
        ensure!(!self.onsen_info.is_empty(), "onsen_info 不能为空");
        for (i, info) in self.onsen_info.iter().enumerate() {
            ensure!(
                info.id == i as i32 + 1,
                "onsen_info[{i}] 的 id 应为 {}，实际 {}",
                i + 1,
                info.id
            );
        }
        ensure!(
            self.training_basic_value.len() == 5,
            "training_basic_value 应有 5 个训练，实际 {}",
            self.training_basic_value.len()
        );
        for (i, levels) in self.training_basic_value.iter().enumerate() {
            ensure!(
                levels.len() == 5,
                "training_basic_value[{i}] 应有 5 个等级，实际 {}",
                levels.len()
            );
        }
        ensure!(
            self.dig_tool_level.len() > 6,
            "dig_tool_level 至少覆盖 1–6 级，实际长度 {}",
            self.dig_tool_level.len()
        );
        ensure!(
            self.dig_bonus.len() == 3,
            "dig_bonus 应有 3 个地层，实际 {}",
            self.dig_bonus.len()
        );
        ensure!(
            self.dig_blue_bonus.len() == 5,
            "dig_blue_bonus 应有 5 行（五维），实际 {}",
            self.dig_blue_bonus.len()
        );
        ensure!(
            self.dig_stat_bonus_types.len() == 3,
            "dig_stat_bonus_types 应有 3 个地层，实际 {}",
            self.dig_stat_bonus_types.len()
        );
        ensure!(!self.super_probs.is_empty(), "super_probs 不能为空");
        ensure!(!self.hotel_effect.is_empty(), "hotel_effect 不能为空");
        // 旅馆档按源泉数升序（实现按「从高到低取首个满足档」扫描）
        for pair in self.hotel_effect.windows(2) {
            ensure!(
                pair[0].onsen_count < pair[1].onsen_count,
                "hotel_effect 必须按 onsen_count 升序"
            );
        }
        for (chara, dig_type) in &self.link_effect {
            ensure!(
                (1..=3).contains(dig_type),
                "link_effect[{chara}] 的地层应为 1–3，实际 {dig_type}"
            );
        }
        Ok(())
    }

    /// 源泉数（配置与代码用索引 `0..len`）
    #[must_use]
    pub fn onsen_count(&self) -> usize {
        self.onsen_info.len()
    }

    /// 取训练基础值表引用（`TrainingValueSource` 用）
    #[must_use]
    pub fn training_table(&self) -> &Vec<Vec<[i32; 7]>> {
        &self.training_basic_value
    }

    /// 已挖源泉效果之和（索引集合由调用方给）
    #[must_use]
    pub fn sum_effects<'a>(&self, completed: impl Iterator<Item = &'a bool>) -> OnsenEffect {
        let mut ret = OnsenEffect::default();
        for (i, done) in completed.enumerate() {
            if *done && let Some(info) = self.onsen_info.get(i) {
                ret.add_eq(&info.effect);
            }
        }
        ret
    }

    /// 按已挖源泉数取旅馆档（取最高满足档；不足首档返回默认）
    #[must_use]
    pub fn hotel_rank(&self, dig_count: i32) -> HotelEffect {
        let mut ret = HotelEffect {
            onsen_count: 0,
            deyilv: 0,
            hint: 0,
        };
        for hotel in &self.hotel_effect {
            if dig_count >= hotel.onsen_count {
                ret = *hotel;
            }
        }
        ret
    }

    /// 取训练基础值（越界报错而不是静默取末档）
    pub fn training_row(&self, train: usize, level: usize) -> Result<[i32; 7]> {
        let levels = self
            .training_basic_value
            .get(train)
            .with_context(|| format!("训练下标越界: {train}"))?;
        levels
            .get(level)
            .copied()
            .with_context(|| format!("训练等级越界: train={train}, level={level}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 仓库自带数据可载入并通过校验
    #[test]
    fn test_load_repo_data() -> Result<()> {
        let data = OnsenData::load("data")?;
        assert_eq!(data.onsen_count(), 10);
        assert_eq!(data.hotel_effect.len(), 4);
        assert_eq!(data.scenario_events.len(), 5);
        println!(
            "载入成功：源泉 {} 座，旅馆 {} 档，剧本事件 {} 条",
            data.onsen_count(),
            data.hotel_effect.len(),
            data.scenario_events.len()
        );
        Ok(())
    }

    /// 未知字段严格报错
    #[test]
    fn test_unknown_field_rejected() {
        let text = std::fs::read_to_string("data/scenario_onsen.json").unwrap();
        let bad = text.replace("\"scenario_id\"", "\"scenario_id_typo\"");
        assert!(OnsenData::parse(&bad).is_err(), "未知字段应报错");
        println!("未知字段被拒绝");
    }

    /// 旅馆档必须升序
    #[test]
    fn test_hotel_effect_sorted() {
        let text = std::fs::read_to_string("data/scenario_onsen.json").unwrap();
        let mut data: OnsenData = serde_json::from_str(&text).unwrap();
        data.hotel_effect.reverse();
        assert!(data.validate().is_err(), "旅馆档逆序应报错");
        println!("旅馆档顺序校验生效");
    }

    /// 效果求和与旅馆档
    #[test]
    fn test_sum_effects_and_hotel() -> Result<()> {
        let data = OnsenData::load("data")?;
        // 开局只有索引 0 已挖
        let state: Vec<bool> = (0..data.onsen_count()).map(|i| i == 0).collect();
        let eff = data.sum_effects(state.iter());
        assert_eq!(eff.vital, 35, "基础温泉应回体 35");
        assert_eq!(eff.xunlian, 10);
        assert_eq!(data.hotel_rank(1).deyilv, 0, "1 座泉不足首档");
        assert_eq!(data.hotel_rank(3).deyilv, 40);
        assert_eq!(data.hotel_rank(7).hint, 2);
        println!("效果求和与旅馆档正确");
        Ok(())
    }
}
