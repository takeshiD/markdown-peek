//! カラーモデルスウォッチ生成モジュール。
//!
//! コードスパン内の色コード文字列を受け取り、その色で塗った
//! 背景色付き2スペースの ANSI エスケープ文字列を返す。
//! 色コードでない場合は `None`。
//!
//! 対応フォーマット:
//! - HEX: `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA`(大文字小文字不問)
//! - `rgb(r,g,b)` / `rgba(r,g,b,a)`
//! - `hsl(h,s%,l%)` / `hsla(h,s%,l%,a)`

/// コードスパンの中身が色コード(#hex / rgb() / rgba() / hsl() / hsla())なら、
/// その色で塗ったスウォッチ文字列(ANSIエスケープ付き)を Some で返す。
/// 色コードでなければ None。
pub fn swatch(code: &str) -> Option<String> {
    let s = code.trim();
    let (r, g, b) = parse_color(s)?;
    // 背景色付き2スペース + リセット
    Some(format!("\x1b[48;2;{r};{g};{b}m  \x1b[0m"))
}

/// 色文字列を (r, g, b) にパース。失敗時は None。
fn parse_color(s: &str) -> Option<(u8, u8, u8)> {
    if s.starts_with('#') {
        parse_hex(s)
    } else {
        let lower = s.to_ascii_lowercase();
        if lower.starts_with("rgba(") {
            parse_rgba(&lower)
        } else if lower.starts_with("rgb(") {
            parse_rgb(&lower)
        } else if lower.starts_with("hsla(") {
            parse_hsla(&lower)
        } else if lower.starts_with("hsl(") {
            parse_hsl(&lower)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// HEX パーサー
// ---------------------------------------------------------------------------

fn parse_hex(s: &str) -> Option<(u8, u8, u8)> {
    let hex = &s[1..]; // '#' を除く
    match hex.len() {
        3 => {
            // #RGB → #RRGGBB
            let r = expand_hex_nibble(hex.as_bytes()[0])?;
            let g = expand_hex_nibble(hex.as_bytes()[1])?;
            let b = expand_hex_nibble(hex.as_bytes()[2])?;
            Some((r, g, b))
        }
        4 => {
            // #RGBA → アルファ無視
            let r = expand_hex_nibble(hex.as_bytes()[0])?;
            let g = expand_hex_nibble(hex.as_bytes()[1])?;
            let b = expand_hex_nibble(hex.as_bytes()[2])?;
            // 'A' チャンネルも valid な hex digit であることを検証
            hex_digit_value(hex.as_bytes()[3])?;
            Some((r, g, b))
        }
        6 => {
            // #RRGGBB
            let r = parse_hex_byte(&hex[0..2])?;
            let g = parse_hex_byte(&hex[2..4])?;
            let b = parse_hex_byte(&hex[4..6])?;
            Some((r, g, b))
        }
        8 => {
            // #RRGGBBAA → アルファ無視
            let r = parse_hex_byte(&hex[0..2])?;
            let g = parse_hex_byte(&hex[2..4])?;
            let b = parse_hex_byte(&hex[4..6])?;
            // AA も valid であることを検証
            parse_hex_byte(&hex[6..8])?;
            Some((r, g, b))
        }
        _ => None,
    }
}

/// 1ニブル(例: 'F') を 0xNN (例: 0xFF) に展開
fn expand_hex_nibble(byte: u8) -> Option<u8> {
    let v = hex_digit_value(byte)?;
    Some(v << 4 | v)
}

/// 2桁 hex 文字列 → u8
fn parse_hex_byte(s: &str) -> Option<u8> {
    if s.len() != 2 {
        return None;
    }
    let hi = hex_digit_value(s.as_bytes()[0])?;
    let lo = hex_digit_value(s.as_bytes()[1])?;
    Some((hi << 4) | lo)
}

/// ASCII hex digit → 0-15。不正なら None。
fn hex_digit_value(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// rgb() / rgba() パーサー
// ---------------------------------------------------------------------------

/// `rgb(r,g,b)` → (r,g,b)。s はすでに lowercase 済み。
fn parse_rgb(s: &str) -> Option<(u8, u8, u8)> {
    let inner = strip_fn_wrapper(s, "rgb(")?;
    let parts = split_comma_trim(inner);
    if parts.len() != 3 {
        return None;
    }
    let r = parse_u8_dec(parts[0])?;
    let g = parse_u8_dec(parts[1])?;
    let b = parse_u8_dec(parts[2])?;
    Some((r, g, b))
}

/// `rgba(r,g,b,a)` → (r,g,b)。アルファ(0-1 小数)は範囲チェックのみ。
fn parse_rgba(s: &str) -> Option<(u8, u8, u8)> {
    let inner = strip_fn_wrapper(s, "rgba(")?;
    let parts = split_comma_trim(inner);
    if parts.len() != 4 {
        return None;
    }
    let r = parse_u8_dec(parts[0])?;
    let g = parse_u8_dec(parts[1])?;
    let b = parse_u8_dec(parts[2])?;
    parse_alpha_01(parts[3])?; // 検証のみ
    Some((r, g, b))
}

// ---------------------------------------------------------------------------
// hsl() / hsla() パーサー
// ---------------------------------------------------------------------------

/// `hsl(h,s%,l%)` → (r,g,b)。s はすでに lowercase 済み。
fn parse_hsl(s: &str) -> Option<(u8, u8, u8)> {
    let inner = strip_fn_wrapper(s, "hsl(")?;
    let parts = split_comma_trim(inner);
    if parts.len() != 3 {
        return None;
    }
    let h = parse_hue(parts[0])?;
    let sv = parse_percent(parts[1])?;
    let l = parse_percent(parts[2])?;
    Some(hsl_to_rgb(h, sv, l))
}

/// `hsla(h,s%,l%,a)` → (r,g,b)。
fn parse_hsla(s: &str) -> Option<(u8, u8, u8)> {
    let inner = strip_fn_wrapper(s, "hsla(")?;
    let parts = split_comma_trim(inner);
    if parts.len() != 4 {
        return None;
    }
    let h = parse_hue(parts[0])?;
    let sv = parse_percent(parts[1])?;
    let l = parse_percent(parts[2])?;
    parse_alpha_01(parts[3])?;
    Some(hsl_to_rgb(h, sv, l))
}

// ---------------------------------------------------------------------------
// HSL → RGB 変換(自前実装)
// ---------------------------------------------------------------------------

/// h: 0.0-360.0, s: 0.0-1.0, l: 0.0-1.0 → (r,g,b) 各 0-255
fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    if s == 0.0 {
        // 無彩色
        let v = (l * 255.0).round() as u8;
        return (v, v, v);
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let r = hue_to_rgb(p, q, h / 360.0 + 1.0 / 3.0);
    let g = hue_to_rgb(p, q, h / 360.0);
    let b = hue_to_rgb(p, q, h / 360.0 - 1.0 / 3.0);
    (
        (r * 255.0).round() as u8,
        (g * 255.0).round() as u8,
        (b * 255.0).round() as u8,
    )
}

fn hue_to_rgb(p: f64, q: f64, t: f64) -> f64 {
    // t を [0,1] に正規化
    let t = t.rem_euclid(1.0);
    if t < 1.0 / 6.0 {
        p + (q - p) * 6.0 * t
    } else if t < 1.0 / 2.0 {
        q
    } else if t < 2.0 / 3.0 {
        p + (q - p) * (2.0 / 3.0 - t) * 6.0
    } else {
        p
    }
}

// ---------------------------------------------------------------------------
// パースユーティリティ
// ---------------------------------------------------------------------------

/// `"rgb("` などのプレフィックスと末尾 `")"` を剥がして inner 文字列を返す。
fn strip_fn_wrapper<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    let s = s.strip_prefix(prefix)?;
    let s = s.strip_suffix(')')?;
    Some(s)
}

/// カンマ区切りで分割し各要素を trim した Vec<&str> を返す。
fn split_comma_trim(s: &str) -> Vec<&str> {
    s.split(',').map(|p| p.trim()).collect()
}

/// 10進数文字列 → u8。0-255 の範囲外は None。
fn parse_u8_dec(s: &str) -> Option<u8> {
    let v: u32 = parse_u32(s)?;
    if v > 255 { None } else { Some(v as u8) }
}

/// 非負整数文字列 → u32。空・非数字は None。
fn parse_u32(s: &str) -> Option<u32> {
    if s.is_empty() {
        return None;
    }
    let mut result: u32 = 0;
    for b in s.bytes() {
        if !b.is_ascii_digit() {
            return None;
        }
        result = result.checked_mul(10)?.checked_add((b - b'0') as u32)?;
    }
    Some(result)
}

/// `"50%"` → 0.0-1.0 の f64。範囲外は None。
fn parse_percent(s: &str) -> Option<f64> {
    let s = s.strip_suffix('%')?;
    let v = parse_f64_simple(s)?;
    if !(0.0..=100.0).contains(&v) {
        return None;
    }
    Some(v / 100.0)
}

/// 色相 0-360 の数値文字列 → f64。範囲外は None。
fn parse_hue(s: &str) -> Option<f64> {
    let v = parse_f64_simple(s)?;
    if !(0.0..=360.0).contains(&v) {
        return None;
    }
    Some(v)
}

/// アルファ値 0-1 の小数文字列 → f64。範囲外は None。
fn parse_alpha_01(s: &str) -> Option<f64> {
    let v = parse_f64_simple(s)?;
    if !(0.0..=1.0).contains(&v) {
        return None;
    }
    Some(v)
}

/// 正規表現なしの単純な非負 f64 パーサー。`123`, `1.5`, `.5`, `0` などを受け付ける。
/// 先頭の '+'/'-' は不可、e/E 表記は不可。
fn parse_f64_simple(s: &str) -> Option<f64> {
    if s.is_empty() {
        return None;
    }
    let mut has_digit = false;
    let mut dot_count = 0u8;
    for b in s.bytes() {
        if b == b'.' {
            dot_count += 1;
            if dot_count > 1 {
                return None;
            }
        } else if b.is_ascii_digit() {
            has_digit = true;
        } else {
            return None;
        }
    }
    if !has_digit {
        return None;
    }
    s.parse::<f64>().ok()
}

// ---------------------------------------------------------------------------
// テスト
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn has_rgb_escape(result: &str, r: u8, g: u8, b: u8) -> bool {
        result.contains(&format!("48;2;{r};{g};{b}m"))
    }

    // --- Some を返すケース ---

    #[test]
    fn test_hex6_uppercase() {
        let sw = swatch("#FF0000").unwrap();
        assert!(has_rgb_escape(&sw, 255, 0, 0), "sw={sw:?}");
        assert!(sw.contains("  "), "2 spaces in swatch");
        assert!(sw.contains("\x1b[0m"), "reset escape");
    }

    #[test]
    fn test_hex3_lowercase() {
        let sw = swatch("#f00").unwrap();
        assert!(has_rgb_escape(&sw, 255, 0, 0), "sw={sw:?}");
    }

    #[test]
    fn test_rgb_spaces() {
        let sw = swatch("rgb(255, 0, 0)").unwrap();
        assert!(has_rgb_escape(&sw, 255, 0, 0), "sw={sw:?}");
    }

    #[test]
    fn test_hsl_pure_red() {
        let sw = swatch("hsl(0, 100%, 50%)").unwrap();
        assert!(has_rgb_escape(&sw, 255, 0, 0), "sw={sw:?}");
    }

    #[test]
    fn test_hex6_mixed_case() {
        let sw = swatch("#00ff00").unwrap();
        assert!(has_rgb_escape(&sw, 0, 255, 0), "sw={sw:?}");
    }

    #[test]
    fn test_hex8_with_alpha() {
        // アルファを含む8桁HEX
        let sw = swatch("#FF000080").unwrap();
        assert!(has_rgb_escape(&sw, 255, 0, 0), "sw={sw:?}");
    }

    #[test]
    fn test_hex4_with_alpha() {
        let sw = swatch("#f00f").unwrap();
        assert!(has_rgb_escape(&sw, 255, 0, 0), "sw={sw:?}");
    }

    #[test]
    fn test_rgba() {
        let sw = swatch("rgba(255, 0, 0, 0.5)").unwrap();
        assert!(has_rgb_escape(&sw, 255, 0, 0), "sw={sw:?}");
    }

    #[test]
    fn test_hsla() {
        let sw = swatch("hsla(0, 100%, 50%, 1.0)").unwrap();
        assert!(has_rgb_escape(&sw, 255, 0, 0), "sw={sw:?}");
    }

    #[test]
    fn test_hsl_white() {
        let sw = swatch("hsl(0, 0%, 100%)").unwrap();
        assert!(has_rgb_escape(&sw, 255, 255, 255), "sw={sw:?}");
    }

    #[test]
    fn test_hsl_black() {
        let sw = swatch("hsl(0, 0%, 0%)").unwrap();
        assert!(has_rgb_escape(&sw, 0, 0, 0), "sw={sw:?}");
    }

    #[test]
    fn test_rgb_black() {
        let sw = swatch("rgb(0,0,0)").unwrap();
        assert!(has_rgb_escape(&sw, 0, 0, 0), "sw={sw:?}");
    }

    #[test]
    fn test_leading_trailing_whitespace() {
        // trim が効くことを確認
        let sw = swatch("  #FF0000  ").unwrap();
        assert!(has_rgb_escape(&sw, 255, 0, 0), "sw={sw:?}");
    }

    // --- None を返すケース ---

    #[test]
    fn test_plain_word_is_none() {
        assert_eq!(swatch("hello"), None);
    }

    #[test]
    fn test_invalid_hex_digit() {
        assert_eq!(swatch("#GGG"), None);
    }

    #[test]
    fn test_rgb_out_of_range() {
        assert_eq!(swatch("rgb(300,0,0)"), None);
    }

    #[test]
    fn test_hex_too_short() {
        assert_eq!(swatch("#12"), None);
    }

    #[test]
    fn test_rgb_missing_component() {
        assert_eq!(swatch("rgb(255,0)"), None);
    }

    #[test]
    fn test_hsl_percent_out_of_range() {
        assert_eq!(swatch("hsl(0, 101%, 50%)"), None);
    }

    #[test]
    fn test_hsl_hue_out_of_range() {
        assert_eq!(swatch("hsl(361, 100%, 50%)"), None);
    }

    #[test]
    fn test_rgba_alpha_out_of_range() {
        assert_eq!(swatch("rgba(255,0,0,1.5)"), None);
    }

    #[test]
    fn test_empty_string() {
        assert_eq!(swatch(""), None);
    }

    #[test]
    fn test_hex5_is_none() {
        assert_eq!(swatch("#12345"), None);
    }
}
