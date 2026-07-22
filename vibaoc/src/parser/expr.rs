// ============================================================
// VIBAO COMPILER (Rust) — parser/expr.rs
// Expression parsing: Pratt parser cho biểu thức (literal, biến,
// toán tử hai ngôi, gọi hàm, hàm màu, mảng, object, template
// string). Tương đương phần expression trong 05-parser-core.ts
// + toàn bộ 06-parser-expr.ts của bản TS cũ, gộp lại đây vì Rust
// tách theo NHÓM CHỨC NĂNG (mọi thứ liên quan tới "biểu thức")
// thay vì tách theo "core vs mở rộng" như bản TS.
// ============================================================

use super::{ParseError, Parser};
use vibao_ast::{BinaryOp, ColorFuncKind, Expr, LiteralValue, Pos, TemplatePart, UnaryOp};
use crate::lexer::TokenKind;

impl Parser {
    // ════════════════════════════════════════════════════════
    // ENTRY POINT — parse 1 biểu thức đầy đủ (có toán tử)
    // ════════════════════════════════════════════════════════

    /// Parse biểu thức bằng thuật toán Pratt parsing (precedence climbing).
    /// min_prec là ngưỡng độ ưu tiên tối thiểu để tiếp tục "ăn" thêm toán
    /// tử — đệ quy giảm dần khi gặp toán tử ưu tiên thấp hơn ngưỡng.
    pub(crate) fn parse_expr(&mut self, min_prec: u8) -> Result<Expr, ParseError> {
        let mut left = self.parse_primary()?;

        loop {
            let op = match self.current_binary_op() {
                Some(op) => op,
                None => break,
            };
            let prec = binary_precedence(op);
            if prec <= min_prec {
                break;
            }
            let pos = left.pos();
            self.advance(); // tiêu thụ token toán tử
            let right = self.parse_expr(prec)?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
                pos,
            };
        }

