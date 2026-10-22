//! A4–A5 机制单测：训练表 / 训练上下层 / 失败率下降 / 挖掘力三部分 / 跨地层 / 30 点挖掘奖励 /
//! 旅馆档 / 温泉券与发放回合 / 超回复分档与保底 / 入浴效果与临时上限 / PR / 红点加成覆写
//!
//! 项目约定：集成测试不用 `assert`，用打印 + `Result` 返回值；失败由 `Err` / 非零退出码判定。
//! 为了让公式可逐位断言，测试自建「受控局面」：剧本数据由本仓 `data/` 载入后就地改写
//! （如清空 `link_effect`），再用固定 `InheritInfo` 建局，不依赖宿主配置的卡组内容。

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use rand::Rng;
use rand::SeedableRng;
use rand::rngs::StdRng;
use umaai::Config;
use umaai_core::Data;

use umaai_onsen::{Action, MAX_TURN, OnsenData, OnsenGame};
use umasim::game::{
    ActionEnum, CardTrainingEffect, FriendCardState, Game, InheritInfo, Person, PersonType,
};

/// 引擎数据根（本仓测试固定用并列引擎仓的数据；CI 按同样布局检出）
fn data_root() -> String {
    std::env::var("UMAAI_DATA_ROOT").unwrap_or_else(|_| "../umaai_next/gamedata".to_string())
}

/// 剧本数据根（本仓 `data/`；CI 与本地测试的工作目录都是仓根）
fn onsen_root() -> String {
    std::env::var("UMAAI_ONSEN_DATA_ROOT").unwrap_or_else(|_| "data".to_string())
}

/// 载入配置并初始化引擎数据（幂等）
fn setup() -> Result<Config> {
    let root = data_root();
    let cfg = Config::load_with_overlay_file(&root, ".").context("载入配置失败")?;
    Data::init(&root)?;
    Ok(cfg)
}

/// 载入剧本数据（可就地改写后再建局）
fn load_onsen_data() -> Result<OnsenData> {
    OnsenData::load(onsen_root()).context("载入剧本数据失败")
}

/// 受控局面：清空链接加成，固定蓝因子（速 3 点 → 1 个因子）
fn controlled_game(cfg: &Config, mut data: OnsenData) -> Result<OnsenGame> {
    data.link_effect.clear();
    let inherit = InheritInfo {
        blue_count: [3, 0, 0, 0, 0],
        ..Default::default()
    };
    OnsenGame::newgame(
        cfg.sim.uma,
        &cfg.sim.cards,
        inherit,
        cfg.race_grades()?,
        Arc::new(data),
    )
}

/// 测试内独立重算挖掘力（三部分：蓝因子 / 属性档位 / 装备）
fn expected_dig_power(game: &OnsenGame, data: &OnsenData) -> [i32; 3] {
    let mut power = [0i32; 3];
    let last_rank = data.dig_stat_ranks.len() - 1;
    for stat in 0..5 {
        let mut rank = last_rank;
        while rank > 0 && game.base.uma.five_status[stat] < data.dig_stat_ranks[rank] {
            rank -= 1;
        }
        for (dig_type, power_slot) in power.iter_mut().enumerate() {
            *power_slot += game.dig_blue_count[stat] * data.dig_blue_bonus[stat][dig_type];
            for (which, slot) in data.dig_stat_bonus_types[dig_type].iter().enumerate() {
                if *slot == (stat + 1) as i32 {
                    *power_slot += data.dig_stat_bonus[which][rank];
                }
            }
        }
    }
    for (dig_type, power_slot) in power.iter_mut().enumerate() {
        *power_slot += data.dig_tool_level[game.dig_level[dig_type] as usize];
    }
    power
}

