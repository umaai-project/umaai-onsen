//! 手动训练员：终端交互选择，人工验证剧本机制
//!
//! 归 `onsen-cli`（终端前端业务：强交互、逻辑不复杂，crate 边界替代原先的 feature 门控）。
//! 依赖 `inquire`（交互式终端组件，仅终端宿主可用）。
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
use umaai_cli::render::panel::key_values;
use umaai_cli::render::table::Grid;
use umaai_cli::render::term;
use umaai_core::Data;
use umaai_core::gamedata::{ActionValue, EventChoice, EventData};
use umasim::game::{Game, Person, PersonType, Trainer};

use onsen_core::game::{Action, OnsenGame};

/// ANSI 前景色：友人 / 团队卡
const ANSI_GREEN: &str = "32";
/// ANSI 前景色：彩圈支援卡
const ANSI_BRIGHT_GREEN: &str = "92";
/// ANSI 前景色：NPC / 理事长 / 记者
const ANSI_GRAY: &str = "90";
/// ANSI 前景色：Hint 叹号前缀
const ANSI_BRIGHT_YELLOW: &str = "93";
/// ANSI 前景色：红色（消耗 / 负面）
const ANSI_RED: &str = "31";
/// ANSI 前景色：亮青（行动资源 / 休息心得）
const ANSI_BRIGHT_CYAN: &str = "96";

/// 手动训练员（终端交互）
#[derive(Debug, Clone, Copy, Default)]
pub struct ManualTrainer;