        Ok(left)
    }

    /// Tiện ích gọi nhanh parse_expr(0) — dùng ở mọi nơi cần "1 giá trị
    /// bất kỳ" (props, args, ...) mà không quan tâm ngưỡng ưu tiên.
    pub(crate) fn parse_value(&mut self) -> Result<Expr, ParseError> {
        self.parse_expr(0)
    }

    // ════════════════════════════════════════════════════════
    // BINARY OPERATOR DETECTION
    // ════════════════════════════════════════════════════════

    /// Nhận diện token hiện tại có phải toán tử hai ngôi không, trả về
    /// None nếu không phải (dừng vòng lặp Pratt parser). Check theo
    /// TokenKind tường minh — không dựa vào giá trị chuỗi như bug đã
    /// từng gặp ở bản TS cũ (getBinaryOp so t.value thay vì t.type).
    fn current_binary_op(&self) -> Option<BinaryOp> {
        match &self.current().kind {
            TokenKind::Plus => Some(BinaryOp::Add),
            TokenKind::Minus => Some(BinaryOp::Sub),
            TokenKind::Star => Some(BinaryOp::Mul),
            TokenKind::Slash => Some(BinaryOp::Div),
            TokenKind::Percent => Some(BinaryOp::Mod),
            TokenKind::Gt => Some(BinaryOp::Gt),
            TokenKind::Lt => Some(BinaryOp::Lt),
            TokenKind::Gte => Some(BinaryOp::Gte),
            TokenKind::Lte => Some(BinaryOp::Lte),
            TokenKind::EqEq => Some(BinaryOp::Eq),
            TokenKind::Neq => Some(BinaryOp::Neq),
            TokenKind::AndAnd => Some(BinaryOp::And),
            TokenKind::OrOr => Some(BinaryOp::Or),
            _ => None,
        }
    }

    // ════════════════════════════════════════════════════════
    // PRIMARY EXPRESSION
    // ════════════════════════════════════════════════════════

    fn parse_primary(&mut self) -> Result<Expr, ParseError> {
        let pos = self.current_pos();

        // Unary: !expr — phủ định logic (vd: !$da_dang_nhap)
        if matches!(self.current().kind, TokenKind::Bang) {
            self.advance();
            let operand = self.parse_primary()?;
            return Ok(Expr::Unary {
                op: UnaryOp::Not,
                operand: Box::new(operand),
                pos,
            });
        }

        // Unary: -expr (không có trong spec ViBao hiện tại nhưng giữ chỗ
        // cho tương lai — "-" đứng ở vị trí toán hạng đã được lexer coi
        // là dấu âm gộp thẳng vào NumberLit rồi, không cần xử lý Unary
        // Neg riêng ở đây cho trường hợp số; chỉ giữ cho phép mở rộng).
        if matches!(self.current().kind, TokenKind::Minus) {
            self.advance();
            let operand = self.parse_primary()?;
            return Ok(Expr::Unary {
                op: UnaryOp::Neg,
                operand: Box::new(operand),
                pos,
            });
        }

        // ( expr )
        if self.check(&TokenKind::LParen) {
            self.advance();
            let inner = self.parse_expr(0)?;
            self.expect(&TokenKind::RParen)?;
            return Ok(inner);
        }

        // Array [ ... ]
        if self.check(&TokenKind::LBracket) {
            return self.parse_array();
        }

        // Object { key: value, ... }
        // CẢNH BÁO NGỮ CẢNH: '{' cũng được dùng cho action block
        // (on_click: { $n = $n - 1 }) và children block (element {
        // ... }) ở những nơi khác trong ngữ pháp ViBao — KHÔNG PHẢI
        // lúc nào gặp '{' cũng nên hiểu là Object literal. parse_primary()
        // chỉ nên được gọi trong ngữ cảnh chắc chắn đang chờ 1 EXPRESSION
        // (vd bên phải dấu '=', bên trong danh sách args của function
        // call). action.rs và element.rs PHẢI tự nhận diện và tiêu thụ
        // '{' của action-block/children-block TRƯỚC khi gọi parse_expr(),
        // không bao giờ được để parse_expr tự ý quyết định ý nghĩa của
        // '{' trong 2 ngữ cảnh đó — nếu không sẽ nuốt nhầm y hệt kiểu bug
        // đã từng gặp ở bản TS cũ khi 1 construct bị hiểu sai ngữ cảnh.
        if self.check(&TokenKind::LBrace) {
            return self.parse_object();
        }

        // Hàm màu: trong_suot(...), lam_sang(...), lam_toi(...)
        if let TokenKind::Identifier(name) = &self.current().kind {
            let name = name.clone();
            if matches!(name.as_str(), "trong_suot" | "lam_sang" | "lam_toi") {
                return self.parse_color_func(&name, pos);
            }
        }

        // Function call trong expression: gia_tien($x), rut_gon($s, 50)
        // Component cũng có thể được gọi như hàm trong 1 số ngữ cảnh
        // (action: dieu_huong("/")) nên check cả Identifier lẫn Component
        // theo sau bởi LParen.
        let callee_name = match &self.current().kind {
            TokenKind::Identifier(n) => Some(n.clone()),
            TokenKind::Component(n) => Some(n.clone()),
            _ => None,
        };
        if let Some(name) = callee_name {
            if self.check_at(1, &TokenKind::LParen) {
                self.advance(); // tên hàm
                self.advance(); // (
                let mut args = Vec::new();
                while !self.check(&TokenKind::RParen) && !self.check(&TokenKind::Eof) {
                    args.push(self.parse_expr(0)?);
                    self.skip_comma();
                }
                self.expect(&TokenKind::RParen)?;
                return Ok(Expr::Call { callee: name, args, pos });
            }
        }

        // Biến $ten (+ member access $obj.field.sub)
        if let TokenKind::Variable(_) = &self.current().kind {
            return self.parse_variable_expr();
        }

        // Literal (string/number/bool/color) — bao gồm cả bare identifier
        // dùng làm giá trị chuỗi (vd huong:row, can:giua — xem parse_literal).
        self.parse_literal()
    }

    // ── Biến + member access ($obj.field.sub) ──────────────────────
    fn parse_variable_expr(&mut self) -> Result<Expr, ParseError> {
        let pos = self.current_pos();
        let name = match self.advance().kind {
            TokenKind::Variable(n) => n,
            _ => unreachable!("đã check Variable trước khi gọi hàm này"),
        };

        // Chuỗi có nội suy: nếu tên biến chứa $ bên trong do template
        // string được lexer tách khác (ViBao dùng chuỗi "Xin chào $ten"
        // parse riêng ở read_string của lexer thành 1 StringLit thô, rồi
        // parse_literal() bên dưới mới tách thành TemplateString nếu cần
        // — variable đứng riêng ở đây luôn là 1 biến đơn thuần $ten).
        let mut node = Expr::Variable(name, pos);

        while self.check(&TokenKind::Dot) {
            self.advance();
            let prop = match &self.current().kind {
                TokenKind::Identifier(s) => s.clone(),
                TokenKind::Component(s) => s.clone(), // vd $item.text nếu "text" trùng tên component
                other => {
                    return Err(self.error(format!(
                        "Mong đợi tên field sau dấu '.', nhận được {}",
                        other
                    )))
                }
            };
            self.advance();
            node = Expr::MemberAccess {
                object: Box::new(node),
                property: prop,
                pos,
            };
        }

        Ok(node)
    }

    // ── Literal: string/number/bool/color/bare-identifier ──────────
    pub(crate) fn parse_literal(&mut self) -> Result<Expr, ParseError> {
        let pos = self.current_pos();
        let tok = self.current().kind.clone();

        match tok {
            TokenKind::StringLit(s) => {
                self.advance();
                // Nếu chuỗi có chứa "$", tách thành TemplateString để
                // codegen sau này bind đúng biến động — xem parse_template_string.
                if s.contains('$') {
                    Ok(parse_template_string(&s, pos))
                } else {
                    Ok(Expr::Literal(LiteralValue::Str(s), pos))
                }
            }
            TokenKind::NumberLit(v, raw) => {
                self.advance();
                Ok(Expr::literal_num_with_unit(v, extract_unit_suffix(&raw), pos))
            }
            TokenKind::BoolLit(b) => {
                self.advance();
                Ok(Expr::Literal(LiteralValue::Bool(b), pos))
            }
            TokenKind::ColorHex(h) => {
                self.advance();
                Ok(Expr::Literal(LiteralValue::Color(h), pos))
            }
            TokenKind::ColorName(n) => {
                self.advance();
                let hex = crate::lexer::resolve_color_name(&n);
                Ok(Expr::Literal(LiteralValue::Color(hex), pos))
            }
            // Bare identifier dùng làm giá trị chuỗi — ViBao dùng rất
            // nhiều kiểu này: huong:row, can:giua, fit:cover, loai:email.
            // Đây là bug đã tìm và sửa ở bản TS cũ (thiếu hẳn nhánh này
            // khiến parser crash với mọi prop dùng từ khoá trần) — viết
            // đúng ngay từ đầu ở bản Rust.
            TokenKind::Identifier(s) => {
                self.advance();
                Ok(Expr::Literal(LiteralValue::Str(s), pos))
            }
            TokenKind::Component(s) => {
                self.advance();
                Ok(Expr::Literal(LiteralValue::Str(s), pos))
            }
            other => Err(self.error(format!("Không parse được giá trị: {}", other))),
        }
    }

    // ── Hàm màu: trong_suot(mau, amount), lam_sang(...), lam_toi(...) ──
    fn parse_color_func(&mut self, name: &str, pos: Pos) -> Result<Expr, ParseError> {
        let kind = match name {
            "trong_suot" => ColorFuncKind::TrongSuot,
            "lam_sang" => ColorFuncKind::LamSang,
            "lam_toi" => ColorFuncKind::LamToi,
            _ => unreachable!("đã check tên hàm màu hợp lệ trước khi gọi"),
        };
        self.advance(); // tên hàm
        self.expect(&TokenKind::LParen)?;
        let color = self.parse_expr(0)?;
        self.expect(&TokenKind::Comma)?;
        let amount_tok = self.expect(&TokenKind::NumberLit(0.0, String::new()))?;
        let amount = match amount_tok.kind {
            TokenKind::NumberLit(v, _) => v,
            _ => unreachable!(),
        };
        self.expect(&TokenKind::RParen)?;
        Ok(Expr::ColorFunc {
            func: kind,
            color: Box::new(color),
            amount,
            pos,
        })
    }

    // ── Array [item1, item2, ...] ────────────────────────────────────
    fn parse_array(&mut self) -> Result<Expr, ParseError> {
        let pos = self.current_pos();
        self.expect(&TokenKind::LBracket)?;
        let mut items = Vec::new();
        while !self.check(&TokenKind::RBracket) && !self.check(&TokenKind::Eof) {
            items.push(self.parse_expr(0)?);
            self.skip_comma();
        }
        self.expect(&TokenKind::RBracket)?;
        Ok(Expr::Array(items, pos))
    }

    // ── Object { key: value, ... } ────────────────────────────────────
    fn parse_object(&mut self) -> Result<Expr, ParseError> {
        let pos = self.current_pos();
        self.expect(&TokenKind::LBrace)?;
        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.check(&TokenKind::Eof) {
            let key = self.expect_identifier_like()?;
            self.expect(&TokenKind::Colon)?;
            let value = self.parse_expr(0)?;
            fields.push((key, value));
            self.skip_comma();
        }
        self.expect(&TokenKind::RBrace)?;
        Ok(Expr::Object(fields, pos))
    }

    /// Đọc 1 tên field/key dạng identifier — chấp nhận cả Identifier lẫn
    /// Component (vd key tên trùng với 1 component có sẵn như "text").
    pub(crate) fn expect_identifier_like(&mut self) -> Result<String, ParseError> {
        match &self.current().kind {
            TokenKind::Identifier(s) => {
                let s = s.clone();
                self.advance();
                Ok(s)
            }
            TokenKind::Component(s) => {
                let s = s.clone();
                self.advance();
                Ok(s)
            }
            other => Err(self.error(format!(
                "Mong đợi tên định danh, nhận được {}",
                other
            ))),
        }
    }
}