/// 训练基础值表：5 个训练 × 5 个等级，且等级越高基础值不降
#[test]
fn training_table_five_levels() -> Result<()> {
    let data = load_onsen_data()?;
    if data.training_basic_value.len() != 5 {
        bail!("训练应有 5 个，实际 {}", data.training_basic_value.len());
    }
    for train in 0..5 {
        let mut prev: Option<[i32; 7]> = None;
        for level in 0..5 {
            let row = data.training_row(train, level)?;
            if let Some(prev) = prev {
                for slot in 0..6 {
                    if row[slot] < prev[slot] {
                        bail!("训练{train} 等级{level} 的槽 {slot} 低于上一档：{row:?} < {prev:?}");
                    }
                }
            }
            prev = Some(row);
        }
    }
    println!(
        "训练表 5 × 5 档单调不降：{}",
        data.training_basic_value[0][4][0]
    );
    Ok(())
}

/// 挖掘力三部分各自生效：蓝因子 / 属性档位 / 装备等级
#[test]
fn dig_power_three_parts() -> Result<()> {
    let cfg = setup()?;
    let data = load_onsen_data()?;
    let mut game = controlled_game(&cfg, data.clone())?;

    // 蓝因子：速 3 点 → 1 个因子
    if game.dig_blue_count != [1, 0, 0, 0, 0] {
        bail!("蓝因子换算应为 [1,0,0,0,0]，实际 {:?}", game.dig_blue_count);
    }
    // 起手属性在 100 阈值之下，档位加成应为 0
    let base_power = game.dig_power;
    if base_power != expected_dig_power(&game, &data) {
        bail!(
            "挖掘力初值不符：{:?} vs {:?}",
            base_power,
            expected_dig_power(&game, &data)
        );
    }

    // ① 属性档位：把速度抬到 300 档后挖掘力应变化，且与新预期一致
    game.base.uma.five_status = [300, 0, 0, 0, 0];
    game.update_scenario_buff(false);
    let with_stat = game.dig_power;
    if with_stat == base_power {
        bail!("属性档位未影响挖掘力：{base_power:?}");
    }
    if with_stat != expected_dig_power(&game, &data) {
        bail!(
            "属性档位部分不符：{:?} vs {:?}",
            with_stat,
            expected_dig_power(&game, &data)
        );
    }
    // ② 装备等级：升到最高级后挖掘力应进一步提高，且与新预期一致
    game.dig_level = [6; 3];
    game.update_scenario_buff(false);
    let with_tool = game.dig_power;
    for dig_type in 0..3 {
        if with_tool[dig_type] <= with_stat[dig_type] {
            bail!("装备升级未提高挖掘力：{:?} <= {:?}", with_tool, with_stat);
        }
    }
    if with_tool != expected_dig_power(&game, &data) {
        bail!(
            "装备部分不符：{:?} vs {:?}",
            with_tool,
            expected_dig_power(&game, &data)
        );
    }
    println!("挖掘力三部分：初值 {base_power:?} → 属性档 {with_stat:?} → 装备满 {with_tool:?}");
    Ok(())
}

/// 跨地层：当前层取剩余量整数（不取 floor），余量按下一层挖掘力（不含链接）折算
#[test]
fn dig_value_cross_stratum() -> Result<()> {
    let cfg = setup()?;
    let data = load_onsen_data()?;
    let mut game = controlled_game(&cfg, data.clone())?;
    game.base.turn = 12;
    game.current_onsen = 1;

    // 不跨层：挖掘量受剩余量封顶
    game.dig_power = [0, 0, 0];
    game.dig_remain[1] = [100, 100, 100];
    let plain = game
        .calc_dig_value(&Action::Sleep)
        .context("休息应处于挖掘窗口")?;
    if plain != [15, 0, 0] {
        bail!("无加成时休息挖掘量应为 [15,0,0]，实际 {plain:?}");
    }

    // 跨层：当前层吃满剩余量（3，而不是 floor 后的更大值），余量按下一层折算
    game.dig_power = [100, 0, 0];
    game.dig_remain[1] = [3, 50, 50];
    let cross = game
        .calc_dig_value(&Action::Sleep)
        .context("休息应处于挖掘窗口")?;
    // base=15，当前层倍率 (100+100)/100 = 2 → 15×2=30 > 3 → 跨层
    // 余量 = 15 − 3/2 = 13.5，下一层倍率 (100+0)/100 = 1 → floor(13.5)=13
    if cross != [3, 13, 0] {
        bail!("跨地层量应为 [3,13,0]，实际 {cross:?}");
    }
    println!("跨地层：不跨 {plain:?}，跨层 {cross:?}");
    Ok(())
}

