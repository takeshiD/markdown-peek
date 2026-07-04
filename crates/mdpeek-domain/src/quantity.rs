//! 「数値の operable 化」(AGENTS.md §9.3-3)。
//!
//! 公差メーター・材料スケーリング・チャートが共通利用する決定論的ロジック。
//! 「読む」を「使える」に変える affordance の核であり、renderer (web/TUI) は
//! この計算結果を描画するだけにする (重い判断は Rust core に集約 — §1)。

use crate::seam::Quantity;

/// 公差判定の結果。`ToleranceMeter` renderer はこれでバー色・位置を決める。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToleranceStatus {
    /// 上下限の範囲内。
    InSpec,
    /// 下限未満。
    BelowMin,
    /// 上限超過。
    AboveMax,
    /// 上下限が未指定で判定不能。
    Unknown,
}

/// 公差メーターの描画用に正規化した評価結果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToleranceEval {
    pub status: ToleranceStatus,
    /// min..max を 0.0..1.0 に写像した value の位置 (バー描画用)。
    /// 範囲外なら 0 未満 / 1 超も返す (クランプは renderer 側の責務)。
    pub position: Option<f64>,
    /// 規格中心値からの偏差 (value - nominal)。nominal 未指定なら None。
    pub deviation: Option<f64>,
}

/// `Quantity` の公差判定。min/max/nominal から決定論的に位置と状態を求める。
pub fn evaluate_tolerance(q: &Quantity) -> ToleranceEval {
    let status = match (q.min, q.max) {
        (Some(min), _) if q.value < min => ToleranceStatus::BelowMin,
        (_, Some(max)) if q.value > max => ToleranceStatus::AboveMax,
        (Some(_), _) | (_, Some(_)) => ToleranceStatus::InSpec,
        (None, None) => ToleranceStatus::Unknown,
    };

    // min と max の両方が揃い、かつ幅が正のときだけ位置を出せる。
    let position = match (q.min, q.max) {
        (Some(min), Some(max)) if max > min => Some((q.value - min) / (max - min)),
        _ => None,
    };

    let deviation = q.nominal.map(|n| q.value - n);

    ToleranceEval {
        status,
        position,
        deviation,
    }
}

/// スケーリング係数を計算する。基準スケール `base` に対する目標 `target` の比。
/// 例: 2 人前レシピを 5 人前にする → 2.5。
pub fn scale_factor(base: f64, target: f64) -> f64 {
    if base == 0.0 { 1.0 } else { target / base }
}

/// `scalable == true` の `Quantity` を係数倍する。scalable でなければ複製のみ
/// (単位固定・スケール対象外の値を守る)。
pub fn scale_quantity(q: &Quantity, factor: f64) -> Quantity {
    if !q.scalable {
        return q.clone();
    }
    Quantity {
        value: q.value * factor,
        unit: q.unit.clone(),
        min: q.min.map(|v| v * factor),
        max: q.max.map(|v| v * factor),
        nominal: q.nominal.map(|v| v * factor),
        scalable: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(value: f64, min: Option<f64>, max: Option<f64>, nominal: Option<f64>) -> Quantity {
        Quantity {
            value,
            unit: Some("mm".into()),
            min,
            max,
            nominal,
            scalable: false,
        }
    }

    #[test]
    fn in_spec_center() {
        let e = evaluate_tolerance(&q(10.0, Some(9.0), Some(11.0), Some(10.0)));
        assert_eq!(e.status, ToleranceStatus::InSpec);
        assert_eq!(e.position, Some(0.5));
        assert_eq!(e.deviation, Some(0.0));
    }

    #[test]
    fn below_min_and_above_max() {
        assert_eq!(
            evaluate_tolerance(&q(8.5, Some(9.0), Some(11.0), None)).status,
            ToleranceStatus::BelowMin
        );
        assert_eq!(
            evaluate_tolerance(&q(11.5, Some(9.0), Some(11.0), None)).status,
            ToleranceStatus::AboveMax
        );
    }

    #[test]
    fn unknown_when_no_bounds() {
        let e = evaluate_tolerance(&q(10.0, None, None, None));
        assert_eq!(e.status, ToleranceStatus::Unknown);
        assert_eq!(e.position, None);
    }

    #[test]
    fn position_out_of_range_is_reported() {
        // 上限超過は position > 1.0 で返る (クランプは renderer)。
        let e = evaluate_tolerance(&q(12.0, Some(9.0), Some(11.0), None));
        assert_eq!(e.position, Some(1.5));
    }

    #[test]
    fn scaling_recomputes_only_scalable() {
        let base = Quantity {
            value: 200.0,
            unit: Some("g".into()),
            min: None,
            max: None,
            nominal: None,
            scalable: true,
        };
        let scaled = scale_quantity(&base, scale_factor(2.0, 5.0));
        assert_eq!(scaled.value, 500.0);

        let fixed = Quantity {
            value: 180.0,
            unit: Some("℃".into()),
            min: None,
            max: None,
            nominal: None,
            scalable: false,
        };
        // オーブン温度など scalable=false は人数で変わってはいけない。
        assert_eq!(scale_quantity(&fixed, 2.5).value, 180.0);
    }

    #[test]
    fn scale_factor_guards_zero_base() {
        assert_eq!(scale_factor(0.0, 5.0), 1.0);
    }
}
