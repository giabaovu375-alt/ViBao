// ============================================================
// VIBAO COMPILER (Rust) — lexer/tests.rs
// UNIT TESTS — chạy bằng `cargo test`
// ============================================================

use super::tokenize;
use super::token::TokenKind;

#[test]
fn test_basic_app() {
    let src = r#"ung_dung("Test") { trang("/") { } }"#;
    let toks = tokenize(src).unwrap();
    assert_eq!(toks[0].kind, TokenKind::UngDung);
    assert_eq!(toks[1].kind, TokenKind::LParen);
    assert!(matches!(toks[2].kind, TokenKind::StringLit(ref s) if s == "Test"));
}

#[test]
fn test_trang_as_keyword_vs_color() {
    // "trang" đứng đầu statement → keyword TRANG
    let toks1 = tokenize(r#"trang("/")"#).unwrap();
    assert_eq!(toks1[0].kind, TokenKind::Trang);

    // "trang" sau dấu ':' → màu trắng (COLOR_NAME)
    let toks2 = tokenize(r#"color:trang"#).unwrap();
    assert!(matches!(toks2[2].kind, TokenKind::ColorName(ref s) if s == "trang"));
}

#[test]
fn test_minus_as_operator_vs_negative() {
    // "$n - 1" (cách biệt) → biến, toán tử trừ, số dương
    let toks1 = tokenize(r#"$n - 1"#).unwrap();
    assert!(matches!(toks1[0].kind, TokenKind::Variable(ref s) if s == "n"));
    assert_eq!(toks1[1].kind, TokenKind::Minus);
    assert!(matches!(toks1[2].kind, TokenKind::NumberLit(v, _) if v == 1.0));

    // "-5" (đầu biểu thức, dính liền) → số âm
    let toks2 = tokenize(r#"-5"#).unwrap();
    assert!(matches!(toks2[0].kind, TokenKind::NumberLit(v, _) if v == -5.0));
}

#[test]
fn test_number_with_unit() {
    let toks = tokenize(r#"50%"#).unwrap();
    assert!(matches!(toks[0].kind, TokenKind::NumberLit(v, ref raw) if v == 50.0 && raw == "50%"));
}

#[test]
fn test_string_with_vietnamese() {
    let toks = tokenize(r#""Xin chào ViBao! 🐧""#).unwrap();
    assert!(matches!(toks[0].kind, TokenKind::StringLit(ref s) if s == "Xin chào ViBao! 🐧"));
}

#[test]
fn test_unclosed_string_errors() {
    let result = tokenize(r#""chưa đóng"#);
    assert!(result.is_err());
}

#[test]
fn test_bang_operator_standalone() {
    // '!' đứng riêng phải ra Bang, không lẫn với '!=' (Neq)
    let toks = tokenize(r#"!$da_dang_nhap"#).unwrap();
    assert_eq!(toks[0].kind, TokenKind::Bang);
    assert!(matches!(toks[1].kind, TokenKind::Variable(ref s) if s == "da_dang_nhap"));

    let toks2 = tokenize(r#"$a != $b"#).unwrap();
    assert!(matches!(toks2[0].kind, TokenKind::Variable(_)));
    assert_eq!(toks2[1].kind, TokenKind::Neq);
}

#[test]
fn test_percent_operator_vs_unit() {
    // "50%" dính liền digit → hậu tố đơn vị CSS, vẫn là 1 NumberLit
    let toks1 = tokenize(r#"50%"#).unwrap();
    assert!(matches!(toks1[0].kind, TokenKind::NumberLit(v, ref raw) if v == 50.0 && raw == "50%"));

    // "$n % 2" (cách biệt) → toán tử chia dư đứng riêng
    let toks2 = tokenize(r#"$n % 2"#).unwrap();
    assert!(matches!(toks2[0].kind, TokenKind::Variable(ref s) if s == "n"));
    assert_eq!(toks2[1].kind, TokenKind::Percent);
    assert!(matches!(toks2[2].kind, TokenKind::NumberLit(v, _) if v == 2.0));

    // "$n%2" (dính liền, không phải sau digit) → vẫn phải là modulo,
    // không được lẫn vào số vì '%' đứng ngay sau Variable, không sau digit
    let toks3 = tokenize(r#"$n%2"#).unwrap();
    assert!(matches!(toks3[0].kind, TokenKind::Variable(ref s) if s == "n"));
    assert_eq!(toks3[1].kind, TokenKind::Percent);
    assert!(matches!(toks3[2].kind, TokenKind::NumberLit(v, _) if v == 2.0));
}

#[test]
fn test_all_event_keywords() {
    // Đảm bảo cả 7 sự kiện trong EventName (ast.rs) đều có keyword
    // tương ứng ở lexer — trước đây thiếu on_blur/on_focus/on_scroll.
    assert_eq!(tokenize("on_click").unwrap()[0].kind, TokenKind::OnClick);
    assert_eq!(tokenize("on_hover").unwrap()[0].kind, TokenKind::OnHover);
    assert_eq!(tokenize("on_blur").unwrap()[0].kind, TokenKind::OnBlur);
    assert_eq!(tokenize("on_focus").unwrap()[0].kind, TokenKind::OnFocus);
    assert_eq!(tokenize("on_change").unwrap()[0].kind, TokenKind::OnChange);
    assert_eq!(tokenize("on_submit").unwrap()[0].kind, TokenKind::OnSubmit);
    assert_eq!(tokenize("on_scroll").unwrap()[0].kind, TokenKind::OnScroll);
}