/// 链接加成只作用于当前层：当前层 +10%，下一层不带链接
#[test]
fn dig_value_link_bonus_current_layer_only() -> Result<()> {
    let cfg = setup()?;
    let mut data = load_onsen_data()?;
    // 剧本链接马娘命中砂层（地层 1）；链接键是角色 ID（= uma_id / 100）
    data.link_effect = HashMap::from([(cfg.sim.uma / 100, 1)]);
    let inherit = InheritInfo {
        blue_count: [3, 0, 0, 0, 0],
        ..Default::default()
    };
    let mut game = OnsenGame::newgame(
        cfg.sim.uma,
        &cfg.sim.cards,
        inherit,
        cfg.race_grades()?,
        Arc::new(data),
    )?;
    game.base.turn = 12;
    game.current_onsen = 1;

    // 当前层含链接：15 × (100+10)/100 = 16.5 → floor 16
    game.dig_power = [0, 0, 0];
    game.dig_remain[1] = [100, 100, 100];
    let with_link = game
        .calc_dig_value(&Action::Sleep)
        .context("休息应处于挖掘窗口")?;
    if with_link != [16, 0, 0] {
        bail!("链接应把当前层挖到 16，实际 {with_link:?}");
    }

    // 跨层时下一层不带链接：(15 − 3/2.2) × 1.0 → floor 13（若误带链接会得 15）
    game.dig_power = [100, 0, 0];
    game.dig_remain[1] = [3, 100, 100];
    let cross = game
        .calc_dig_value(&Action::Sleep)
        .context("休息应处于挖掘窗口")?;
    if cross != [3, 13, 0] {
        bail!("跨层下一层不应带链接，应为 [3,13,0]，实际 {cross:?}");
    }
    println!("链接加成：当前层 {with_link:?}，跨层 {cross:?}");
    Ok(())
}

/// 30 点挖掘奖励：每跨一个 30 点段发一次，跨两段发两次
#[test]
fn dig_bonus_every_30_points() -> Result<()> {
    let cfg = setup()?;
    let data = load_onsen_data()?;
    let mut game = controlled_game(&cfg, data.clone())?;
    let mut rng = StdRng::seed_from_u64(7);
    game.current_onsen = 1;
    game.dig_remain[1] = [1000, 1000, 1000];
    let bonus = data.dig_bonus[0];

    // 29 → 30：跨一段，发一次
    game.dig_progress[1] = [29, 0, 0];
    let before = (game.base.uma.five_status, game.base.uma.vital);
    game.do_dig(&[1, 0, 0], &mut rng);
    let after = (game.base.uma.five_status, game.base.uma.vital);
    for slot in 0..5 {
        if after.0[slot] - before.0[slot] != bonus[slot] {
            bail!(
                "29→30 奖励槽 {slot} 应为 {}，实际 {}",
                bonus[slot],
                after.0[slot] - before.0[slot]
            );
        }
    }
    if after.1 - before.1 != bonus[5] {
        bail!(
            "29→30 体力奖励应为 {}，实际 {}",
            bonus[5],
            after.1 - before.1
        );
    }

    // 28 → 61：跨两段，发两次
    game.dig_progress[1] = [28, 0, 0];
    let before = (game.base.uma.five_status, game.base.uma.vital);
    game.do_dig(&[33, 0, 0], &mut rng);
    let after = (game.base.uma.five_status, game.base.uma.vital);
    for slot in 0..5 {
        if after.0[slot] - before.0[slot] != bonus[slot] * 2 {
            bail!(
                "28→61 奖励槽 {slot} 应为 {}，实际 {}",
                bonus[slot] * 2,
                after.0[slot] - before.0[slot]
            );
        }
    }
    if after.1 - before.1 != bonus[5] * 2 {
        bail!(
            "28→61 体力奖励应为 {}，实际 {}",
            bonus[5] * 2,
            after.1 - before.1
        );
    }
    println!("30 点奖励：单段 {bonus:?}，双段 {:?}", bonus.map(|x| x * 2));
    Ok(())
}

