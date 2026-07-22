// ============================================================
// VIBAO COMPILER (Rust) — parser/mod.rs
// Struct Parser + helper cốt lõi (advance, expect, check, error).
// Các file khác trong module này (app.rs, expr.rs, element.rs,
// control.rs, action.rs) đều `impl Parser { ... }` thêm method
// mới cho CÙNG struct Parser định nghĩa ở đây — Rust cho phép
// nhiều khối impl nằm ở nhiều file miễn cùng crate, nên không
// cần định nghĩa lại struct, không cần kế thừa kiểu OOP.
// ============================================================

mod action;
mod app;
mod control;
mod element;
mod expr;

use vibao_ast::Pos;
use crate::lexer::{Token, TokenKind};
use std::fmt;

// ════════════════════════════════════════════════════════════
// 1. PARSE ERROR
// ════════════════════════════════════════════════════════════

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "[ViBao Parser] {} (dòng {}:{})",
            self.message, self.line, self.column
        )
    }
}

impl std::error::Error for ParseError {}

// ════════════════════════════════════════════════════════════
// 2. PARSER STRUCT
// ════════════════════════════════════════════════════════════

pub struct Parser {
    /// Toàn bộ token stream, đã lọc sẵn (không còn whitespace/comment vì
    /// lexer không emit các loại token đó — khác bản TS cũ vốn phải lọc
    /// riêng ở bước parser vì lexer TS emit cả whitespace/comment token).
    pub(crate) tokens: Vec<Token>,
    pub(crate) pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    // ── Entry point công khai ──────────────────────────────────────
    /// Parse toàn bộ token stream thành 1 Program hoàn chỉnh.
    /// Định nghĩa thật nằm ở app.rs (parse_app) — hàm này chỉ là điểm
    /// vào công khai duy nhất mà bên ngoài module parser cần biết tới.
    pub fn parse(mut self) -> Result<vibao_ast::Program, ParseError> {
        let app = self.parse_app()?;
        self.expect(&TokenKind::Eof)?;
        Ok(vibao_ast::Program { app })
    }
}

// ════════════════════════════════════════════════════════════
// 3. CORE HELPERS — dùng chung bởi mọi file con trong module parser
// ════════════════════════════════════════════════════════════

impl Parser {
    /// Token hiện tại, không tiêu thụ. Nếu đã hết token, trả về Eof "ảo"
    /// tại vị trí cuối cùng — tránh phải xử lý Option<&Token> ở khắp mọi
    /// nơi gọi (đơn giản hoá logic, đổi lại lúc nào cũng có 1 token hợp
    /// lệ để so sánh, kể cả khi đã vượt quá cuối mảng).
    pub(crate) fn current(&self) -> &Token {
        self.tokens
            .get(self.pos)
            .unwrap_or_else(|| self.tokens.last().expect("token stream rỗng"))
    }

    /// Nhìn trước offset token (không tiêu thụ), dùng cho lookahead khi
    /// cần phân biệt 2 cấu trúc cú pháp gần giống nhau (vd IDENTIFIER
    /// theo sau bởi COLON tức là key:value, ngược lại có thể là literal
    /// hoặc function call).
    pub(crate) fn peek(&self, offset: usize) -> &Token {
        self.tokens
            .get(self.pos + offset)
            .unwrap_or_else(|| self.tokens.last().expect("token stream rỗng"))
    }

    /// Tiêu thụ token hiện tại, trả về nó, rồi tiến con trỏ lên 1.
    /// Không tiến quá Eof — gọi advance() liên tục khi đã ở Eof sẽ luôn
    /// trả về chính token Eof đó, không panic, không đọc ra ngoài mảng.
    pub(crate) fn advance(&mut self) -> Token {
        let tok = self.current().clone();
        if !matches!(tok.kind, TokenKind::Eof) {
            self.pos += 1;
        }
        tok
    }