// ════════════════════════════════════════════════════════════
// PRECEDENCE TABLE
// ════════════════════════════════════════════════════════════

fn binary_precedence(op: BinaryOp) -> u8 {
    match op {
        BinaryOp::Or => 1,
        BinaryOp::And => 2,
        BinaryOp::Eq
        | BinaryOp::Neq
        | BinaryOp::Gt
        | BinaryOp::Gte
        | BinaryOp::Lt
        | BinaryOp::Lte => 3,
        BinaryOp::Add | BinaryOp::Sub => 4,
        BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => 5,
    }
}

// ════════════════════════════════════════════════════════════
// SỐ + ĐƠN VỊ CSS
// ════════════════════════════════════════════════════════════

/// Tách hậu tố đơn vị CSS (px, %, vw, vh, em, rem) khỏi chuỗi số thô mà
/// lexer trả về (vd "50%" → Some("%"), "16" → None). Lexer (read_number)
/// đã match sẵn các đơn vị này và nối vào cuối chuỗi num — ở đây ta chỉ
/// cần đọc lại phần hậu tố không phải số/dấu chấm/dấu trừ.
fn extract_unit_suffix(raw: &str) -> Option<String> {
    let unit_start = raw.find(|c: char| !c.is_ascii_digit() && c != '.' && c != '-')?;
    let unit = &raw[unit_start..];
    if unit.is_empty() {
        None
    } else {
        Some(unit.to_string())
    }
}