/// 旅馆档位：按已挖源泉数取最高满足档，且得意率加成计入 `deyilv`
#[test]
fn hotel_rank_feeds_deyilv() -> Result<()> {
    let cfg = setup()?;
    let data = load_onsen_data()?;
    let mut game = controlled_game(&cfg, data.clone())?;

    // 起手仅基础温泉已挖
    if game.dig_count != 1 {
        bail!("起手已挖源泉数应为 1，实际 {}", game.dig_count);
    }
    let rank1 = data.hotel_rank(game.dig_count);
    if game.hotel != rank1 {
        bail!("旅馆档不符：{:?} vs {:?}", game.hotel, rank1);
    }
    // 得意率 = 卡片累计效果 + 旅馆档加成
    let card_deyilv = game.deck[0].calc_training_effect(&game, 0).deyilv;
    let got = game.deyilv(0);
    if (got - (card_deyilv + rank1.deyilv as f32)).abs() > 1e-6 {
        bail!(
            "deyilv 未含旅馆加成：{got} vs {card_deyilv} + {}",
            rank1.deyilv
        );
    }

    // 多挖两座 → 旅馆档提升
    game.onsen_state[1] = true;
    game.onsen_state[2] = true;
    game.update_scenario_buff(false);
    let rank3 = data.hotel_rank(game.dig_count);
    if game.hotel != rank3 {
        bail!("旅馆档未随源泉数更新：{:?} vs {:?}", game.hotel, rank3);
    }
    if rank3.deyilv < rank1.deyilv {
        bail!("旅馆档得意率应随源泉数不降：{rank1:?} → {rank3:?}");
    }
    println!(
        "旅馆档：{rank1:?} → {rank3:?}（deyilv {} → {}）",
        rank1.deyilv, rank3.deyilv
    );
    Ok(())
}

/// 训练上层加成与失败率下降：温泉效果生效期间才生效，且上层逐槽截 100
#[test]
fn training_upper_layer_and_fail_rate() -> Result<()> {
    let cfg = setup()?;
    let data = load_onsen_data()?;
    let mut game = controlled_game(&cfg, data.clone())?;
    // 起手面板等于基础温泉效果之和
    if game.onsen_buff != data.onsen_info[0].effect {
        bail!("温泉效果面板未等于基础温泉：{:?}", game.onsen_buff);
    }
    let buffs = game.calc_training_buff(0)?;

    // 未使用温泉券：与引擎默认值一致（仅逐槽截 100）
    game.bathing.buff_remain_turn = 0;
    let base_value = game.calc_training_value(&buffs, 0)?;
    let default_value = game.default_calc_training_value(&buffs, 0)?;
    for slot in 0..6 {
        if base_value.status_pt[slot] != default_value.status_pt[slot].min(100) {
            bail!(
                "未生效时应等于默认值截 100：槽 {slot} 得 {} vs {}",
                base_value.status_pt[slot],
                default_value.status_pt[slot].min(100)
            );
        }
    }

    // 温泉效果生效：上层加成存在且逐槽不超过 100
    game.bathing.buff_remain_turn = 2;
    let total_value = game.calc_training_value(&buffs, 0)?;
    let mut any_upper = false;
    for slot in 0..5 {
        let upper = total_value.status_pt[slot] - base_value.status_pt[slot];
        if !(0..=100).contains(&upper) {
            bail!("上层加成越界：槽 {slot} 为 {upper}");
        }
        any_upper |= upper > 0;
    }
    if !any_upper {
        bail!("温泉效果生效期间应有上层加成");
    }

    // 失败率下降：把温泉的 fail_rate_drop 并入卡片 buffs 后，引擎算得的失败率不升
    let rate_plain = game.calc_training_failure_rate(&buffs, 0)?;
    let drop = CardTrainingEffect {
        fail_rate_drop: game.onsen_buff.fail_rate_drop as f32,
        ..Default::default()
    };
    let merged = buffs.add(&drop);
    let rate_merged = game.calc_training_failure_rate(&merged, 0)?;
    if rate_merged > rate_plain {
        bail!("失败率下降后失败率反而升高：{rate_plain} → {rate_merged}");
    }
    println!(
        "训练上层加成：{:?} → {:?}；失败率 {rate_plain} → {rate_merged}",
        base_value.status_pt, total_value.status_pt
    );
    Ok(())
}

