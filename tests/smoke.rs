//! 剧本仓冒烟与可复现性守门
//!
//! 项目约定：集成测试不用 `assert`，用打印 + `Result` 返回值；失败由 `Err` / 非零退出码判定。

use anyhow::{Context, Result, bail};
use rand::SeedableRng;
use rand::rngs::StdRng;
use std::sync::Arc;
use umaai::Config;
use umaai::runner::{Runner, Summary};
use umaai_cli::frontend::{PrintFrontend, PrintOptions, Verbosity};
use umaai_cli::render::term;
use umaai_core::Data;

use umaai_onsen::{OnsenData, build_game};
use umasim::game::Game;
use umasim::trainer::RandomTrainer;

/// 引擎数据根（本仓测试固定用并列引擎仓的数据；CI 按同样布局检出）
fn data_root() -> String {
    std::env::var("UMAAI_DATA_ROOT").unwrap_or_else(|_| "../umaai_next/gamedata".to_string())
}

/// 剧本数据根（本仓 `data/`；CI 与本地测试的工作目录都是仓根）
fn onsen_root() -> String {
    std::env::var("UMAAI_ONSEN_DATA_ROOT").unwrap_or_else(|_| "data".to_string())
}

/// 载入配置 + 剧本数据并初始化引擎数据（幂等）
fn setup() -> Result<(Config, Arc<OnsenData>)> {
    let root = data_root();
    let cfg = Config::load_with_overlay_file(&root, ".").context("载入配置失败")?;
    Data::init(&root)?;
    let data = Arc::new(OnsenData::load(onsen_root()).context("载入剧本数据失败")?);
    Ok((cfg, data))
}

/// 跑一局，返回终局汇总
fn run_once(cfg: &Config, data: &Arc<OnsenData>) -> Result<Summary> {
    let consts = &Data::get()?.consts;
    let mut out = std::io::sink();
    let opts = PrintOptions {
        verbosity: Verbosity::Quiet,
        width: term::resolve_width(None),
        color: false,
    };
    let mut fe = PrintFrontend::new(&mut out, opts, consts);
    let mut runner = Runner::new(cfg, &mut fe)?;
    let game = build_game(cfg, Arc::clone(data))?;
    runner.run_game(game, &RandomTrainer, None)
}

/// 固定种子跑满 78 回合（阶段数 = 78 × 5）
#[test]
fn smoke_runs_full_turn_count() -> Result<()> {
    let (cfg, data) = setup()?;
    let summary = run_once(&cfg, &data)?;
    println!(
        "回合 {}/{}，阶段 {}，评分 {}，PT {}，五维 {:?}",
        summary.turn,
        umaai_onsen::MAX_TURN,
        summary.stages,
        summary.score,
        summary.pt,
        summary.five_status
    );
    if summary.turn != umaai_onsen::MAX_TURN {
        bail!("未跑满回合：{}", summary.turn);
    }
    if summary.stages != (umaai_onsen::MAX_TURN as usize + 1) * 5 {
        bail!("阶段数不符：{}（应为 78 × 5 = 390）", summary.stages);
    }
    Ok(())
}

/// 同种子两次运行逐位一致（可复现性守门）
#[test]
fn smoke_is_deterministic() -> Result<()> {
    let (cfg, data) = setup()?;
    let a = run_once(&cfg, &data)?;
    let b = run_once(&cfg, &data)?;
    if a != b {
        bail!("同种子两次运行不一致：{a:?} vs {b:?}");
    }
    println!(
        "两次运行逐位一致：评分 {}，五维 {:?}",
        a.score, a.five_status
    );
    Ok(())
}

/// 规则主种子注入后可直接推进（不经 Runner 的直连路径）
#[test]
fn smoke_direct_drive_without_runner() -> Result<()> {
    let (cfg, data) = setup()?;
    let mut game = build_game(&cfg, data)?;
    game.set_rule_master(cfg.sim.seed);
    let mut decision = StdRng::seed_from_u64(cfg.sim.seed);
    game.run_full_game(&RandomTrainer, &mut decision)?;
    println!(
        "直连跑满：回合 {}，评分 {}",
        game.turn(),
        game.uma().five_status.iter().sum::<i32>()
    );
    Ok(())
}