    /// So sánh kind của token hiện tại — dùng discriminant thay vì so
    /// sánh trực tiếp bằng == để không phải quan tâm dữ liệu bên trong
    /// biến thể (vd chỉ cần biết "đây có phải StringLit không", không
    /// cần biết chuỗi bên trong là gì cụ thể ở bước kiểm tra này).
    pub(crate) fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.current().kind) == std::mem::discriminant(kind)
    }

    /// Giống check() nhưng kiểm tra tại vị trí offset thay vì hiện tại.
    pub(crate) fn check_at(&self, offset: usize, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.peek(offset).kind) == std::mem::discriminant(kind)
    }

    /// Bắt buộc token hiện tại phải đúng kind, tiêu thụ nó nếu đúng,
    /// báo lỗi rõ ràng nếu sai. Đây là "expect" quen thuộc của mọi
    /// recursive-descent parser — tương đương this.expect() ở bản TS.
    pub(crate) fn expect(&mut self, kind: &TokenKind) -> Result<Token, ParseError> {
        if self.check(kind) {
            Ok(self.advance())
        } else {
            let cur = self.current();
            Err(ParseError {
                message: format!("Mong đợi {}, nhận được {}", kind, cur.kind),
                line: cur.line,
                column: cur.column,
            })
        }
    }

    /// Kiểm tra token hiện tại có phải Identifier với đúng giá trị chuỗi
    /// name không — dùng cho các "soft keyword" như "state", "layout",
    /// "guard" vốn không có TokenKind riêng (khác với keyword cứng như
    /// Trang/Neu đã có variant enum riêng trong lexer).
    pub(crate) fn check_ident(&self, name: &str) -> bool {
        matches!(&self.current().kind, TokenKind::Identifier(s) if s == name)
    }

    /// Vị trí (dòng, cột) của token hiện tại — dùng để gắn Pos cho node
    /// mới tạo, khớp với "currentPos()" ở bản TS cũ.
    pub(crate) fn current_pos(&self) -> Pos {
        let t = self.current();
        Pos {
            line: t.line,
            column: t.column,
        }
    }

    /// Tạo lỗi parse tại vị trí token hiện tại — tiện ích ngắn gọn thay
    /// vì phải tự tạo ParseError { ... } thủ công ở mọi nơi cần báo lỗi.
    pub(crate) fn error(&self, message: impl Into<String>) -> ParseError {
        let cur = self.current();
        ParseError {
            message: message.into(),
            line: cur.line,
            column: cur.column,
        }
    }

    /// Nếu token hiện tại là Comma thì tiêu thụ nó — dùng ở cuối mỗi
    /// vòng lặp parse danh sách (args, props, children...) để chấp nhận
    /// dấu phẩy phân cách tuỳ chọn (không bắt buộc dấu phẩy cuối).
    pub(crate) fn skip_comma(&mut self) {
        if self.check(&TokenKind::Comma) {
            self.advance();
        }
    }

    // ────────────────────────────────────────────────────────────
    // CÁC HÀM HELPER BỔ SUNG ĐỂ TƯƠNG THÍCH VỚI ACTION/APP/ELEMENT
    // ────────────────────────────────────────────────────────────

    /// Kiểm tra xem đã hết token chưa
    pub(crate) fn is_at_end(&self) -> bool {
        matches!(self.current().kind, TokenKind::Eof)
    }

    /// Kiểm tra xem token hiện tại có khớp không, nếu khớp thì tiêu thụ luôn
    pub(crate) fn match_token(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Giống expect nhưng nhận message lỗi tùy biến
    pub(crate) fn consume(&mut self, kind: &TokenKind, message: &str) -> Result<Token, ParseError> {
        if self.check(kind) {
            Ok(self.advance())
        } else {
            let cur = self.current();
            Err(ParseError {
                message: format!("{} (Nhận được: {})", message, cur.kind),
                line: cur.line,
                column: cur.column,
            })
        }
    }
}

// ════════════════════════════════════════════════════════════
// 4. ENTRY POINT CẤP MODULE — dùng bởi main.rs
// ════════════════════════════════════════════════════════════

/// Parse 1 chuỗi token thành Program. Tương đương hàm `parse()` export ở
/// 05-parser-core.ts bản cũ — điểm vào duy nhất mà code bên ngoài
/// (main.rs, và sau này codegen.rs) cần gọi tới.
pub fn parse(tokens: Vec<Token>) -> Result<vibao_ast::Program, ParseError> {
    Parser::new(tokens).parse()
}

// ════════════════════════════════════════════════════════════
// 5. UNIT TESTS
// ════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;

    #[test]
    fn test_advance_stops_at_eof() {
        let tokens = tokenize("").unwrap();
        let mut p = Parser::new(tokens);
        for _ in 0..5 {
            let t = p.advance();
            assert_eq!(t.kind, TokenKind::Eof);
        }
    }

    #[test]
    fn test_check_and_expect() {
        let tokens = tokenize(r#""hello""#).unwrap();
        let mut p = Parser::new(tokens);
        assert!(p.check(&TokenKind::StringLit(String::new())));
        let tok = p.expect(&TokenKind::StringLit(String::new())).unwrap();
        assert!(matches!(tok.kind, TokenKind::StringLit(ref s) if s == "hello"));
    }

    #[test]
    fn test_expect_wrong_kind_errors() {
        let tokens = tokenize(r#""hello""#).unwrap();
        let mut p = Parser::new(tokens);
        let result = p.expect(&TokenKind::LParen);
        assert!(result.is_err());
    }

    #[test]
    fn test_expect_error_message_is_human_readable_not_debug_syntax() {
        // Cải thiện trải nghiệm: message lỗi phải đọc tự nhiên (vd
        // "chuỗi \"hello\"") thay vì cú pháp Debug thô của Rust (vd
        // 'StringLit("hello")') — trước đây dùng {:?} khiến lỗi cú
        // pháp trông như crash nội bộ, khó hiểu với người không biết Rust.
        let tokens = tokenize(r#""hello""#).unwrap();
        let mut p = Parser::new(tokens);
        let err = p.expect(&TokenKind::LParen).unwrap_err();
        let msg = err.to_string();
        assert!(!msg.contains("StringLit("), "message vẫn lộ cú pháp Debug: {}", msg);
        assert!(msg.contains("chuỗi \"hello\""), "message phải mô tả token tự nhiên: {}", msg);
    }

    #[test]
    fn test_check_ident() {
        let tokens = tokenize("state").unwrap();
        let p = Parser::new(tokens);
        let tokens2 = tokenize("layout_custom_name").unwrap();
        let p2 = Parser::new(tokens2);
        assert!(p2.check_ident("layout_custom_name"));
        let _ = p; 
    }
}