/// 源泉选择 / 装备升级的解锁与上限校验
#[test]
fn source_select_and_upgrade_guards() -> Result<()> {
    let cfg = setup()?;
    let data = load_onsen_data()?;
    let mut game = controlled_game(&cfg, data.clone())?;

    // 未解锁的源泉不能选
    let locked = data
        .onsen_info
        .iter()
        .position(|info| info.unlock_turn > game.turn)
        .context("剧本数据中应有未解锁源泉")?;
    if game
        .list_actions_onsen_select()
        .contains(&Action::Dig(locked))
    {
        bail!("未解锁源泉不应出现在可选列表：{locked}");
    }
    if game.do_select_dig(locked).is_ok() {
        bail!("选择未解锁源泉应当报错");
    }
    // 已挖通的源泉不能重选
    if game.do_select_dig(0).is_ok() {
        bail!("重选已挖通源泉应当报错");
    }

    // 装备升级上限 6 级
    if game.get_upgradeable_equipment().len() != 3 {
        bail!("起手三件装备都应可升级");
    }
    game.dig_level = [6, 6, 6];
    if !game.get_upgradeable_equipment().is_empty() {
        bail!("满级装备不应出现在可升级列表");
    }
    if game.do_upgrade_equipment(0).is_ok() {
        bail!("满级装备再升级应当报错");
    }
    println!("守门：未解锁 / 已挖通 / 满级装备均被拒绝");
    Ok(())
}

/// 温泉券上限 3，且 2 / 36 / 60 回合开始时发放
#[test]
fn ticket_cap_and_grant_turns() -> Result<()> {
    let cfg = setup()?;
    let data = load_onsen_data()?;
    let mut game = controlled_game(&cfg, data)?;

    // 上限 3：多给不越界
    game.add_ticket(10);
    if game.bathing.ticket_num != 3 {
        bail!("温泉券上限应为 3，实际 {}", game.bathing.ticket_num);
    }

    // 发放回合由阶段机接线：推到 36 回合开始，清空券后执行 Begin 应按下发规则恢复
    game.set_rule_master(cfg.sim.seed);
    let mut decision = StdRng::seed_from_u64(cfg.sim.seed);
    while game.turn() < 36 {
        game.run_stage(&umasim::trainer::RandomTrainer, &mut decision)?;
        game.next();
    }
    if game.turn() != 36 {
        bail!("推进后应停在 36 回合开头，实际 {}", game.turn());
    }
    game.bathing.ticket_num = 0;
    // 固定友人卡档次，让「发 2 张」可观测
    game.base.friend.card_state = FriendCardState::SSR;
    game.run_stage(&umasim::trainer::RandomTrainer, &mut decision)?;
    if game.bathing.ticket_num != 2 {
        bail!("36 回合开始应发 2 张券，实际 {}", game.bathing.ticket_num);
    }

    // 开局回合（2）固定 2 张
    game.base.turn = 2;
    if game.grant_ticket_num() != 2 {
        bail!("开局回合应固定发 2 张，实际 {}", game.grant_ticket_num());
    }
    println!("温泉券：上限 3；36 回合开始发放 2 张");
    Ok(())
}

