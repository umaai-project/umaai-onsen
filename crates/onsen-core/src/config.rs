//! 剧本配置段（`onsen_config.toml`）：schema、载入与覆盖层合并
//!
//! 复用引擎 H5 钩子（[`OverlayConfig`] + [`parse_with_overlay`]）的语义：默认层字段齐全、
//! 未知键严格报错；覆盖层同构、字段全 `Option`，段缺失整段沿用。根目录由宿主注入。

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{Context, Result, anyhow, ensure};
use serde::Deserialize;
use umaai::config::{OverlayConfig, parse_with_overlay};

/// 剧本配置文件名（放在宿主注入的剧本配置根下）
pub const ONSEN_CONFIG_FILE: &str = "onsen_config.toml";

/// 源泉索引上限（温泉杯共 10 座源泉，见 glossary §3）
const ONSEN_INDEX_MAX: usize = 9;

/// 剧本专属配置段
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnsenConfig {
    /// 源泉选择优先顺序（手写策略用）
    pub onsen_order: OnsenOrder,
}

/// 源泉选择优先顺序（按年；值为源泉索引，0 基）
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnsenOrder {
    /// 第 1 年（回合 0-23）
    pub year1: Vec<usize>,
    /// 第 2 年（回合 24-47）
    pub year2: Vec<usize>,
    /// 第 3 年（回合 48-77）
    pub year3: Vec<usize>,
}

impl OnsenOrder {
    /// 取指定年份（1-3）的优先顺序
    pub fn year(&self, year: i32) -> Result<&[usize]> {
        match year {
            1 => Ok(&self.year1),
            2 => Ok(&self.year2),
            3 => Ok(&self.year3),
            _ => Err(anyhow!("年份应为 1-3，实际 {year}")),
        }
    }
}

/// 覆盖层：与默认层同构、字段全 `Option`（`None` = 沿用默认层）
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnsenConfigOverlay {
    onsen_order: Option<OnsenOrderOverlay>,
}

/// `onsen_order` 段的覆盖层
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OnsenOrderOverlay {
    year1: Option<Vec<usize>>,
    year2: Option<Vec<usize>>,
    year3: Option<Vec<usize>>,
}

impl OverlayConfig for OnsenConfig {
    type Overlay = OnsenConfigOverlay;

    fn apply_overlay(&mut self, overlay: Self::Overlay) {
        if let Some(order) = overlay.onsen_order {
            if let Some(v) = order.year1 {
                self.onsen_order.year1 = v;
            }
            if let Some(v) = order.year2 {
                self.onsen_order.year2 = v;
            }
            if let Some(v) = order.year3 {
                self.onsen_order.year3 = v;
            }
        }
    }

    fn validate(&self) -> Result<()> {
        OnsenConfig::validate(self)
    }
}

impl OnsenConfig {
    /// 解析 TOML 文本（默认层 + 可选覆盖层；测试 / 内嵌场景复用）
    pub fn parse(base_toml: &str, overlay_toml: Option<&str>) -> Result<Self> {
        parse_with_overlay(base_toml, overlay_toml)
    }

    /// 载入：默认层文件 + 显式指定的覆盖层文件（给了就必须存在）
    pub fn load(base: impl AsRef<Path>, overlay: Option<&Path>) -> Result<Self> {
        let base = base.as_ref();
        let base_text = std::fs::read_to_string(base)
            .with_context(|| format!("读取剧本配置失败: {}", base.display()))?;
        let overlay_text = match overlay {
            Some(path) => Some(
                std::fs::read_to_string(path)
                    .with_context(|| format!("读取剧本覆盖配置失败: {}", path.display()))?,
            ),
            None => None,
        };
        Self::parse(&base_text, overlay_text.as_deref())
            .with_context(|| format!("剧本配置来源: {}", base.display()))
    }