// ════════════════════════════════════════════════════════════
// TEMPLATE STRING PARSING (module-level function, không cần &self)
// ════════════════════════════════════════════════════════════

/// Tách 1 chuỗi thô "Xin chào $ten, bạn $tuoi.nam tuổi" thành
/// Expr::TemplateString với các phần text/variable/member xen kẽ.
/// Được gọi từ parse_literal() khi phát hiện chuỗi chứa dấu "$".
fn parse_template_string(raw: &str, pos: Pos) -> Expr {
    let chars: Vec<char> = raw.chars().collect();
    let mut parts = Vec::new();
    let mut i = 0;
    let mut text_buf = String::new();

    while i < chars.len() {
        if chars[i] == '$' {
            if !text_buf.is_empty() {
                parts.push(TemplatePart::Text(std::mem::take(&mut text_buf)));
            }
            i += 1;
            let mut name = String::new();
            while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                name.push(chars[i]);
                i += 1;
            }

            // $obj.field.sub — gom thành 1 path
            let mut path = vec![name];
            while i < chars.len() && chars[i] == '.' {
                i += 1;
                let mut sub = String::new();
                while i < chars.len() && (chars[i].is_alphanumeric() || chars[i] == '_') {
                    sub.push(chars[i]);
                    i += 1;
                }
                if !sub.is_empty() {
                    path.push(sub);
                }
            }

            if path.len() > 1 {
                parts.push(TemplatePart::Member(path));
            } else {
                parts.push(TemplatePart::Variable(path.into_iter().next().unwrap_or_default()));
            }
        } else {
            text_buf.push(chars[i]);
            i += 1;
        }
    }

    if !text_buf.is_empty() {
        parts.push(TemplatePart::Text(text_buf));
    }

    Expr::TemplateString(parts, pos)
}

