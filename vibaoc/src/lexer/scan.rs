// ============================================================
// VIBAO COMPILER (Rust) — lexer/scan.rs
// Các phương thức scan chi tiết của Lexer: string, số, định danh,
// comment, toán tử 2 ký tự, di chuyển con trỏ...
// (impl block bổ sung cho struct Lexer, khai báo ở lexer/mod.rs)
// ============================================================

use super::error::LexError;
use super::helpers::{is_ident_char, unescape};
use super::token::{Token, TokenKind};
use super::Lexer;

impl Lexer {
    // ── Kiểm tra token trước có phải 1 giá trị hoàn chỉnh không ────────
    pub(crate) fn prev_token_is_value(&self) -> bool {
        match self.tokens.last() {
            None => false,
            Some(t) => matches!(
                t.kind,
                TokenKind::NumberLit(_, _)
                    | TokenKind::Variable(_)
                    | TokenKind::Identifier(_)
                    | TokenKind::Component(_)
                    | TokenKind::ColorName(_)
                    | TokenKind::ColorHex(_)
                    | TokenKind::BoolLit(_)
                    | TokenKind::StringLit(_)
                    | TokenKind::RParen
                    | TokenKind::RBracket
            ),
        }
    }

    // ── 2-char operators ────────────────────────────────────────────
    pub(crate) fn try_two_char_op(&mut self) -> Option<Token> {
        let a = self.peek(0);
        let b = self.peek(1);
        let kind = match (a, b) {
            ('=', '=') => Some(TokenKind::EqEq),
            ('!', '=') => Some(TokenKind::Neq),
            ('>', '=') => Some(TokenKind::Gte),
            ('<', '=') => Some(TokenKind::Lte),
            ('&', '&') => Some(TokenKind::AndAnd),
            ('|', '|') => Some(TokenKind::OrOr),
            _ => None,
        };
        kind.map(|k| {
            let (line, col) = (self.line, self.column);
            self.advance();
            self.advance();
            Token::new(k, line, col)
        })
    }

    // ── Comment ──────────────────────────────────────────────────────
    pub(crate) fn skip_line_comment(&mut self) {
        while !self.is_eof() && self.peek(0) != '\n' {
            self.advance();
        }
    }

    pub(crate) fn skip_block_comment(&mut self) -> Result<(), LexError> {
        let (start_line, start_col) = (self.line, self.column);
        self.advance(); // /
        self.advance(); // *
        loop {
            if self.is_eof() {
                return Err(LexError {
                    message: "Block comment chưa đóng (thiếu */)".to_string(),
                    line: start_line,
                    column: start_col,
                });
            }
            if self.peek(0) == '*' && self.peek(1) == '/' {
                self.advance();
                self.advance();
                return Ok(());
            }
            self.advance();
        }
    }

    // ── String "..." ─────────────────────────────────────────────────
    pub(crate) fn read_string(&mut self) -> Result<Token, LexError> {
        let (line, col) = (self.line, self.column);
        self.advance(); // bỏ qua "
        let mut value = String::new();

        loop {
            if self.is_eof() {
                return Err(LexError {
                    message: "Chuỗi chưa đóng (thiếu dấu \")".to_string(),
                    line,
                    column: col,
                });
            }
            let ch = self.peek(0);
            if ch == '"' {
                self.advance();
                break;
            }
            if ch == '\\' {
                self.advance();
                let escaped = self.peek(0);
                self.advance();
                value.push(unescape(escaped));
                continue;
            }
            value.push(ch);
            self.advance();
        }

        Ok(Token::new(TokenKind::StringLit(value), line, col))
    }

    // ── Màu #hex ──────────────────────────────────────────────────────
    pub(crate) fn read_hex_color(&mut self) -> Result<Token, LexError> {
        let (line, col) = (self.line, self.column);
        self.advance(); // #
        let mut hex = String::from("#");
        while !self.is_eof() && self.peek(0).is_ascii_hexdigit() {
            hex.push(self.peek(0));
            self.advance();
        }
        if ![4, 5, 7, 9].contains(&hex.len()) {
            return Err(LexError {
                message: format!(
                    "Màu hex không hợp lệ: {} — cần dạng #RGB hoặc #RRGGBB",
                    hex
                ),
                line,
                column: col,
            });
        }
        Ok(Token::new(TokenKind::ColorHex(hex), line, col))
    }

    // ── Biến $ten_bien ────────────────────────────────────────────────
    pub(crate) fn read_variable(&mut self) -> Token {
        let (line, col) = (self.line, self.column);
        self.advance(); // $
        let mut name = String::new();
        while !self.is_eof() && is_ident_char(self.peek(0)) {
            name.push(self.peek(0));
            self.advance();
        }
        Token::new(TokenKind::Variable(name), line, col)
    }