/// 超回复分档：按累计体力消耗每 50 一档查表，累计 ≥275 保底
#[test]
fn super_prob_tiers_and_floor() -> Result<()> {
    let cfg = setup()?;
    let data = load_onsen_data()?;
    let mut game = controlled_game(&cfg, data.clone())?;

    for (idx, raw) in data.super_probs.iter().enumerate() {
        game.dig_vital_cost = idx as i32 * 50;
        let want = *raw as f64 / 100.0;
        let got = game.calc_super_prob(0);
        if (got - want).abs() > 1e-9 {
            bail!("档 {idx}（累计 {}）应为 {want}，实际 {got}", idx * 50);
        }
    }
    // 保底：已有 250 + 本次 25 = 275 → 必定触发
    game.dig_vital_cost = 250;
    if game.calc_super_prob(25) != 1.0 {
        bail!("累计 275 应保底 1.0，实际 {}", game.calc_super_prob(25));
    }
    println!("超回复分档：{:?}，275 保底", data.super_probs);
    Ok(())
}

/// 超回复就绪：触发时清空累计消耗，重复触发不重置
#[test]
fn super_ready_resets_cost() -> Result<()> {
    let cfg = setup()?;
    let data = load_onsen_data()?;
    let mut game = controlled_game(&cfg, data)?;

    game.dig_vital_cost = 120;
    game.add_super();
    if !game.bathing.is_super_ready || game.dig_vital_cost != 0 {
        bail!(
            "触发超回复应就绪并清零累计：ready={} cost={}",
            game.bathing.is_super_ready,
            game.dig_vital_cost
        );
    }
    game.dig_vital_cost = 40;
    game.add_super();
    if game.dig_vital_cost != 40 {
        bail!(
            "已就绪时不应重复获得超回复（不该清零）：{}",
            game.dig_vital_cost
        );
    }
    println!("超回复：触发即就绪并清零累计；已就绪不再重复触发");
    Ok(())
}

/// 入浴：超回复用临时上限 150、羁绊取最低 N 人（满 100 不加）、干劲与扣券
#[test]
fn use_ticket_applies_super_and_cap() -> Result<()> {
    let cfg = setup()?;
    let data = load_onsen_data()?;
    let mut game = controlled_game(&cfg, data.clone())?;
    let mut rng = StdRng::seed_from_u64(7);
    let panel = data.onsen_info[0].effect;

    // 格局 1：普通入浴（羁绊取最低 3 人）
    game.add_ticket(1);
    game.uma_mut().vital = 40;
    game.uma_mut().max_vital = 100;
    game.uma_mut().motivation = 3;
    let spread = [95, 20, 30, 40, 50, 60];
    for (i, value) in spread.iter().enumerate() {
        game.persons[i].set_friendship(*value);
    }
    Action::UseTicket(true).apply(&mut game, &mut rng)?;
    if game.bathing.ticket_num != 0 || game.bathing.buff_remain_turn != 2 {
        bail!(
            "入浴应扣 1 张券并让效果持续 2 回合：券={} 剩余={}",
            game.bathing.ticket_num,
            game.bathing.buff_remain_turn
        );
    }
    let got: Vec<i32> = (0..6).map(|i| game.persons[i].friendship()).collect();
    if got != [95, 30, 40, 50, 50, 60] {
        bail!("羁绊最低 3 人各 +10 口径不符：{got:?}");
    }
    if game.uma().vital != 40 + panel.vital {
        bail!(
            "普通入浴体力应为 {}，实际 {}",
            40 + panel.vital,
            game.uma().vital
        );
    }
    if game.uma().motivation != 3 + panel.motivation {
        bail!(
            "入浴干劲应为 {}，实际 {}",
            3 + panel.motivation,
            game.uma().motivation
        );
    }

    // 格局 2：超回复入浴（临时上限 150 + 加算超回复体力 + 技能点 + 红点）
    let super_effect = data.super_effect;
    game.add_ticket(1);
    game.bathing.buff_remain_turn = 0;
    game.bathing.is_super_ready = true;
    game.uma_mut().vital = 100;
    game.uma_mut().max_vital = 100;
    let pt_before = game.uma().skill_pt;
    let hint_events_before = game.unresolved_events.len();
    for person in game.persons.iter_mut().take(6) {
        person.set_friendship(100);
    }
    Action::UseTicket(true).apply(&mut game, &mut rng)?;
    if game.uma().vital != super_effect.temp_max_vital {
        bail!(
            "超回复入浴体力应到临时上限 {}，实际 {}",
            super_effect.temp_max_vital,
            game.uma().vital
        );
    }
    if game.uma().skill_pt != pt_before + super_effect.pt {
        bail!(
            "超回复应加技能点 {}，实际 +{}",
            super_effect.pt,
            game.uma().skill_pt - pt_before
        );
    }
    let extra_hints = game.unresolved_events.len() - hint_events_before;
    if extra_hints != (game.hotel.hint + 2) as usize {
        bail!(
            "超回复红点应为旅馆档 hint+2 = {} 次，实际 {extra_hints}",
            game.hotel.hint + 2
        );
    }
    if !game.bathing.is_super || game.bathing.is_super_ready {
        bail!(
            "超回复入浴后应标记已消费该次超回复：super={} ready={}",
            game.bathing.is_super,
            game.bathing.is_super_ready
        );
    }
    println!(
        "入浴：普通 {} → 体力 {}；超回复 → 体力 {} + PT {} + 红点 {}",
        panel.vital,
        40 + panel.vital,
        game.uma().vital,
        super_effect.pt,
        extra_hints
    );
    Ok(())
}

