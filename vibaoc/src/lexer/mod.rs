// ============================================================
// VIBAO COMPILER (Rust) — lexer/mod.rs
// Token types + Lexer: đọc source code ViBao → Vec<Token>
//
// File này được chia nhỏ từ 1 file lexer.rs duy nhất thành nhiều
// module con theo trách nhiệm, để dễ đọc / dễ maintain hơn:
//   token.rs    — TokenKind + Token
//   tables.rs   — bảng keyword / component / màu
//   error.rs    — LexError
//   scan.rs     — các phương thức scan chi tiết của Lexer (impl bổ sung)
//   helpers.rs  — hàm module-level không cần &self
//   tests.rs    — unit tests (chỉ build khi cfg(test))
// ============================================================

mod error;
mod helpers;
mod scan;
mod tables;
mod token;

#[cfg(test)]
mod tests;

use std::collections::HashMap;

pub use error::LexError;
pub use tables::resolve_color_name;
pub use token::{Token, TokenKind};

use helpers::{is_ident_start, single_char_bracket, single_char_operator};
use tables::{color_map, component_set, keyword_map};

// ════════════════════════════════════════════════════════════
// 4. LEXER
// ════════════════════════════════════════════════════════════

pub struct Lexer {
    /// Source lưu dưới dạng Vec<char> chứ không phải &str/String thô —
    /// ViBao có ký tự tiếng Việt (UTF-8 nhiều byte/ký tự), việc index
    /// trực tiếp theo byte trên String sẽ cắt giữa ký tự multi-byte và
    /// panic. Vec<char> cho phép random-access O(1) theo đúng "ký tự"
    /// người dùng nhìn thấy, tương đương cách bản JS/TS cũ dùng src[i]
    /// (JS string index vốn theo UTF-16 code unit, cũng an toàn tương tự
    /// cho phần lớn ký tự tiếng Việt, nên hành vi giữ được nhất quán).
    chars: Vec<char>,
    pos: usize,
    line: usize,
    column: usize,
    tokens: Vec<Token>,
    keywords: HashMap<&'static str, TokenKind>,
    components: Vec<&'static str>,
    colors: HashMap<&'static str, &'static str>,
}

impl Lexer {
    pub fn new(source: &str) -> Self {
        Lexer {
            chars: source.chars().collect(),
            pos: 0,
            line: 1,
            column: 1,
            tokens: Vec::new(),
            keywords: keyword_map(),
            components: component_set(),
            colors: color_map(),
        }
    }

