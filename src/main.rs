//! 终端样例外壳：装配一局温泉并打印
//!
//! 真实剧本仓的入口按宿主需要自定（终端 / Web / Android 各自装配自己的 bin / lib）；本 bin
//! 只是主线自测外壳，复用 `umaai-cli` 的呈现件（样例面，pub 面不承诺稳定）。
//!
//! 用法：`cargo run --release -- [引擎数据根 [剧本数据根 [剧本配置根 [训练员]]]]`
//! （缺省 `../umaai_next/gamedata` / `data` / `config` / `onsen`；训练员可切 `random` 或
//! `manual`——后者为终端交互的手动训练员，需 feature `manual`，默认开启）

use std::process::ExitCode;
use std::sync::Arc;

use anyhow::{Context, Result, anyhow};
use umaai::Config;
use umaai::runner::Runner;
use umaai_cli::frontend::{PrintFrontend, PrintOptions, Verbosity};
use umaai_cli::render::term;
use umaai_core::Data;

#[cfg(feature = "manual")]
use umaai_onsen::ManualTrainer;
use umaai_onsen::{OnsenConfig, OnsenData, OnsenTrainer, build_game};
use umasim::trainer::RandomTrainer;

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("错误: {err:#}");
            ExitCode::FAILURE
        }
    }
}

/// 装配一局并跑到底（配置与数据根都由宿主注入）
fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let data_root = args
        .next()
        .unwrap_or_else(|| "../umaai_next/gamedata".to_string());
    // 剧本数据根（scenario_onsen.json 所在目录）；缺省取仓内 data/
    let onsen_root = args.next().unwrap_or_else(|| "data".to_string());
    // 剧本配置根（onsen_config.toml 默认层）；缺省取仓内 config/
    let config_root = args.next().unwrap_or_else(|| "config".to_string());
    // 训练员：onsen（手写策略，默认）/ random（引擎随机基线）
    let trainer_name = args.next().unwrap_or_else(|| "onsen".to_string());

    // 配置两份同名不同位：默认层在数据根、覆盖层在当前目录（与引擎约定一致）
    let cfg = Config::load_with_overlay_file(&data_root, ".").context("载入配置失败")?;
    Data::init(&data_root)?;
    let onsen_data = Arc::new(OnsenData::load(&onsen_root).context("载入剧本数据失败")?);
    let onsen_cfg =
        OnsenConfig::load_with_overlay_file(&config_root, ".").context("载入剧本配置失败")?;

    let consts = &Data::get()?.consts;
    let mut out = std::io::stdout();
    let opts = PrintOptions {
        verbosity: Verbosity::Normal,
        width: term::resolve_width(None),
        color: false,
    };
    let mut fe = PrintFrontend::new(&mut out, opts, consts);
    let game = build_game(&cfg, onsen_data)?;
    // 规则主种子由 Runner 注入（`cfg.sim.seed`）：引擎两轴与剧本策略轴都按它派生
    let mut runner = Runner::new(&cfg, &mut fe)?;
    let summary = match trainer_name.as_str() {
        "onsen" => {
            let trainer = OnsenTrainer::new(onsen_cfg);
            runner.run_game(game, &trainer, None)?
        }
        "random" => runner.run_game(game, &RandomTrainer, None)?,
        #[cfg(feature = "manual")]
        "manual" => runner.run_game(game, &ManualTrainer, None)?,
        other => {
            return Err(anyhow!(
                "未知训练员: {other}（可选 onsen / random / manual）"
            ));
        }
    };
    println!(
        "\n终局：评分 {}（{}），五维 {:?}，PT {}",
        summary.score, summary.rank, summary.five_status, summary.pt
    );
    Ok(())
}