/// PR：入账配置数值 + 发 1 张券，并按挖掘表 PR 行（11 点）计入挖掘；消耗并入超回复判定
#[test]
fn pr_value_ticket_and_dig_row() -> Result<()> {
    let cfg = setup()?;
    let data = load_onsen_data()?;
    let mut game = controlled_game(&cfg, data.clone())?;
    let mut rng = StdRng::seed_from_u64(3);
    let pr = data.pr_base_value.clone();

    // 让超回复必定触发（累计消耗已远超保底线）
    game.bathing.is_super_ready = false;
    game.dig_vital_cost = 300;
    // 打开一个未挖通的源泉作为挖掘目标（基础温泉已挖通，余量为 0）
    game.current_onsen = 1;
    game.dig_remain[1] = [90, 90, 90];
    game.dig_progress[1] = [0, 0, 0];
    game.base.turn = 10;

    let pt_before = game.uma().skill_pt;
    let status_before = game.uma().five_status;
    let limit = game.uma().five_status_limit;
    let tickets_before = game.bathing.ticket_num;
    // 挖掘量按当前（本回合 Distribute 阶段重算的）挖掘力折算，而非 PR 入账后的属性
    let rate = (100.0 + game.dig_power[0] as f64) / 100.0;
    let want_dig = (11.0 * rate).floor() as i32;

    Action::PR.apply(&mut game, &mut rng)?;

    if game.uma().skill_pt != pt_before + pr.status_pt[5] {
        bail!(
            "PR 技能点应为 +{}，实际 +{}",
            pr.status_pt[5],
            game.uma().skill_pt - pt_before
        );
    }
    for i in 0..5 {
        let expect = (status_before[i] + pr.status_pt[i]).min(limit[i]);
        if game.uma().five_status[i] != expect {
            bail!(
                "PR 五维槽 {i} 应为 {expect}，实际 {}",
                game.uma().five_status[i]
            );
        }
    }
    if game.bathing.ticket_num != tickets_before + 1 {
        bail!(
            "PR 应发 1 张温泉券：{} → {}",
            tickets_before,
            game.bathing.ticket_num
        );
    }
    if game.dig_progress[1][0] != want_dig {
        bail!(
            "PR 挖掘表应按 11 点计入（本层挖掘力折算值 {want_dig}），实际 {}",
            game.dig_progress[1][0]
        );
    }
    if !game.bathing.is_super_ready || game.dig_vital_cost != 0 {
        bail!(
            "PR 体力消耗应并入超回复判定并清零累计：ready={} cost={}",
            game.bathing.is_super_ready,
            game.dig_vital_cost
        );
    }
    println!(
        "PR：技能点 +{}，五维 +{:?}，券 {} → {}，挖掘 {}，超回复就绪",
        pr.status_pt[5],
        &pr.status_pt[..5],
        tickets_before,
        game.bathing.ticket_num,
        want_dig
    );
    Ok(())
}