    pub fn tokenize(mut self) -> Result<Vec<Token>, LexError> {
        while !self.is_eof() {
            self.skip_whitespace();
            if self.is_eof() {
                break;
            }

            let ch = self.peek(0);

            // Comment
            if ch == '/' && self.peek(1) == '/' {
                self.skip_line_comment();
                continue;
            }
            if ch == '/' && self.peek(1) == '*' {
                self.skip_block_comment()?;
                continue;
            }

            // String
            if ch == '"' {
                let tok = self.read_string()?;
                self.tokens.push(tok);
                continue;
            }

            // Hex color
            if ch == '#' {
                let tok = self.read_hex_color()?;
                self.tokens.push(tok);
                continue;
            }

            // Variable $ten
            if ch == '$' {
                let tok = self.read_variable();
                self.tokens.push(tok);
                continue;
            }

            // Arrow → (1 ký tự Unicode)
            if ch == '→' {
                let (line, col) = (self.line, self.column);
                self.advance();
                self.tokens.push(Token::new(TokenKind::Arrow, line, col));
                continue;
            }
            // Arrow ASCII ->
            if ch == '-' && self.peek(1) == '>' {
                let (line, col) = (self.line, self.column);
                self.advance();
                self.advance();
                self.tokens.push(Token::new(TokenKind::Arrow, line, col));
                continue;
            }

            // Toán tử 2 ký tự — PHẢI check trước single-char để không bị
            // tách nhầm thành 2 token riêng (vd "==" thành Equals + Equals).
            if let Some(tok) = self.try_two_char_op() {
                self.tokens.push(tok);
                continue;
            }

            // "-" đứng riêng: toán tử trừ hay dấu âm của số? Quyết định
            // dựa trên token liền trước — nếu trước đó đã là 1 giá trị
            // hoàn chỉnh (số/biến/định danh/đóng ngoặc/chuỗi), "-" ở đây
            // luôn là phép trừ, bất kể có dính liền digit theo sau hay
            // không (vd "$n-1" và "$n - 1" đều là phép trừ). Đây là bug
            // đã gặp và sửa 2 lần ở bản TS/JS cũ (mini-compiler và bộ
            // compiler TS đầy đủ) — viết đúng ngay từ đầu ở bản Rust.
            if ch == '-' {
                let prev_is_value = self.prev_token_is_value();
                if prev_is_value {
                    let (line, col) = (self.line, self.column);
                    self.advance();
                    self.tokens.push(Token::new(TokenKind::Minus, line, col));
                    continue;
                }
                // Không ở vị trí toán hạng trước đó → có thể là dấu âm,
                // rơi tiếp xuống nhánh Number bên dưới.
            }

            // Số (kể cả số âm dính liền digit, và hậu tố đơn vị px/%/...)
            if ch.is_ascii_digit() || (ch == '-' && self.peek(1).is_ascii_digit()) {
                let tok = self.read_number();
                self.tokens.push(tok);
                continue;
            }

            // Toán tử 1 ký tự còn lại: + * > <
            if let Some(kind) = single_char_operator(ch) {
                let (line, col) = (self.line, self.column);
                self.advance();
                self.tokens.push(Token::new(kind, line, col));
                continue;
            }

            // '!' đứng riêng (phủ định logic): "!=" đã được try_two_char_op()
            // bắt ở trên rồi, nên nếu còn rơi tới đây thì chắc chắn là '!' đơn.
            if ch == '!' {
                let (line, col) = (self.line, self.column);
                self.advance();
                self.tokens.push(Token::new(TokenKind::Bang, line, col));
                continue;
            }

            // '%' đứng riêng (toán tử chia dư): chỉ tới được nhánh này khi
            // KHÔNG đứng ngay sau digit, vì trường hợp đó đã bị read_number()
            // nuốt làm hậu tố đơn vị CSS rồi (xem read_number, nhánh Số ở
            // trên gọi read_number khi ch.is_ascii_digit()). Ví dụ:
            //   "50%"   → NumberLit(50, "50%")           — đơn vị CSS
            //   "$n % 2" hoặc "$n%2" → Variable, Percent, NumberLit — modulo
            if ch == '%' {
                let (line, col) = (self.line, self.column);
                self.advance();
                self.tokens.push(Token::new(TokenKind::Percent, line, col));
                continue;
            }

            // Identifier / keyword / component / màu
            if is_ident_start(ch) {
                let tok = self.read_identifier();
                self.tokens.push(tok);
                continue;
            }

            // Dấu ngoặc & ký tự đơn còn lại
            if let Some(kind) = single_char_bracket(ch) {
                let (line, col) = (self.line, self.column);
                self.advance();
                self.tokens.push(Token::new(kind, line, col));
                continue;
            }

            // Ký tự không nhận diện được — báo lỗi rõ ràng thay vì bỏ qua
            // im lặng (chính kiểu "bỏ qua im lặng" này là nguồn gốc nhiều
            // bug khó debug nhất ở bản TS/JS cũ — lệch token stream mà
            // không có dấu vết gì để lần ra).
            return Err(LexError {
                message: format!("Ký tự không nhận dạng được: '{}'", ch),
                line: self.line,
                column: self.column,
            });
        }

        let (line, col) = (self.line, self.column);
        self.tokens.push(Token::new(TokenKind::Eof, line, col));
        Ok(self.tokens)
    }
}

// ════════════════════════════════════════════════════════════
// 6. PUBLIC ENTRY POINT
// ════════════════════════════════════════════════════════════

/// Tokenize 1 chuỗi source ViBao. Dùng bởi main.rs và parser.rs.
pub fn tokenize(source: &str) -> Result<Vec<Token>, LexError> {
    Lexer::new(source).tokenize()
}
