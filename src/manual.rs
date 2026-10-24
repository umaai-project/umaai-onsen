//! 手动训练员（feature `manual`）：终端交互选择，人工验证剧本机制
//!
//! 依赖 `inquire`（交互式终端组件，仅终端宿主可用，故 feature 门控、不进默认依赖面）。
//! 每次决策前打印局面面板（回合 / 体力 / 券 / 挖掘 / 旅馆档），玩家从候选中手选；
//! 取消（Esc / Ctrl+C）视为中止本局。事件选项按组展示各结果的概率与数值。

use anyhow::{Result, anyhow};
use inquire::Select;
use rand::rngs::StdRng;
use umaai_core::gamedata::{EventChoice, EventData};
use umasim::game::{Game, Trainer};

use crate::game::{Action, OnsenGame};

/// 手动训练员（终端交互）
#[derive(Debug, Clone, Copy, Default)]
pub struct ManualTrainer;

impl ManualTrainer {
    /// 决策面板：回合 / 马娘 / 温泉资源摘要
    fn status_panel(game: &OnsenGame) -> String {
        let view = game.view();
        let uma = game.uma();
        let info = &game.data.onsen_info[view.current_onsen];
        let super_state = if view.is_super_ready {
            "就绪"
        } else {
            "未就绪"
        };
        format!(
            "── 回合 {}/{} ── 体力 {}/{} 干劲 {} 五维 {:?}\n\
             温泉券 {} · 效果剩余 {} 回合（超回复 {super_state}）\n\
             挖掘：{}/{} 座已挖，当前 {} · 剩余 {:?} · 力 {:?} · 装备 {:?}\n\
             旅馆档：得意 +{} / 超回复红点 +{} · 累计挖掘消耗 {}（超回复分档用）",
            game.turn() + 1,
            game.max_turn() + 1,
            uma.vital,
            uma.max_vital,
            uma.motivation,
            uma.five_status,
            view.ticket_num,
            view.buff_remain_turn,
            view.dig_count,
            game.data.onsen_count(),
            info.name,
            view.dig_remain[view.current_onsen],
            view.dig_power,
            view.dig_level,
            view.hotel.deyilv,
            view.hotel.hint,
            view.dig_vital_cost,
        )
    }

    /// 事件选项组的展示行：组内各结果的档名、概率与数值
    fn choice_label(choice: &[EventChoice]) -> String {
        choice
            .iter()
            .map(|row| {
                let prob = if row.prob == 0 {
                    String::new()
                } else {
                    format!("（{}%）", row.prob)
                };
                let value = row.value.to_string();
                if value.is_empty() {
                    format!("[{}]{prob}", result_name(row.result))
                } else {
                    format!("[{}] {value}{prob}", result_name(row.result))
                }
            })
            .collect::<Vec<_>>()
            .join(" / ")
    }

    /// 弹出选择列表，返回所选下标（取消即中止）
    fn select(message: &str, options: Vec<String>) -> Result<usize> {
        Select::new(message, options)
            .raw_prompt()
            .map(|answer| answer.index)
            .map_err(|err| anyhow!("手动选择被中止: {err}"))
    }
}

impl Trainer<OnsenGame> for ManualTrainer {
    fn select_action(
        &self,
        game: &OnsenGame,
        actions: &[Action],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        if actions.len() <= 1 {
            return Ok(0);
        }
        println!("{}", Self::status_panel(game));
        let options: Vec<String> = actions.iter().map(ToString::to_string).collect();
        Self::select("选择动作", options)
    }

    fn select_choice(
        &self,
        _game: &OnsenGame,
        choices: &[Vec<EventChoice>],
        _rng: &mut StdRng,
    ) -> Result<usize> {
        if choices.len() <= 1 {
            return Ok(0);
        }
        let labels: Vec<String> = choices.iter().map(|c| Self::choice_label(c)).collect();
        Self::select("选择事件选项", labels)
    }

    fn select_event_choice(
        &self,
        game: &OnsenGame,
        event: &EventData,
        choices: &[Vec<EventChoice>],
        rng: &mut StdRng,
    ) -> Result<usize> {
        if choices.len() <= 1 {
            return Ok(0);
        }
        println!("事件：{}（id {}）", event.name, event.id);
        self.select_choice(game, choices, rng)
    }
}

/// 事件结果档名（`ChoiceResult` 的 i32 值 → 中文）
fn result_name(result: i32) -> &'static str {
    match result {
        1 => "成功",
        2 => "大成功",
        3 => "失败",
        4 => "大失败",
        _ => "普通",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use umaai_core::gamedata::ActionValue;

    /// 选项组展示：档名 / 概率 / 数值拼接
    #[test]
    fn test_choice_label() {
        let label = ManualTrainer::choice_label(&[
            EventChoice {
                result: 2,
                prob: 30,
                value: ActionValue {
                    status_pt: [10, 0, 0, 0, 0, 5],
                    vital: -10,
                    ..Default::default()
                },
                ..Default::default()
            },
            EventChoice {
                result: 0,
                prob: 70,
                value: ActionValue {
                    status_pt: [5, 0, 0, 0, 0, 0],
                    ..Default::default()
                },
                ..Default::default()
            },
        ]);
        assert!(label.contains("[大成功]"), "应含档名：{label}");
        assert!(label.contains("（30%）"), "应含概率：{label}");
        assert!(label.contains("PT+5"), "应含数值：{label}");
        assert!(label.contains(" / "), "组内结果应以 / 分隔：{label}");
        println!("{label}");
    }

    /// 零数值结果只显示档名与概率
    #[test]
    fn test_choice_label_empty_value() {
        let label = ManualTrainer::choice_label(&[EventChoice::default()]);
        assert_eq!(label, "[普通]");
    }
}
