// ============================================================
// VIBAO COMPILER (Rust) — lexer/token.rs
// TokenKind + Token: định nghĩa các loại token và struct token cụ thể
// ============================================================

use std::fmt;

// ════════════════════════════════════════════════════════════
// 1. TOKEN TYPE
// ════════════════════════════════════════════════════════════

/// Mọi loại token trong ViBao. Dùng enum thay vì string type như bản TS
/// cũ — Rust match sẽ tự bắt lỗi biên dịch nếu quên xử lý 1 nhánh nào đó
/// (exhaustiveness check có sẵn của ngôn ngữ, không cần tự viết như TS).
#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Keywords - khai báo
    Trang,
    UngDung,
    Layout,
    Theme,
    State,

    // Keywords - điều khiển
    Neu,
    KhongThi,
    NeuNhieu,
    TruongHop,
    MacDinh,
    VongLap,

    // Keywords - sự kiện
    OnClick,
    OnHover,
    OnBlur,
    OnFocus,
    OnChange,
    OnSubmit,
    OnScroll,
    OnTai,
    OnHuy,

    // Component (text, box, flex, button, ...)
    Component(String),

    // Literals
    StringLit(String),
    NumberLit(f64, String), // (giá trị, chuỗi gốc kèm đơn vị nếu có — vd "50%")
    BoolLit(bool),
    ColorHex(String),
    ColorName(String),

    // Identifiers
    Identifier(String),
    Variable(String), // $ten_bien (không gồm dấu $)

    // Toán tử
    Plus,
    Minus,
    Star,
    Slash,
    Gt,
    Lt,
    Gte,
    Lte,
    EqEq,
    Neq,
    AndAnd,
    OrOr,
    Bang,    // ! (phủ định logic, đứng riêng — khác Neq "!=")
    Percent, // % dùng làm toán tử chia dư (modulo), khác với hậu tố đơn vị CSS "50%"
    Equals,
    Colon,
    Comma,
    Dot,
    Arrow, // → hoặc ->

    // Ngoặc
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,

    // Đặc biệt
    At, // @ (đứng riêng, không phải @the — dùng cho @hieu_ung, @di_dong...)
    Eof,
}

impl fmt::Display for TokenKind {
    /// Hiển thị token theo dạng NGƯỜI ĐỌC ĐƯỢC, dùng trong thông báo lỗi
    /// parse — khác hẳn `{:?}` (Debug) vốn in ra cú pháp enum Rust thô
    /// (vd `Identifier("mua_nen")`) trông như lỗi debug nội bộ, không
    /// thân thiện với người viết code ViBao. Mọi chỗ báo lỗi trong
    /// parser/*.rs nên dùng `{}` (Display, hàm này) thay vì `{:?}`.
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            TokenKind::Trang => write!(f, "từ khóa 'trang'"),
            TokenKind::UngDung => write!(f, "từ khóa 'ung_dung'"),
            TokenKind::Layout => write!(f, "từ khóa 'layout'"),
            TokenKind::Theme => write!(f, "từ khóa 'theme'"),
            TokenKind::State => write!(f, "từ khóa 'state'"),

            TokenKind::Neu => write!(f, "từ khóa 'neu'"),
            TokenKind::KhongThi => write!(f, "từ khóa 'khong_thi'"),
            TokenKind::NeuNhieu => write!(f, "từ khóa 'neu_nhieu'"),
            TokenKind::TruongHop => write!(f, "từ khóa 'truong_hop'"),
            TokenKind::MacDinh => write!(f, "từ khóa 'mac_dinh'"),
            TokenKind::VongLap => write!(f, "từ khóa 'vong_lap'"),

            TokenKind::OnClick => write!(f, "từ khóa 'on_click'"),
            TokenKind::OnHover => write!(f, "từ khóa 'on_hover'"),
            TokenKind::OnBlur => write!(f, "từ khóa 'on_blur'"),
            TokenKind::OnFocus => write!(f, "từ khóa 'on_focus'"),
            TokenKind::OnChange => write!(f, "từ khóa 'on_change'"),
            TokenKind::OnSubmit => write!(f, "từ khóa 'on_submit'"),
            TokenKind::OnScroll => write!(f, "từ khóa 'on_scroll'"),
            TokenKind::OnTai => write!(f, "từ khóa 'on_tai'"),
            TokenKind::OnHuy => write!(f, "từ khóa 'on_huy'"),

