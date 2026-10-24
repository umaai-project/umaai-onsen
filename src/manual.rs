//! 手动训练员（feature `manual`）：终端交互选择，人工验证剧本机制
//!
//! 依赖 `inquire`（交互式终端组件，仅终端宿主可用，故 feature 门控、不进默认依赖面）。
//! 每次决策前打印局面面板（回合 / 体力 / 券 / 挖掘 / 旅馆档 + 彩色人头分布表），玩家从
//! 候选中手选；取消（Esc / Ctrl+C）视为中止本局。事件选项按组展示各结果的概率与数值。
//!
//! 显示约定（颜色语义剧本自持，上游只出原语）：训练 / 源泉 / 装备用数据文件名
//! （`train_names` / `onsen_info.name` / `dig_tool_name`）；分布表每训练一列（列头
//! 「训练名 Lv 等级」），人物按类型着色——支援卡默认前景、友人 / 团队绿、彩圈支援卡
//! 亮绿、NPC 及理事长记者灰；支援卡叹号前缀亮黄、羁绊未满 100 附数值。

use anyhow::{Result, anyhow};
use inquire::Select;
use rand::rngs::StdRng;
use umaai_cli::render::table::Grid;
use umaai_cli::render::term;
use umaai_core::Data;
use umaai_core::gamedata::{EventChoice, EventData};
use umasim::game::{Game, Person, PersonType, Trainer};

use crate::game::{Action, OnsenGame};

/// ANSI 前景色：友人 / 团队卡
const ANSI_GREEN: &str = "32";
/// ANSI 前景色：彩圈支援卡
const ANSI_BRIGHT_GREEN: &str = "92";
/// ANSI 前景色：NPC / 理事长 / 记者
const ANSI_GRAY: &str = "90";
/// ANSI 前景色：Hint 叹号前缀
const ANSI_BRIGHT_YELLOW: &str = "93";

/// 手动训练员（终端交互）
#[derive(Debug, Clone, Copy, Default)]
pub struct ManualTrainer;

impl ManualTrainer {
    /// 决策面板：回合 / 马娘 / 温泉资源 + 人头分布表
    fn status_panel(game: &OnsenGame) -> String {
        let view = game.view();
        let uma = game.uma();
        let info = &game.data.onsen_info[view.current_onsen];
        let super_state = if view.is_super_ready {
            "就绪"
        } else {
            "未就绪"
        };
        let head = format!(
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
        );
        let color = term::use_color(false);
        format!("{head}\n{}", Self::distribution_grid(game, color))
    }

    /// 人头分布表：每训练一行——数值摘要一格、该训练位人物一格内多行
    fn distribution_grid(game: &OnsenGame, color: bool) -> String {
        let mut grid = Grid::new(["训练", "数值", "人物"]);
        for train in 0..5 {
            let head = format!("{} Lv{}", train_name(train), game.train_level(train));
            let members: Vec<String> = game.distribution()[train]
                .iter()
                .filter(|index| **index >= 0)
                .filter_map(|index| game.persons.get(*index as usize).map(|_| *index as usize))
                .map(|index| person_label(game, index, train, color))
                .collect();
            let person_cell = if members.is_empty() {
                "—".to_string()
            } else {
                members.join("\n")
            };
            grid.push([head, Self::training_value_cell(game, train), person_cell]);
        }
        grid.render(term::resolve_width(None))
    }

    /// 训练数值摘要格：实际增加数值（引擎 `ActionValue` 紧凑摘要）+ 成功率（未满 100% 才显示）
    ///
    /// 数值与 `OnsenTrainer` 打分同源（`calc_training_value` 的温泉上下层口径、卡片 buffs
    /// 的失败率），玩家手动决策与手写策略看到的是同一套数。
    fn training_value_cell(game: &OnsenGame, train: usize) -> String {
        let buffs = match game.calc_training_buff(train) {
            Ok(buffs) => buffs,
            Err(err) => return format!("（数值不可用: {err}）"),
        };
        let value = match game.calc_training_value(&buffs, train) {
            Ok(value) => value,
            Err(err) => return format!("（数值不可用: {err}）"),
        };
        let mut lines = vec![value.to_string()];
        if let Ok(rate) = game.calc_training_failure_rate(&buffs, train)
            && rate > 0.0
        {
            lines.push(format!("成功率{:.0}%", 100.0 - rate));
        }
        lines.join("\n")
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

    /// 动作展示名：训练 / 源泉 / 装备用数据文件名，其余沿用 `Action` 默认显示
    fn action_label(game: &OnsenGame, action: &Action) -> String {
        match action {
            Action::Train(train) => train_name(*train as usize),
            Action::Dig(index) => game
                .data
                .onsen_info
                .get(*index)
                .map(|info| info.name.clone())
                .unwrap_or_else(|| action.to_string()),
            Action::Upgrade(dig_type) => {
                let tool = game
                    .data
                    .dig_tool_name
                    .get(*dig_type)
                    .cloned()
                    .unwrap_or_else(|| dig_type.to_string());
                format!("升级{tool}")
            }
            _ => action.to_string(),
        }
    }

    /// 弹出选择列表，返回所选下标（取消即中止）
    fn select(message: &str, options: Vec<String>) -> Result<usize> {
        Select::new(message, options)
            .raw_prompt()
            .map(|answer| answer.index)
            .map_err(|err| anyhow!("手动选择被中止: {err}"))
    }
}

/// 单个人头的展示（含类型着色与标记）
fn person_label(game: &OnsenGame, person_index: usize, train: usize, color: bool) -> String {
    let person = &game.persons[person_index];
    let name = person.short_name();
    match person.person_type() {
        // 支援卡：彩圈亮绿（否则默认前景）；叹号前缀亮黄；羁绊未满 100 附数值
        PersonType::Card => {
            let body = if person.friendship() < 100 {
                format!("{name}{}", person.friendship())
            } else {
                name
            };
            let marked = if person.hint() {
                format!("{}{body}", paint("!", Some(ANSI_BRIGHT_YELLOW), color))
            } else {
                body
            };
            let code = game
                .is_shining_at(person_index, train)
                .then_some(ANSI_BRIGHT_GREEN);
            paint(&marked, code, color)
        }
        // 友人 / 团队卡：绿
        PersonType::ScenarioCard | PersonType::OtherFriend | PersonType::TeamCard => {
            paint(&name, Some(ANSI_GREEN), color)
        }
        // NPC / 理事长 / 记者：灰
        _ => paint(&name, Some(ANSI_GRAY), color),
    }
}

/// ANSI 前景色包装（不上色或无码时原样返回）
fn paint(text: &str, code: Option<&str>, color: bool) -> String {
    match code {
        Some(code) if color => format!("\x1b[{code}m{text}\x1b[0m"),
        _ => text.to_string(),
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

/// 训练名（引擎常量表 `train_names`；未载入时退化为下标）
fn train_name(train: usize) -> String {
    Data::try_get().map_or_else(
        || format!("训练{train}"),
        |data| data.consts.train_name(train),
    )
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
        let options: Vec<String> = actions
            .iter()
            .map(|action| Self::action_label(game, action))
            .collect();
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

    /// 上色包装：不上色或无码时原样返回，上色时保留原文
    #[test]
    fn test_paint() {
        assert_eq!(paint("x", Some("93"), false), "x", "关闭着色应原样返回");
        assert_eq!(paint("x", None, true), "x", "无码应原样返回");
        let painted = paint("x", Some("93"), true);
        assert!(
            painted.contains("x") && painted.contains('\x1b'),
            "上色应保留原文: {painted:?}"
        );
    }
}