impl ManualTrainer {
    /// 决策面板：回合 / 温泉资源（效果三态着色）/ 事件回显 + 人头分布表 + 训练数值行
    fn status_panel(game: &OnsenGame) -> String {
        let view = game.view();
        let uma = game.uma();
        let info = &game.data.onsen_info[view.current_onsen];
        let color = term::use_color(false);
        // 温泉效果三态：超回复档生效 / 普通生效（亮绿）/ 未生效
        let effect = if view.is_super {
            paint(
                &format!("超回复生效中 残{}回合", view.buff_remain_turn),
                Some(ANSI_BRIGHT_GREEN),
                color,
            )
        } else if view.buff_remain_turn > 0 {
            paint(
                &format!("生效中 残{}回合", view.buff_remain_turn),
                Some(ANSI_BRIGHT_GREEN),
                color,
            )
        } else {
            "未生效".to_string()
        };
        let ready = if view.is_super_ready {
            format!(
                " · {}",
                paint("下次入浴超回复", Some(ANSI_BRIGHT_GREEN), color)
            )
        } else {
            String::new()
        };
        // 体力色阶（具体数值：>100 亮青（超回复溢出区）/ >70 绿 / 40-70 黄 / <40 红）
        let vital_code = if uma.vital > 100 {
            "96"
        } else if uma.vital > 70 {
            "32"
        } else if uma.vital >= 40 {
            "33"
        } else {
            "31"
        };
        let vital_text = paint(
            &format!("体力 {}/{}", uma.vital, uma.max_vital),
            Some(vital_code),
            color,
        );
        // 超回复累计（距上次超回复已消耗体力；<100 绿 / 100-200 黄 / >200 红）
        let cost_code = if view.dig_vital_cost > 200 {
            "31"
        } else if view.dig_vital_cost >= 100 {
            "33"
        } else {
            "32"
        };
        let cost_text = paint(
            &format!("超回复累计 {}", view.dig_vital_cost),
            Some(cost_code),
            color,
        );
        // 马娘名（umaDB 查名；未载入退化为 id）
        let uma_name = uma
            .get_data()
            .map(|row| row.short_name().to_string())
            .unwrap_or_else(|_| format!("#{}", uma.uma_id));
        // 五维行：名字 + 各轴数值（>1200 部分减半显示，与旧仓 explain 口径一致）+ PT；近上限（≥90%，按原始值）亮黄警示
        let five: Vec<String> = (0..5)
            .map(|i| {
                let shown = display_stat(uma.five_status[i]);
                let text = format!("{} {}", train_name(i), shown);
                let near_limit = uma.five_status[i] * 10 >= uma.five_status_limit[i] * 9;
                let code = near_limit.then_some(ANSI_BRIGHT_YELLOW);
                paint(&text, code, color)
            })
            .collect();
        let five_line = format!("{uma_name} {} PT {}", five.join(" "), uma.skill_pt);
        // 干劲档文案 + 着色（绝好调亮绿 / 好调绿 / 普通默认 / 不调灰）
        let motivation_index = uma.motivation.clamp(0, 4) as usize;
        let motivation_name = Data::get()
            .map(|data| {
                data.consts
                    .motivation_names
                    .get(motivation_index)
                    .cloned()
                    .unwrap_or_else(|| format!("#{}", uma.motivation))
            })
            .unwrap_or_else(|_| format!("#{}", uma.motivation));
        let motivation_code = match uma.motivation {
            4 => Some(ANSI_BRIGHT_GREEN),
            3 => Some(ANSI_GREEN),
            2 => None,
            _ => Some(ANSI_GRAY),
        };
        let motivation_text = paint(&format!("干劲 {motivation_name}"), motivation_code, color);
        // 状态行：休息心得（亮青）与 UmaFlags 摘要
        let flags = &uma.flags;
        let mut states: Vec<String> = Vec::new();
        if flags.refresh_mind > 0 {
            states.push(paint(
                &format!("休息心得 已持续{}回合", flags.refresh_mind),
                Some(ANSI_BRIGHT_CYAN),
                color,
            ));
        }
        if flags.aijiao {
            states.push(paint("爱娇", Some(ANSI_BRIGHT_GREEN), color));
        }
        if flags.qiezhe {
            states.push(paint("切者", Some(ANSI_BRIGHT_YELLOW), color));
        }
        if flags.xiaoqie {
            states.push(paint("小切", Some(ANSI_BRIGHT_YELLOW), color));
        }
        if flags.good_trainer {
            states.push(paint("擅长", Some(ANSI_GREEN), color));
        }
        if flags.bad_trainer {
            states.push(paint("不擅长", Some(ANSI_RED), color));
        }
        if !flags.ill.is_empty() {
            states.push(paint("病", Some(ANSI_RED), color));
        }
        if flags.positive_thinking_count > 0 {
            states.push(paint(
                &format!("正向思考{}", flags.positive_thinking_count),
                Some(ANSI_GREEN),
                color,
            ));
        }
        if flags.lucky_count > 0 {
            states.push(paint(
                &format!("幸运{}", flags.lucky_count),
                Some(ANSI_GREEN),
                color,
            ));
        }
        let state_line = if states.is_empty() {
            String::new()
        } else {
            format!("状态：{}\n", states.join(" · "))
        };
        // 温泉券：有券（可入浴）亮青
        let ticket_text = if view.ticket_num > 0 {
            paint(
                &format!("温泉券 {}", view.ticket_num),
                Some(ANSI_BRIGHT_CYAN),
                color,
            )
        } else {
            format!("温泉券 {}", view.ticket_num)
        };
        let head = format!(
            "── 回合 {}/{} ──\n{}\n\
             {five_line}\n\
             {vital_text} · {cost_text} · {motivation_text}\n\
             {state_line}\
             {ticket_text} · 效果 {effect}{ready}\n\
             挖掘：{}/{} 座已挖，当前 {} · 剩余 {} · 力 {:?} · 装备 {:?}\n\
             旅馆档：得意 +{} / 超回复红点 +{}",
            game.turn() + 1,
            game.max_turn() + 1,
            divider(),
            view.dig_count,
            game.data.onsen_count(),
            info.name,
            dig_remain_colored(&view.dig_remain[view.current_onsen], color),
            view.dig_power,
            view.dig_level,
            view.hotel.deyilv,
            view.hotel.hint,
        );
        let mut sections = vec![head];
        sections.push(Self::distribution_grid(game, color));
        sections.push(key_values(&Self::training_value_rows(game, color)));
        sections.join("\n")
    }

