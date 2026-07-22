// ============================================================
// VIBAO COMPILER (Rust) — parser/action.rs
// Xử lý các câu lệnh thực thi (Action) bên trong khối sự kiện,
// bao gồm phép gán trạng thái, gọi hàm thông thường và gọi API.
// ============================================================

use super::{ParseError, Parser};
use vibao_ast::{Action, Expr};
use crate::lexer::TokenKind;

impl Parser {
    /// Thao tác parse một câu lệnh hành động hoàn chỉnh trong hàm xử lý sự kiện
    pub(crate) fn parse_action(&mut self) -> Result<Action, ParseError> {
        let pos = self.current_pos();

        // Xử lý khối rẽ nhánh cục bộ bên trong event: neu dieu_kien { ... }
        if self.check(&TokenKind::Neu) {
            self.advance();
            let condition = self.parse_value()?;
            self.consume(&TokenKind::LBrace, "Mong đợi '{' mở khối hành động 'neu'")?;
            let mut consequent = Vec::new();
            while !self.check(&TokenKind::RBrace) && !self.is_at_end() {
                consequent.push(self.parse_action()?);
            }
            self.consume(&TokenKind::RBrace, "Mong đợi '}' đóng khối hành động 'neu'")?;

            let mut alternate = None;
            if self.check(&TokenKind::KhongThi) {
                self.advance();
                self.consume(&TokenKind::LBrace, "Mong đợi '{' mở khối hành động 'khong_thi'")?;
                let mut alt_body = Vec::new();
                while !self.check(&TokenKind::RBrace) && !self.is_at_end() {
                    alt_body.push(self.parse_action()?);
                }
                self.consume(&TokenKind::RBrace, "Mong đợi '}' đóng khối hành động 'khong_thi'")?;
                alternate = Some(alt_body);
            }
            return Ok(Action::IfAction { condition, consequent, alternate, pos });
        }

        // Nhận diện lưu kết quả trả về của hàm/api vào biến hứng: $res = ...
        // Phải lookahead TRƯỚC KHI tiêu thụ token, để phân biệt đúng:
        //   $x = 5          → Assign
        //   $x = ham(...)   → FunctionCall/ApiCall với assign_to = Some("x")
        // (Bug cũ: advance() qua $var và '=' rồi mới check_at(1, LParen) nên
        // lookahead bị lệch vị trí, luôn rơi vào Assign kể cả khi vế phải là
        // một lời gọi hàm/api.)
        let mut assign_to = None;
        if let TokenKind::Variable(name) = &self.current().kind {
            if self.check_at(1, &TokenKind::Equals) {
                // Token ngay sau '=' nằm ở offset 2; là lời gọi hàm/api khi
                // offset 2 là Identifier/Component VÀ offset 3 là LParen.
                let is_call = matches!(
                    self.peek(2).kind,
                    TokenKind::Identifier(_) | TokenKind::Component(_)
                ) && self.check_at(3, &TokenKind::LParen);

                let name = name.clone();
                if !is_call {
                    self.advance(); // tiêu thụ biến
                    self.advance(); // tiêu thụ '='
                    let value = self.parse_value()?;
                    self.skip_comma();
                    return Ok(Action::Assign { target: name, value, pos });
                }
                // Là lời gọi hàm/api: tiêu thụ $var và '=', giữ tên biến lại
                // để gắn vào assign_to của FunctionCall/ApiCall bên dưới.
                self.advance(); // tiêu thụ biến
                self.advance(); // tiêu thụ '='
                assign_to = Some(name);
            }
        }

        // Tên của Hàm chức năng hoặc tác vụ gọi API
        let name = match self.advance().kind {
            TokenKind::Identifier(s) | TokenKind::Component(s) => s,
            other => return Err(self.error(format!("Cấu trúc câu lệnh hành động không hợp lệ: {}", other))),
        };

        self.consume(&TokenKind::LParen, "Mong đợi '(' để truyền tham số hành động")?;
        
        let mut args = Vec::new();
        let mut opts = Vec::new();

        while !self.check(&TokenKind::RParen) && !self.is_at_end() {
            // Nhận diện Named Option (lựa chọn có tên gọi) dạng key:value (vd: kieu: thanh_cong)
            if let TokenKind::Identifier(k) = &self.current().kind {
                if self.check_at(1, &TokenKind::Colon) {
                    let key = k.clone();
                    self.advance(); // tiêu thụ định danh
                    self.advance(); // tiêu thụ ':'
                    let val = self.parse_value()?;
                    opts.push((key, val));
                    self.skip_comma();
                    continue;
                }
            }

            args.push(self.parse_value()?);
            self.skip_comma();
        }
        self.consume(&TokenKind::RParen, "Mong đợi ')' để kết thúc tham số")?;

        // Đặc biệt hóa cho tác vụ mạng "goi_api" -> Chuyển thành cấu trúc ApiCall riêng biệt trong AST
        if name == "goi_api" || name == "api" {
            let method = if !args.is_empty() {
                match &args[0] {
                    Expr::Literal(vibao_ast::LiteralValue::Str(m), _) => m.clone(),
                    _ => "GET".to_string(),
                }
            } else {
                "GET".to_string()
            };

            let endpoint = if args.len() > 1 {
                args[1].clone()
            } else {
                Expr::literal_str("", pos)
            };

            let data = if args.len() > 2 { Some(args[2].clone()) } else { None };

            let mut on_success = None;
            let mut on_failure = None;

            // Kiểm tra và bóc tách các khối callback dạng lồng `{ thanh_cong: { ... } }`
            if self.match_token(&TokenKind::LBrace) {
                while !self.check(&TokenKind::RBrace) && !self.is_at_end() {
                    let cb_name = self.expect_identifier_like()?;
                    self.consume(&TokenKind::Colon, "Mong đợi ':' sau định danh phản hồi api")?;
                    self.consume(&TokenKind::LBrace, "Mong đợi '{' để triển khai khối mã phản hồi")?;
                    let mut cb_body = Vec::new();
                    while !self.check(&TokenKind::RBrace) && !self.is_at_end() {
                        cb_body.push(self.parse_action()?);
                    }
                    self.consume(&TokenKind::RBrace, "Mong đợi '}' để đóng khối mã phản hồi")?;
                    
                    if cb_name == "thanh_cong" || cb_name == "success" {
                        on_success = Some(cb_body);
                    } else if cb_name == "that_bai" || cb_name == "failure" {
                        on_failure = Some(cb_body);
                    }
                }
                self.consume(&TokenKind::RBrace, "Mong đợi '}' để đóng hoàn toàn khối xử lý API")?;
            }

            return Ok(Action::ApiCall {
                method,
                endpoint,
                data,
                assign_to,
                on_success,
                on_failure,
                pos,
            });
        }

        self.skip_comma();

        Ok(Action::FunctionCall {
            name,
            args,
            opts,
            assign_to,
            pos,
        })
    }
}