    // ── Số (nguyên/thập phân/đơn vị CSS) ─────────────────────────────
    pub(crate) fn read_number(&mut self) -> Token {
        let (line, col) = (self.line, self.column);
        let mut raw = String::new();

        if self.peek(0) == '-' {
            raw.push('-');
            self.advance();
        }
        while !self.is_eof() && self.peek(0).is_ascii_digit() {
            raw.push(self.peek(0));
            self.advance();
        }
        if self.peek(0) == '.' && self.peek(1).is_ascii_digit() {
            raw.push('.');
            self.advance();
            while !self.is_eof() && self.peek(0).is_ascii_digit() {
                raw.push(self.peek(0));
                self.advance();
            }
        }

        // Đơn vị CSS liền sau: px, %, vw, vh, em, rem
        for unit in ["px", "vw", "vh", "em", "rem", "%"] {
            if self.match_str_at_cursor(unit) {
                raw.push_str(unit);
                for _ in 0..unit.chars().count() {
                    self.advance();
                }
                break;
            }
        }

        // Giá trị số thuần (bỏ đơn vị) để dùng ở codegen sau này.
        let numeric_part: String = raw
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == '-')
            .collect();
        let value: f64 = numeric_part.parse().unwrap_or(0.0);

        Token::new(TokenKind::NumberLit(value, raw), line, col)
    }

    pub(crate) fn match_str_at_cursor(&self, s: &str) -> bool {
        let chars: Vec<char> = s.chars().collect();
        for (i, c) in chars.iter().enumerate() {
            if self.peek(i as isize) != *c {
                return false;
            }
        }
        // Đảm bảo không khớp giữa 1 identifier dài hơn, vd "pxx" không
        // nên bị đọc thành đơn vị "px" + còn thừa "x" gây lỗi tiếp theo.
        let after = self.peek(chars.len() as isize);
        !is_ident_char(after)
    }

    // ── Identifier / keyword / component / màu ───────────────────────
    pub(crate) fn read_identifier(&mut self) -> Token {
        let (line, col) = (self.line, self.column);
        let mut name = String::new();
        while !self.is_eof() && is_ident_char(self.peek(0)) {
            name.push(self.peek(0));
            self.advance();
        }

        if name == "true" {
            return Token::new(TokenKind::BoolLit(true), line, col);
        }
        if name == "false" {
            return Token::new(TokenKind::BoolLit(false), line, col);
        }

        // "trang" (và về lý thuyết các từ khác trùng tên) vừa là keyword
        // khai báo trang web vừa là tên màu trắng — chỉ ưu tiên coi là
        // tên màu khi token liền trước là dấu ':' (đang ở vị trí giá trị
        // của 1 prop, vd "color:trang"). Đây là bug đã tìm và sửa 2 lần
        // ở bản TS/JS cũ, viết đúng ngay từ đầu ở đây.
        let is_prop_value_position = matches!(
            self.tokens.last().map(|t| &t.kind),
            Some(TokenKind::Colon)
        );

        if is_prop_value_position && self.colors.contains_key(name.as_str()) {
            return Token::new(TokenKind::ColorName(name), line, col);
        }
        if let Some(kw) = self.keywords.get(name.as_str()) {
            return Token::new(kw.clone(), line, col);
        }
        // .iter().any(|c| *c == name) thay vì .contains(&name.as_str())
        // — cách viết rõ ràng hơn, tránh phải suy luận về coercion giữa
        // &&str / &str / String qua nhiều tầng tham chiếu lồng nhau (dễ
        // gây lỗi kiểu khó đọc, đặc biệt khi không có compiler tại chỗ
        // để verify thực nghiệm trong quá trình viết).
        if self.components.iter().any(|c| *c == name.as_str()) {
            return Token::new(TokenKind::Component(name), line, col);
        }
        if self.colors.contains_key(name.as_str()) {
            return Token::new(TokenKind::ColorName(name), line, col);
        }

        Token::new(TokenKind::Identifier(name), line, col)
    }

    // ── Di chuyển con trỏ ─────────────────────────────────────────────
    pub(crate) fn advance(&mut self) -> char {
        let ch = self.chars[self.pos];
        self.pos += 1;
        if ch == '\n' {
            self.line += 1;
            self.column = 1;
        } else {
            self.column += 1;
        }
        ch
    }

    pub(crate) fn peek(&self, offset: isize) -> char {
        let idx = self.pos as isize + offset;
        if idx < 0 || idx as usize >= self.chars.len() {
            '\0'
        } else {
            self.chars[idx as usize]
        }
    }

    pub(crate) fn is_eof(&self) -> bool {
        self.pos >= self.chars.len()
    }

    pub(crate) fn skip_whitespace(&mut self) {
        while !self.is_eof() {
            let ch = self.peek(0);
            if ch == ' ' || ch == '\t' || ch == '\r' || ch == '\n' {
                self.advance();
            } else {
                break;
            }
        }
    }
}
