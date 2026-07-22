// ============================================================
// VIBAO COMPILER (Rust) — parser/control.rs
// Xử lý các cấu trúc luồng điều khiển hiển thị (Control Flow)
// như điều kiện 'neu' / 'khong_thi' và cấu trúc lặp 'vong_lap'.
// ============================================================

use super::{ParseError, Parser};
use vibao_ast::{CaseNode, Child, IfNode, LoopKind, LoopNode, SwitchNode};
use crate::lexer::TokenKind;

impl Parser {
    /// Parse cấu trúc điều kiện: neu biểu_thức { ... } khong_thi { ... }
    pub(crate) fn parse_if_node(&mut self) -> Result<IfNode, ParseError> {
        let pos = self.current_pos();
        self.consume(&TokenKind::Neu, "Mong đợi từ khóa 'neu'")?;
        let condition = self.parse_value()?;
        
        self.consume(&TokenKind::LBrace, "Mong đợi '{' để bắt đầu khối của 'neu'")?;
        let mut consequent = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_at_end() {
            consequent.push(self.parse_child()?);
        }
        self.consume(&TokenKind::RBrace, "Mong đợi '}' để đóng khối của 'neu'")?;

        let mut alternate = None;
        if self.check(&TokenKind::KhongThi) {
            self.advance(); // tiêu thụ 'khong_thi'
            if self.check(&TokenKind::Neu) {
                // Khớp dạng khong_thi neu (Else If lồng nhau)
                let nested_if = self.parse_if_node()?;
                alternate = Some(vec![Child::If(Box::new(nested_if))]);
            } else {
                self.consume(&TokenKind::LBrace, "Mong đợi '{' để bắt đầu khối 'khong_thi'")?;
                let mut alt_body = Vec::new();
                while !self.check(&TokenKind::RBrace) && !self.is_at_end() {
                    alt_body.push(self.parse_child()?);
                }
                self.consume(&TokenKind::RBrace, "Mong đợi '}' để đóng khối 'khong_thi'")?;
                alternate = Some(alt_body);
            }
        }

        Ok(IfNode {
            condition,
            consequent,
            alternate,
            pos,
        })
    }

    /// Parse cấu trúc vòng lặp: vong_lap $item trong $mang hoặc vòng lặp range số tăng dần
    pub(crate) fn parse_loop_node(&mut self) -> Result<LoopNode, ParseError> {
        let pos = self.current_pos();
        self.consume(&TokenKind::VongLap, "Mong đợi từ khóa 'vong_lap'")?;
        
        let kind = if let TokenKind::Variable(item_var) = self.current().kind.clone() {
            self.advance();
            if self.check_ident("trong") || self.check_ident("in") {
                self.advance();
            }
            let iterable = self.parse_value()?;
            LoopKind::Each {
                iterable,
                item_var,
                index_var: None,
            }
        } else if self.check_ident("tu") || self.check_ident("from") {
            self.advance();
            let from_val = match self.advance().kind {
                TokenKind::NumberLit(v, _) => v as i64,
                _ => return Err(self.error("Mong đợi số bắt đầu hợp lệ trong vòng lặp đoạn (range)")),
            };
            if self.check_ident("den") || self.check_ident("to") {
                self.advance();
            }
            let to_val = match self.advance().kind {
                TokenKind::NumberLit(v, _) => v as i64,
                _ => return Err(self.error("Mong đợi số kết thúc hợp lệ trong vòng lặp đoạn (range)")),
            };
            LoopKind::Range { from: from_val, to: to_val }
        } else {
            return Err(self.error("Cú pháp khai báo vòng lặp không được hỗ trợ hoặc viết sai định dạng"));
        };

        self.consume(&TokenKind::LBrace, "Mong đợi '{' để bắt đầu thân vòng lặp")?;
        let mut body = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_at_end() {
            body.push(self.parse_child()?);
        }
        self.consume(&TokenKind::RBrace, "Mong đợi '}' để đóng khối vòng lặp")?;

        Ok(LoopNode { kind, body, pos })
    }

    /// Parse cấu trúc rẽ nhánh nhiều điều kiện:
    ///   truong_hop $bien {
    ///       gia_tri_1 { ... }
    ///       gia_tri_2 { ... }
    ///       mac_dinh { ... }
    ///   }
    ///
    /// TRƯỚC ĐÂY CHƯA TỒN TẠI: dù lexer có sẵn token TruongHop/MacDinh
    /// và AST có sẵn SwitchNode/CaseNode, không có hàm parser nào tạo ra
    /// được node này (xác nhận qua warning "never constructed" khi
    /// build) — đây là lần đầu cú pháp này thực sự hoạt động.
    ///
    /// Mỗi "case" là 1 biểu thức (giá trị so khớp, parse qua
    /// parse_value() để tái dùng đúng logic biểu thức đã có — cho phép
    /// case là literal, biến, hay biểu thức phức tạp hơn) theo sau bởi
    /// khối `{...}` — PHÂN BIỆT với cú pháp element (`tag(props) {...}`)
    /// bằng cách: không có `(...)` ngay sau, mà `{` xuất hiện thẳng sau
    /// biểu thức case. `mac_dinh { ... }` (nếu có) phải là nhánh CUỐI
    /// CÙNG — giống switch/default ở phần lớn ngôn ngữ khác, dù trình
    /// biên dịch không cưỡng ép thứ tự nghiêm ngặt (mac_dinh xuất hiện
    /// giữa chừng vẫn được chấp nhận, chỉ là bất thường về phong cách).
    pub(crate) fn parse_switch_node(&mut self) -> Result<SwitchNode, ParseError> {
        let pos = self.current_pos();
        self.consume(&TokenKind::TruongHop, "Mong đợi từ khóa 'truong_hop'")?;
        let subject = self.parse_value()?;

        self.consume(&TokenKind::LBrace, "Mong đợi '{' để bắt đầu khối của 'truong_hop'")?;

        let mut cases = Vec::new();
        let mut default_case = None;

        while !self.check(&TokenKind::RBrace) && !self.is_at_end() {
            if self.check(&TokenKind::MacDinh) {
                self.advance(); // tiêu thụ 'mac_dinh'
                self.consume(&TokenKind::LBrace, "Mong đợi '{' để bắt đầu khối 'mac_dinh'")?;
                let mut body = Vec::new();
                while !self.check(&TokenKind::RBrace) && !self.is_at_end() {
                    body.push(self.parse_child()?);
                }
                self.consume(&TokenKind::RBrace, "Mong đợi '}' để đóng khối 'mac_dinh'")?;
                if default_case.is_some() {
                    return Err(self.error(
                        "Chỉ được có tối đa 1 khối 'mac_dinh' trong 1 'truong_hop'",
                    ));
                }
                default_case = Some(body);
            } else {
                let case_pos = self.current_pos();
                let value = self.parse_value()?;
                self.consume(&TokenKind::LBrace, "Mong đợi '{' để bắt đầu khối giá trị trong 'truong_hop'")?;
                let mut body = Vec::new();
                while !self.check(&TokenKind::RBrace) && !self.is_at_end() {
                    body.push(self.parse_child()?);
                }
                self.consume(&TokenKind::RBrace, "Mong đợi '}' để đóng khối giá trị trong 'truong_hop'")?;
                cases.push(CaseNode {
                    value,
                    body,
                    pos: case_pos,
                });
            }
        }

        self.consume(&TokenKind::RBrace, "Mong đợi '}' để đóng khối 'truong_hop'")?;

        Ok(SwitchNode {
            subject,
            cases,
            default_case,
            pos,
        })
    }
}
