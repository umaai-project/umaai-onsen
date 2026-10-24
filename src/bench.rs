//! 数值基线（B6）：固定种子一局的终局快照，供「变更探测器」比对
//!
//! 基线 JSON 由本仓自持（`baselines/`），记录只含确定性字段（终局数值），不含路径 /
//! 时间戳。驱动方式与 `Runner` 同序同种子（规则主种子 + 决策流都取同一 seed），因此
//! 与 bin / Runner 路径逐位一致；测试与将来的测卡入口复用同一份记录结构。

use std::path::Path;

use anyhow::{Context, Result};
use rand::SeedableRng;
use rand::rngs::StdRng;
use serde::{Deserialize, Serialize};
use umasim::game::Game;

use crate::game::OnsenGame;
use crate::trainer::OnsenTrainer;

/// 固定种子一局的基线记录（剧本仓自持 JSON 的 schema）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaselineRecord {
    /// 规则主种子（决策流同源播种）
    pub seed: u64,
    /// 终局评分
    pub score: i32,
    /// 终局 PT
    pub pt: i32,
    /// 终局五维
    pub five_status: [i32; 5],
    /// 终局回合（0 基）
    pub turn: i32,
    /// 已挖通源泉数
    pub dig_count: i32,
    /// 各源泉挖通状态
    pub onsen_state: Vec<bool>,
    /// 挖掘装备等级 `[砂 土 岩]`
    pub dig_level: [i32; 3],
    /// 终局正在挖掘的源泉索引
    pub current_onsen: usize,
    /// 剩余温泉券
    pub ticket_num: i32,
    /// 终局技能点
    pub skill_pt: i32,
}

impl BaselineRecord {
    /// 直连驱动一局并采集终局（局面会被推进到终局）
    pub fn run(seed: u64, game: &mut OnsenGame, trainer: &OnsenTrainer) -> Result<Self> {
        let mut decision = StdRng::seed_from_u64(seed);
        game.set_rule_master(seed);
        game.run_full_game(trainer, &mut decision)?;
        Self::from_game(seed, game)
    }

    /// 从终局局面采集（不驱动）
    fn from_game(seed: u64, game: &OnsenGame) -> Result<Self> {
        let view = game.view();
        Ok(Self {
            seed,
            score: game.uma().calc_score()?,
            pt: game.uma().total_pt()?,
            five_status: game.uma().five_status,
            turn: game.turn(),
            dig_count: view.dig_count,
            onsen_state: view.onsen_state,
            dig_level: view.dig_level,
            current_onsen: view.current_onsen,
            ticket_num: view.ticket_num,
            skill_pt: game.uma().skill_pt,
        })
    }

    /// 从基线文件载入
    pub fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("读取基线失败: {}", path.display()))?;
        serde_json::from_str(&text).with_context(|| format!("基线文件: {}", path.display()))
    }

    /// 写出基线文件（带缩进，便于人工核对与版本比对）
    pub fn save(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let text = serde_json::to_string_pretty(self).context("基线序列化失败")?;
        std::fs::write(path, text).with_context(|| format!("写出基线失败: {}", path.display()))?;
        Ok(())
    }

    /// 与另一份记录逐字段比对，返回差异列表（空 = 一致）
    #[must_use]
    pub fn diff(&self, other: &Self) -> Vec<String> {
        let mut out = Vec::new();
        diff_field(&mut out, "seed", &self.seed, &other.seed);
        diff_field(&mut out, "score", &self.score, &other.score);
        diff_field(&mut out, "pt", &self.pt, &other.pt);
        diff_field(
            &mut out,
            "five_status",
            &self.five_status,
            &other.five_status,
        );
        diff_field(&mut out, "turn", &self.turn, &other.turn);
        diff_field(&mut out, "dig_count", &self.dig_count, &other.dig_count);
        diff_field(
            &mut out,
            "onsen_state",
            &self.onsen_state,
            &other.onsen_state,
        );
        diff_field(&mut out, "dig_level", &self.dig_level, &other.dig_level);
        diff_field(
            &mut out,
            "current_onsen",
            &self.current_onsen,
            &other.current_onsen,
        );
        diff_field(&mut out, "ticket_num", &self.ticket_num, &other.ticket_num);
        diff_field(&mut out, "skill_pt", &self.skill_pt, &other.skill_pt);
        out
    }

    /// 单行摘要（打印用）
    #[must_use]
    pub fn summary_line(&self) -> String {
        format!(
            "评分 {}，PT {}，五维 {:?}，已挖 {}，装备 {:?}，券 {}",
            self.score, self.pt, self.five_status, self.dig_count, self.dig_level, self.ticket_num
        )
    }
}

/// 逐字段比对辅助（不等才记一行差异）
fn diff_field<T: PartialEq + std::fmt::Debug>(
    out: &mut Vec<String>,
    name: &str,
    base: &T,
    current: &T,
) {
    if base != current {
        out.push(format!("{name}: 基线 {base:?} → 当前 {current:?}"));
    }
}