    /// 载入：默认层 + **约定覆盖层**（`<dir>/onsen_config.toml`，存在才合并）
    ///
    /// 与引擎 `Config::load_with_overlay_file` 同语义：覆盖层缺席不报错。
    pub fn load_with_overlay_file(
        base_dir: impl AsRef<Path>,
        dir: impl AsRef<Path>,
    ) -> Result<Self> {
        let overlay = dir.as_ref().join(ONSEN_CONFIG_FILE);
        let overlay = overlay.exists().then_some(overlay);
        Self::load(
            base_dir.as_ref().join(ONSEN_CONFIG_FILE),
            overlay.as_deref(),
        )
    }

    /// 载入期校验：顺序表非空、索引在源泉范围内且不重复
    fn validate(&self) -> Result<()> {
        let lists = [
            ("year1", &self.onsen_order.year1),
            ("year2", &self.onsen_order.year2),
            ("year3", &self.onsen_order.year3),
        ];
        for (name, list) in lists {
            ensure!(!list.is_empty(), "onsen_order.{name} 不能为空");
            let mut seen = BTreeSet::new();
            for &index in list {
                ensure!(
                    index <= ONSEN_INDEX_MAX,
                    "onsen_order.{name} 索引越界: {index}（应为 0-{ONSEN_INDEX_MAX}）"
                );
                ensure!(seen.insert(index), "onsen_order.{name} 重复索引: {index}");
            }
        }
        Ok(())
    }
}

impl Default for OnsenConfig {
    /// 与仓内默认层 `config/onsen_config.toml` 同值（未注入配置的宿主 / 测试兜底）
    fn default() -> Self {
        Self {
            onsen_order: OnsenOrder {
                year1: vec![1, 3, 2],
                year2: vec![7, 5, 4, 2],
                year3: vec![8, 9, 4, 2, 6],
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 默认层文本（与仓内 `data/onsen_config.toml` 同构）
    fn base_toml() -> String {
        std::fs::read_to_string(crate::testing::data_dir().join("onsen_config.toml")).unwrap()
    }

    /// 仓内默认层可解析且与 `Default` 同值
    #[test]
    fn test_parse_repo_default() -> Result<()> {
        let cfg = OnsenConfig::parse(&base_toml(), None)?;
        assert_eq!(cfg, OnsenConfig::default());
        assert_eq!(cfg.onsen_order.year1, vec![1, 3, 2]);
        Ok(())
    }

    /// 未知键严格报错
    #[test]
    fn test_unknown_key_rejected() {
        let bad = base_toml().replace("year1", "year_one");
        assert!(OnsenConfig::parse(&bad, None).is_err(), "未知键应报错");
    }

    /// 覆盖层合并：段缺失整段沿用，段内字段级覆盖
    #[test]
    fn test_overlay_merge() -> Result<()> {
        let overlay = "[onsen_order]\nyear2 = [4, 2]\n";
        let cfg = OnsenConfig::parse(&base_toml(), Some(overlay))?;
        assert_eq!(cfg.onsen_order.year2, vec![4, 2]);
        assert_eq!(
            cfg.onsen_order.year1,
            OnsenConfig::default().onsen_order.year1
        );
        Ok(())
    }

    /// 顺序表校验：越界与重复索引被拒
    #[test]
    fn test_order_validation() {
        let bad = "[onsen_order]\nyear1 = [10]\nyear2 = [1]\nyear3 = [1]\n";
        assert!(OnsenConfig::parse(bad, None).is_err(), "索引越界应报错");
        let dup = "[onsen_order]\nyear1 = [1, 1]\nyear2 = [1]\nyear3 = [1]\n";
        assert!(OnsenConfig::parse(dup, None).is_err(), "重复索引应报错");
    }

    /// `year()` 越界报错
    #[test]
    fn test_year_out_of_range() {
        let cfg = OnsenConfig::default();
        assert!(cfg.onsen_order.year(0).is_err());
        assert!(cfg.onsen_order.year(4).is_err());
        assert!(cfg.onsen_order.year(2).is_ok());
    }
}