/// 红点加成覆写：`hint_bonus` 并入后逐人重算，与测试内独立复现的判定序列一致
#[test]
fn hint_bonus_override() -> Result<()> {
    let cfg = setup()?;
    let data = load_onsen_data()?;
    let mut game = controlled_game(&cfg, data)?;
    let bonus = 40;
    game.onsen_buff.hint_bonus = bonus;
    let base_hint_rate = Data::get()?.consts.base_hint_rate / 100.0;

    // 独立复现的概率：卡面加成 + 剧本 `hint_bonus`（`extra` 传 0 为含加成，传 -bonus 为不含）
    let cards: Vec<(u32, i32)> = game
        .deck()
        .iter()
        .map(|card| (card.card_id, card.card_value().hint_prob_increase + bonus))
        .collect();
    let prob_of = |cid: Option<u32>, extra: i32| -> f64 {
        let card_bonus = cid
            .and_then(|cid| cards.iter().find(|(id, _)| *id == cid))
            .map_or(0, |(_, b)| *b);
        base_hint_rate * ((100 + card_bonus + extra) as f64 / 100.0)
    };
    let sweep = |game: &OnsenGame, extra: i32, seed: u64| -> Vec<bool> {
        let mut rng = StdRng::seed_from_u64(seed);
        game.persons()
            .iter()
            .filter(|p| p.person_type() == PersonType::Card)
            .map(|p| rng.random_bool(prob_of(p.card_id(), extra)))
            .collect()
    };
    let actual = |game: &OnsenGame| -> Vec<bool> {
        game.persons()
            .iter()
            .filter(|p| p.person_type() == PersonType::Card)
            .map(|p| p.hint())
            .collect()
    };

    // 扫多个种子：每个种子下实际判定序列都必须等于「含加成」的复现序列，
    // 且至少有一个种子能区分「含 / 不含加成」（否则本用例无法鉴别覆写是否生效）
    let mut distinguished = 0;
    for seed in 1..=64 {
        let expect = sweep(&game, 0, seed);
        let plain = sweep(&game, -bonus, seed);
        let mut rng = StdRng::seed_from_u64(seed);
        game.distribute_hint(&mut rng)?;
        let got = actual(&game);
        if got != expect {
            bail!("seed {seed}: 红点判定序列与复现不符 {got:?} vs {expect:?}");
        }
        distinguished += usize::from(plain != expect);
    }
    if distinguished == 0 {
        bail!("64 个种子都无法区分含 / 不含 hint_bonus，用例失去鉴别力");
    }
    println!(
        "红点加成：hint_bonus={bonus}，64 个种子全部与复现一致（其中 {distinguished} 个能区分加成）"
    );
    Ok(())
}

/// 固定种子跑满回合（A5 入浴与超回复接入后的端到端回归）
#[test]
fn full_game_reaches_end_turn() -> Result<()> {
    let cfg = setup()?;
    let data = load_onsen_data()?;
    let mut game = controlled_game(&cfg, data)?;
    game.set_rule_master(cfg.sim.seed);
    let mut decision = StdRng::seed_from_u64(cfg.sim.seed);
    game.run_full_game(&umasim::trainer::RandomTrainer, &mut decision)?;
    if game.turn() != MAX_TURN {
        bail!("未跑满回合：{}", game.turn());
    }
    // 跑满一局至少应挖通基础温泉之外的一座
    if game.dig_count <= 1 {
        bail!("跑满一局未挖通任何源泉：已挖 {}", game.dig_count);
    }
    println!(
        "端到端：回合 {}，已挖源泉 {}，五维 {:?}",
        game.turn(),
        game.dig_count,
        game.uma().five_status
    );
    Ok(())
}
