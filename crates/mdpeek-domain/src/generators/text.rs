//! テキスト → 構造値の決定論的抽出ヘルパ (rules generator 用)。
//! 依存を増やさないため regex は使わず手書きパースする。

use crate::seam::Quantity;

/// 文字列先頭の数値 (符号・小数対応) を切り出し、(数値, 残り) を返す。
pub fn split_leading_number(s: &str) -> Option<(f64, &str)> {
    let s = s.trim_start();
    let mut end = 0;
    let bytes = s.as_bytes();
    let mut seen_digit = false;
    let mut seen_dot = false;
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'+' | b'-' if i == 0 => {}
            b'0'..=b'9' => seen_digit = true,
            b'.' if !seen_dot => seen_dot = true,
            _ => {
                end = i;
                break;
            }
        }
        end = i + 1;
    }
    if !seen_digit {
        return None;
    }
    let num: f64 = s[..end].parse().ok()?;
    Some((num, s[end..].trim_start()))
}

/// 末尾の単位 (数値以外の記号を除いた文字列) を抽出。空なら None。
fn clean_unit(rest: &str) -> Option<String> {
    let u = rest
        .trim()
        .trim_matches(|c: char| c == '(' || c == ')' || c == '（' || c == '）');
    let u = u.trim();
    if u.is_empty() {
        None
    } else {
        Some(u.to_string())
    }
}

/// 数量文字列を `Quantity` に解釈する。対応形式:
/// - `10±0.1 mm`      → value/nominal 10, min 9.9, max 10.1
/// - `9〜11 mm` / `9-11 mm` / `9~11` → min 9, max 11, value 中点
/// - `200g` / `2 個`  → value 200/2, unit g/個
///
/// `scalable` は呼び出し側 (レシピ等) が後付けする。
pub fn parse_quantity(input: &str) -> Option<Quantity> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }

    // ± 形式 (公差)。
    for sep in ["±", "+/-", "＋－"] {
        if let Some((left, right)) = s.split_once(sep) {
            let (nominal, _) = split_leading_number(left)?;
            let (tol, rest) = split_leading_number(right)?;
            return Some(Quantity {
                value: nominal,
                unit: clean_unit(rest),
                min: Some(nominal - tol),
                max: Some(nominal + tol),
                nominal: Some(nominal),
                scalable: false,
            });
        }
    }

    // 範囲形式 (下限〜上限)。全角/半角の区切りに対応。'-' は符号と衝突するため
    // 先頭数値を消費した後の残りで判定する。
    if let Some((lo, rest)) = split_leading_number(s) {
        let rest_trim = rest.trim_start();
        for sep in ['〜', '～', '~', '-', '–'] {
            if let Some(after) = rest_trim.strip_prefix(sep)
                && let Some((hi, unit_rest)) = split_leading_number(after)
            {
                return Some(Quantity {
                    value: (lo + hi) / 2.0,
                    unit: clean_unit(unit_rest),
                    min: Some(lo),
                    max: Some(hi),
                    nominal: None,
                    scalable: false,
                });
            }
        }
        // 単一値。
        return Some(Quantity::scalar(lo, clean_unit(rest)));
    }

    None
}

/// `key: value` / `key：value` 行を分割する。全角コロンにも対応。
pub fn split_kv(line: &str) -> Option<(String, String)> {
    for sep in [':', '：'] {
        if let Some((k, v)) = line.split_once(sep) {
            return Some((k.trim().to_string(), v.trim().to_string()));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leading_number() {
        assert_eq!(split_leading_number("10 mm").unwrap().0, 10.0);
        assert_eq!(split_leading_number("-2.5x").unwrap().0, -2.5);
        assert_eq!(split_leading_number("  2.75 rad").unwrap().0, 2.75);
        assert!(split_leading_number("abc").is_none());
    }

    #[test]
    fn tolerance_pm() {
        let q = parse_quantity("10±0.1 mm").unwrap();
        assert_eq!(q.value, 10.0);
        assert_eq!(q.min, Some(9.9));
        assert_eq!(q.max, Some(10.1));
        assert_eq!(q.nominal, Some(10.0));
        assert_eq!(q.unit.as_deref(), Some("mm"));
    }

    #[test]
    fn range_forms() {
        let q = parse_quantity("9〜11 mm").unwrap();
        assert_eq!(q.min, Some(9.0));
        assert_eq!(q.max, Some(11.0));
        assert_eq!(q.value, 10.0);

        let q2 = parse_quantity("9-11").unwrap();
        assert_eq!((q2.min, q2.max), (Some(9.0), Some(11.0)));
    }

    #[test]
    fn plain_amount() {
        let q = parse_quantity("200g").unwrap();
        assert_eq!(q.value, 200.0);
        assert_eq!(q.unit.as_deref(), Some("g"));

        let q2 = parse_quantity("2 個").unwrap();
        assert_eq!(q2.value, 2.0);
        assert_eq!(q2.unit.as_deref(), Some("個"));
    }

    #[test]
    fn kv() {
        assert_eq!(
            split_kv("品番: ABC-123").unwrap(),
            ("品番".into(), "ABC-123".into())
        );
        assert_eq!(
            split_kv("数量：100").unwrap(),
            ("数量".into(), "100".into())
        );
        assert!(split_kv("no colon here").is_none());
    }
}