    /// 人头分布表：每训练一列（列头「训练名 Lv 等级」），一格内多行列出该训练位全部人物；
    /// 末行是各训练的本次挖掘量（无挖掘窗口时 `—`）
    ///
    /// 各列按全局最大显示宽度补齐（彩色不计宽），列宽一致。
    fn distribution_grid(game: &OnsenGame, color: bool) -> String {
        let mut grid = Grid::new(
            (0..5).map(|train| format!("{} Lv{}", train_name(train), game.train_level(train))),
        );
        let mut rows: Vec<Vec<String>> = vec![Self::person_cells(game, color)];
        rows.push(Self::dig_cells(game));
        // 等宽：所有格的所有行取最大显示宽，逐行补空格
        let width = rows
            .iter()
            .flat_map(|cells| cells.iter())
            .flat_map(|cell| cell.lines())
            .map(term::text_width)
            .max()
            .unwrap_or(0);
        for cells in rows {
            let padded: Vec<String> = cells
                .iter()
                .map(|cell| {
                    cell.lines()
                        .map(|line| pad_to(line, width))
                        .collect::<Vec<_>>()
                        .join("\n")
                })
                .collect();
            grid.push(padded);
        }
        grid.render(term::resolve_width(None))
    }

    /// 各训练位的人物格（一格内多行）
    fn person_cells(game: &OnsenGame, color: bool) -> Vec<String> {
        (0..5)
            .map(|train| {
                let members: Vec<String> = game.distribution()[train]
                    .iter()
                    .filter(|index| **index >= 0)
                    .filter_map(|index| game.persons.get(*index as usize).map(|_| *index as usize))
                    .map(|index| person_label(game, index, train, color))
                    .collect();
                if members.is_empty() {
                    "—".to_string()
                } else {
                    members.join("\n")
                }
            })
            .collect()
    }

    /// 各训练的本次挖掘量格（`calc_dig_value` 三层求和；无挖掘窗口时 `—`）
    fn dig_cells(game: &OnsenGame) -> Vec<String> {
        (0..5)
            .map(|train| match game.calc_dig_value(&Action::Train(train)) {
                Some(value) => format!("挖+{}", value.iter().sum::<i32>()),
                None => "—".to_string(),
            })
            .collect()
    }

    /// 训练数值行（表格下方逐训练一行）：实际增加数值 + 成功率（未满 100% 才显示）
    ///
    /// 数值与 `OnsenTrainer` 打分同源（`calc_training_value` 的温泉上下层口径、卡片 buffs
    /// 的失败率），玩家手动决策与手写策略看到的是同一套数。
    fn training_value_rows(game: &OnsenGame, color: bool) -> Vec<(String, String)> {
        (0..5)
            .map(|train| {
                (
                    train_name(train),
                    Self::training_value_text(game, train, color)
                        .unwrap_or_else(|| "—".to_string()),
                )
            })
            .collect()
    }

    /// 单个训练的实际增加数值文本（收益绿 / 体力消耗红）+ 成功率（未满 100% 亮黄）
    fn training_value_text(game: &OnsenGame, train: usize, color: bool) -> Option<String> {
        let buffs = game.calc_training_buff(train).ok()?;
        let value = game.calc_training_value(&buffs, train).ok()?;
        let mut text = colored_action_value(&value, color);
        if text.is_empty() {
            text.push('—');
        }
        if let Ok(rate) = game.calc_training_failure_rate(&buffs, train)
            && rate > 0.0
        {
            text.push_str(&paint(
                &format!(" 成功率{:.0}%", 100.0 - rate),
                Some(ANSI_BRIGHT_YELLOW),
                color,
            ));
        }
        Some(text)
    }

