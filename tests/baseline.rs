//! 固定种子基线守门（B6）：手写训练员 + 仓内默认数据 / 配置跑一局，与 `baselines/` 比对
//!
//! 项目约定：集成测试不用 `assert`，用打印 + `Result` 返回值；失败由 `Err` / 非零退出码判定。
//! 设 `UMAAI_ONSEN_UPDATE_BASELINE=1` 时重写基线（有意变更后更新基线用）；默认只比对，
//! 基线位移即失败。基线与 bin / Runner 路径同种子同序，逐位可比。

use std::sync::Arc;

use anyhow::{Context, Result, bail};
use umaai::Config;
use umaai_core::Data;

use umaai_onsen::{BaselineRecord, OnsenConfig, OnsenData, OnsenTrainer, build_game};

/// 引擎数据根（本仓测试固定用并列引擎仓的数据；CI 按同样布局检出）
fn data_root() -> String {
    std::env::var("UMAAI_DATA_ROOT").unwrap_or_else(|_| "../umaai_next/gamedata".to_string())
}

/// 剧本数据根（本仓 `data/`；CI 与本地测试的工作目录都是仓根）
fn onsen_root() -> String {
    std::env::var("UMAAI_ONSEN_DATA_ROOT").unwrap_or_else(|_| "data".to_string())
}

/// 固定种子跑一局并取基线记录
fn run_baseline(cfg: &Config, data: &Arc<OnsenData>) -> Result<BaselineRecord> {
    let onsen_cfg = OnsenConfig::load_with_overlay_file("config", ".")?;
    let trainer = OnsenTrainer::new(onsen_cfg);
    let mut game = build_game(cfg, Arc::clone(data))?;
    BaselineRecord::run(cfg.sim.seed, &mut game, &trainer)
}

/// 基线比对：同种子一局与仓内基线逐字段一致
#[test]
fn baseline_fixed_seed_matches_record() -> Result<()> {
    let root = data_root();
    let cfg = Config::load_with_overlay_file(&root, ".").context("载入配置失败")?;
    Data::init(&root)?;
    let data = Arc::new(OnsenData::load(onsen_root()).context("载入剧本数据失败")?);
    let record = run_baseline(&cfg, &data)?;
    let path = "baselines/onsen_default.json";
    if std::env::var("UMAAI_ONSEN_UPDATE_BASELINE").ok().as_deref() == Some("1") {
        record.save(path)?;
        println!("基线已重写：{path}（{}）", record.summary_line());
        return Ok(());
    }
    let baseline = BaselineRecord::load(path).context("载入基线失败")?;
    let diff = baseline.diff(&record);
    if !diff.is_empty() {
        bail!(
            "固定种子基线位移（有意变更请设 UMAAI_ONSEN_UPDATE_BASELINE=1 重写）：\n{}",
            diff.join("\n")
        );
    }
    println!("基线一致：种子 {}，{}", record.seed, record.summary_line());
    Ok(())
}

/// 基线可复现：同种子两次直连驱动逐位一致
#[test]
fn baseline_reproducible() -> Result<()> {
    let root = data_root();
    let cfg = Config::load_with_overlay_file(&root, ".").context("载入配置失败")?;
    Data::init(&root)?;
    let data = Arc::new(OnsenData::load(onsen_root()).context("载入剧本数据失败")?);
    let first = run_baseline(&cfg, &data)?;
    let second = run_baseline(&cfg, &data)?;
    if first != second {
        bail!("同种子两次运行不一致：\n{}", first.diff(&second).join("\n"));
    }
    println!("基线可复现：{}", first.summary_line());
    Ok(())
}