            TokenKind::Component(name) => write!(f, "thành phần '{}'", name),

            TokenKind::StringLit(s) => write!(f, "chuỗi \"{}\"", s),
            TokenKind::NumberLit(_, raw) => write!(f, "số {}", raw),
            TokenKind::BoolLit(b) => write!(f, "giá trị luận lý {}", b),
            TokenKind::ColorHex(h) => write!(f, "mã màu {}", h),
            TokenKind::ColorName(n) => write!(f, "tên màu '{}'", n),

            TokenKind::Identifier(s) => write!(f, "định danh '{}'", s),
            TokenKind::Variable(s) => write!(f, "biến '${}'", s),

            TokenKind::Plus => write!(f, "'+'"),
            TokenKind::Minus => write!(f, "'-'"),
            TokenKind::Star => write!(f, "'*'"),
            TokenKind::Slash => write!(f, "'/'"),
            TokenKind::Gt => write!(f, "'>'"),
            TokenKind::Lt => write!(f, "'<'"),
            TokenKind::Gte => write!(f, "'>='"),
            TokenKind::Lte => write!(f, "'<='"),
            TokenKind::EqEq => write!(f, "'=='"),
            TokenKind::Neq => write!(f, "'!='"),
            TokenKind::AndAnd => write!(f, "'&&'"),
            TokenKind::OrOr => write!(f, "'||'"),
            TokenKind::Bang => write!(f, "'!'"),
            TokenKind::Percent => write!(f, "'%'"),
            TokenKind::Equals => write!(f, "'='"),
            TokenKind::Colon => write!(f, "':'"),
            TokenKind::Comma => write!(f, "','"),
            TokenKind::Dot => write!(f, "'.'"),
            TokenKind::Arrow => write!(f, "'->'"),

            TokenKind::LParen => write!(f, "'('"),
            TokenKind::RParen => write!(f, "')'"),
            TokenKind::LBrace => write!(f, "'{{'"),
            TokenKind::RBrace => write!(f, "'}}'"),
            TokenKind::LBracket => write!(f, "'['"),
            TokenKind::RBracket => write!(f, "']'"),

            TokenKind::At => write!(f, "'@'"),
            TokenKind::Eof => write!(f, "kết thúc file"),
        }
    }
}

/// 1 token cụ thể trong source, kèm vị trí để báo lỗi chính xác.
#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub column: usize,
}

impl Token {
    pub(crate) fn new(kind: TokenKind, line: usize, column: usize) -> Self {
        Token { kind, line, column }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cải thiện trải nghiệm: thông báo lỗi parse phải hiển thị token
    /// theo dạng NGƯỜI ĐỌC ĐƯỢC (vd 'định danh 'abc''), không phải cú
    /// pháp Debug thô của Rust (vd 'Identifier("abc")') — nếu ai đó vô
    /// tình đổi Display impl về lại `write!(f, "{:?}", self)`, test này
    /// sẽ báo đỏ ngay.
    #[test]
    fn test_display_identifier_is_human_readable() {
        let s = format!("{}", TokenKind::Identifier("abc".to_string()));
        assert_eq!(s, "định danh 'abc'");
        assert!(!s.contains("Identifier("), "Display không được lộ cú pháp Debug enum");
    }

    #[test]
    fn test_display_string_lit_is_human_readable() {
        let s = format!("{}", TokenKind::StringLit("Xin chào".to_string()));
        assert_eq!(s, "chuỗi \"Xin chào\"");
    }

    #[test]
    fn test_display_punctuation_uses_symbol_not_variant_name() {
        assert_eq!(format!("{}", TokenKind::RParen), "')'");
        assert_eq!(format!("{}", TokenKind::Colon), "':'");
        assert_eq!(format!("{}", TokenKind::LBrace), "'{'");
    }

    #[test]
    fn test_display_works_through_reference() {
        // Nhiều chỗ trong parser match trên `&self.current().kind`, nên
        // Display cần hoạt động đúng qua tham chiếu (Rust tự deref vì
        // &T: Display khi T: Display).
        let kind = TokenKind::Identifier("x".to_string());
        let r: &TokenKind = &kind;
        assert_eq!(format!("{}", r), "định danh 'x'");
    }
}