// ════════════════════════════════════════════════════════════
// UNIT TESTS
// ════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;

    fn parse_expr_from_source(src: &str) -> Expr {
        let tokens = tokenize(src).unwrap();
        let mut p = Parser::new(tokens);
        p.parse_expr(0).unwrap()
    }

    #[test]
    fn test_simple_arithmetic_precedence() {
        // 1 + 2 * 3  →  phải nhóm thành 1 + (2 * 3), không phải (1+2)*3
        let expr = parse_expr_from_source("1 + 2 * 3");
        match expr {
            Expr::Binary { op: BinaryOp::Add, right, .. } => match *right {
                Expr::Binary { op: BinaryOp::Mul, .. } => {} // đúng
                _ => panic!("Sai precedence: * phải nằm trong nhánh phải của +"),
            },
            _ => panic!("Kết quả không phải Binary Add"),
        }
    }

    #[test]
    fn test_variable_minus_number() {
        // $n - 1 — đúng bug đã gặp ở bản TS/JS cũ (dấu "-" đứng cách biệt)
        let expr = parse_expr_from_source("$n - 1");
        match expr {
            Expr::Binary { op: BinaryOp::Sub, left, right, .. } => {
                assert!(matches!(*left, Expr::Variable(ref s, _) if s == "n"));
                assert!(matches!(*right, Expr::Literal(LiteralValue::Num(v, _), _) if v == 1.0));
            }
            _ => panic!("Phải parse thành phép trừ"),
        }
    }

    #[test]
    fn test_bare_identifier_as_string_literal() {
        // "row" đứng riêng (vd giá trị của huong:row) phải parse được
        // thành string literal, không phải lỗi — bug đã gặp ở bản TS cũ.
        let expr = parse_expr_from_source("row");
        assert!(matches!(expr, Expr::Literal(LiteralValue::Str(ref s), _) if s == "row"));
    }

    #[test]
    fn test_member_access_chain() {
        let expr = parse_expr_from_source("$obj.field.sub");
        match expr {
            Expr::MemberAccess { property, .. } => assert_eq!(property, "sub"),
            _ => panic!("Phải là MemberAccess"),
        }
    }

    #[test]
    fn test_function_call_in_expr() {
        let expr = parse_expr_from_source("gia_tien($gia)");
        match expr {
            Expr::Call { callee, args, .. } => {
                assert_eq!(callee, "gia_tien");
                assert_eq!(args.len(), 1);
            }
            _ => panic!("Phải là Call"),
        }
    }

    #[test]
    fn test_template_string_extraction() {
        let expr = parse_expr_from_source(r#""Xin chào $ten""#);
        match expr {
            Expr::TemplateString(parts, _) => {
                assert_eq!(parts.len(), 2);
                assert!(matches!(parts[0], TemplatePart::Text(ref s) if s == "Xin chào "));
                assert!(matches!(parts[1], TemplatePart::Variable(ref s) if s == "ten"));
            }
            _ => panic!("Phải tách thành TemplateString"),
        }
    }

    #[test]
    fn test_logical_and_or_precedence() {
        // $a > 1 && $b > 2 || $c   →   (($a>1) && ($b>2)) || $c
        let expr = parse_expr_from_source("$a > 1 && $b > 2 || $c");
        match expr {
            Expr::Binary { op: BinaryOp::Or, .. } => {} // đúng: Or ở ngoài cùng (ưu tiên thấp nhất)
            _ => panic!("Toán tử ưu tiên thấp nhất (||) phải nằm ở gốc cây"),
        }
    }

    #[test]
    fn test_color_function() {
        let expr = parse_expr_from_source("trong_suot(den, 50)");
        match expr {
            Expr::ColorFunc { func, amount, .. } => {
                assert_eq!(func, ColorFuncKind::TrongSuot);
                assert_eq!(amount, 50.0);
            }
            _ => panic!("Phải là ColorFunc"),
        }
    }

    #[test]
    fn test_logical_not_unary() {
        let expr = parse_expr_from_source("!$da_dang_nhap");
        match expr {
            Expr::Unary { op: UnaryOp::Not, operand, .. } => {
                assert!(matches!(*operand, Expr::Variable(ref s, _) if s == "da_dang_nhap"));
            }
            _ => panic!("Phải parse thành Unary Not"),
        }
    }

    #[test]
    fn test_modulo_operator() {
        let expr = parse_expr_from_source("$n % 2");
        match expr {
            Expr::Binary { op: BinaryOp::Mod, left, right, .. } => {
                assert!(matches!(*left, Expr::Variable(ref s, _) if s == "n"));
                assert!(matches!(*right, Expr::Literal(LiteralValue::Num(v, _), _) if v == 2.0));
            }
            _ => panic!("Phải parse thành phép chia dư (Mod)"),
        }
    }

    #[test]
    fn test_number_literal_keeps_css_unit() {
        // "50%" phải giữ lại đơn vị "%" trong AST, không chỉ giữ ở lexer —
        // codegen (props.rs/layout.rs) cần biết đây là % chứ không phải px
        // mặc định. Đây là bug đã tìm thấy khi đối chiếu ast.rs cũ (Num(f64)
        // không có chỗ chứa unit) với lexer (NumberLit(f64, String) có raw).
        let expr = parse_expr_from_source("50%");
        match expr {
            Expr::Literal(LiteralValue::Num(v, unit), _) => {
                assert_eq!(v, 50.0);
                assert_eq!(unit, Some("%".to_string()));
            }
            _ => panic!("Phải là Literal Num với unit %"),
        }
    }

    #[test]
    fn test_number_literal_without_unit_is_none() {
        let expr = parse_expr_from_source("16");
        match expr {
            Expr::Literal(LiteralValue::Num(v, unit), _) => {
                assert_eq!(v, 16.0);
                assert_eq!(unit, None);
            }
            _ => panic!("Phải là Literal Num không có unit"),
        }
    }

    #[test]
    fn test_modulo_precedence_with_addition() {
        // 1 + 2 % 3  →  1 + (2 % 3), vì % cùng cấp * / (cao hơn +)
        let expr = parse_expr_from_source("1 + 2 % 3");
        match expr {
            Expr::Binary { op: BinaryOp::Add, right, .. } => match *right {
                Expr::Binary { op: BinaryOp::Mod, .. } => {}
                _ => panic!("% phải nằm trong nhánh phải của +"),
            },
            _ => panic!("Kết quả không phải Binary Add"),
        }
    }
}
