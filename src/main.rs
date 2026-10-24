//! 终端样例外壳：装配一局温泉并打印
//!
//! 真实剧本仓的入口按宿主需要自定（终端 / Web / Android 各自装配自己的 bin / lib）；本 bin
//! 只是主线自测外壳，复用 `umaai-cli` 的呈现件（样例面，pub 面不承诺稳定）。
//!
//! 用法：`cargo run --release -- [训练员]`
//! （缺省 `onsen`；可切 `random` 或 `manual`——后者为终端交互的手动训练员，需 feature
//! `manual`，默认开启。数据根是代码级配置 [`DATA_ROOT`]，不暴露在命令行）

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

/// 统一数据根（代码级配置）：引擎静态数据拷贝 + 剧本资产 + 双配置默认层
///
/// 拷仓即跑；衍生开发改数据布局时直接改本常量（或整目录替换 `data/`）。
/// 引擎覆盖层约定保留：工作目录放同名 `ai_config.toml` / `onsen_config.toml` 存在才合并。
const DATA_ROOT: &str = "data";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("错误: {err:#}");
            ExitCode::FAILURE
        }
    }
}

/// 装配一局并跑到底（数据根为代码级配置；命令行只选训练员）
fn run() -> Result<()> {
    // 训练员：onsen（手写策略，默认）/ random（引擎随机基线）/ manual（终端交互）
    let trainer_name = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "onsen".to_string());

    // 配置两份同名不同位：默认层在数据根、覆盖层在当前目录（与引擎约定一致）
    let cfg = Config::load_with_overlay_file(DATA_ROOT, ".").context("载入配置失败")?;
    Data::init(DATA_ROOT)?;
    let onsen_data = Arc::new(OnsenData::load(DATA_ROOT).context("载入剧本数据失败")?);
    let onsen_cfg =
        OnsenConfig::load_with_overlay_file(DATA_ROOT, ".").context("载入剧本配置失败")?;

    let consts = &Data::get()?.consts;
    let mut out = std::io::stdout();
    // 上游输出全静（Quiet 只出终局块）：逐回合状态与决策信息由剧本侧自持显示（手动面板 / 事件回显）
    let opts = PrintOptions {
        verbosity: Verbosity::Quiet,
        width: term::resolve_width(None),
        color: false,
    };
    let mut fe = PrintFrontend::new(&mut out, opts, consts);
    let mut game = build_game(&cfg, onsen_data)?;
    // 规则主种子由 Runner 注入（`cfg.sim.seed`）：引擎两轴与剧本策略轴都按它派生
    let mut runner = Runner::new(&cfg, &mut fe)?;
    let summary = match trainer_name.as_str() {
        "onsen" => {
            let trainer = OnsenTrainer::new(onsen_cfg);
            runner.run_game(game, &trainer, None)?
        }
        "random" => runner.run_game(game, &RandomTrainer, None)?,
        #[cfg(feature = "manual")]
        "manual" => {
            // 手动验证：事件结算即时回显（青色）
            game.event_verbose = true;
            runner.run_game(game, &ManualTrainer, None)?
        }
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