    /// 事件选项组的展示行：组内各结果的档名、概率与数值（`Hint+N` 口径）
    fn choice_label(choice: &[EventChoice]) -> String {
        choice
            .iter()
            .map(|row| {
                let prob = if row.prob == 0 {
                    String::new()
                } else {
                    format!("（{}%）", row.prob)
                };
                let value = onsen_core::game::value_summary(&row.value);
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
///
/// 羁绊数值：除 NPC 外，未满 100 的一律尾附数值（支援卡 / 友人 / 团队 / 理事长 / 记者）。
fn person_label(game: &OnsenGame, person_index: usize, train: usize, color: bool) -> String {
    let person = &game.persons[person_index];
    let name = person.short_name();
    let body = if person.person_type() != PersonType::Npc && person.friendship() < 100 {
        format!("{name}{}", person.friendship())
    } else {
        name
    };
    match person.person_type() {
        // 支援卡：彩圈亮绿（否则默认前景）；叹号前缀亮黄
        PersonType::Card => {
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
            paint(&body, Some(ANSI_GREEN), color)
        }
        // NPC / 理事长 / 记者：灰
        _ => paint(&body, Some(ANSI_GRAY), color),
    }
}

/// 按显示宽度右补空格（彩色不计宽；超宽原样返回）
fn pad_to(text: &str, width: usize) -> String {
    let used = term::text_width(text);
    if used >= width {
        text.to_string()
    } else {
        format!("{text}{}", " ".repeat(width - used))
    }
}

/// 面板分隔线（终端宽的三分之一，单线 `─`）
fn divider() -> String {
    "─".repeat(term::resolve_width(None) / 3)
}

/// 属性显示值：超过 1200 的部分减半（`1200 + (stat - 1200) / 2`，与旧仓 `explain.rs` 同口径）
fn display_stat(stat: i32) -> i32 {
    if stat <= 1200 {
        stat
    } else {
        1200 + (stat - 1200) / 2
    }
}

/// 源泉各层剩余量的分段着色：0 灰（空）/ ≤50 亮绿（近穿）/ 其余默认
fn dig_remain_colored(remain: &[i32; 3], color: bool) -> String {
    let parts: Vec<String> = remain
        .iter()
        .map(|&v| {
            let code = if v == 0 {
                Some(ANSI_GRAY)
            } else if v <= 50 {
                Some(ANSI_BRIGHT_GREEN)
            } else {
                None
            };
            paint(&v.to_string(), code, color)
        })
        .collect();
    format!("[{}]", parts.join(", "))
}

/// 训练数值摘要：非零字段默认色、体力消耗红（与引擎 `ActionValue` Display 同字段序）
fn colored_action_value(value: &ActionValue, _color: bool) -> String {
    let mut parts: Vec<String> = Vec::new();
    for (i, &pt) in value.status_pt.iter().take(5).enumerate() {
        if pt != 0 {
            parts.push(format!("{}{:+}", train_name(i), pt));
        }
    }
    if value.status_pt[5] != 0 {
        parts.push(format!("PT{:+}", value.status_pt[5]));
    }
    if value.motivation != 0 {
        parts.push(format!("干劲{:+}", value.motivation));
    }
    if value.vital != 0 {
        let code = if value.vital < 0 {
            Some(ANSI_RED)
        } else {
            None
        };
        parts.push(paint(&format!("体{:+}", value.vital), code, _color));
    }
    if value.friendship != 0 {
        parts.push(format!("羁绊{:+}", value.friendship));
    }
    parts.join(" ")
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
        // 空防御：单选项（如比赛）也交给玩家确认
        if actions.is_empty() {
            return Ok(0);
        }
        // 空行隔开上一次刷新
        println!();
        println!("{}", Self::status_panel(game));
        // 源泉选择时列出各可选温泉的挖掘进度
        let digs: Vec<usize> = actions
            .iter()
            .filter_map(|action| match action {
                Action::Dig(i) => Some(*i),
                _ => None,
            })
            .collect();
        if !digs.is_empty() {
            let color = term::use_color(false);
            let view = game.view();
            let rows: Vec<(String, String)> = digs
                .iter()
                .map(|&i| {
                    let progress = &view.dig_progress[i];
                    let remain = dig_remain_colored(&view.dig_remain[i], color);
                    (
                        game.data.onsen_info[i].name.clone(),
                        format!("进度 {progress:?} / 30 · 剩余 {remain}"),
                    )
                })
                .collect();
            println!("{}", key_values(&rows));
        }
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
