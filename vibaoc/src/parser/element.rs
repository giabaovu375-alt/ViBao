// ============================================================
// VIBAO COMPILER (Rust) — parser/element.rs
// Logic xử lý các thẻ giao diện cơ bản (Element) và việc gọi
// các thành phần tự định nghĩa (ComponentCall), kèm theo Props,
// Sự kiện tương tác, chỉ thị Responsive và Animation.
// ============================================================

use super::{ParseError, Parser};
use vibao_ast::{Element, ComponentCall, EventNode, EventName, PropsMap, AnimationProps, ResponsiveNode, Breakpoint};
use crate::lexer::TokenKind;

impl Parser {
    /// Hoàn thiện việc parse một phần tử UI chuẩn sau khi đã lấy được thẻ tag
    pub(crate) fn parse_element_rest(&mut self, tag: String, pos: vibao_ast::Pos) -> Result<Element, ParseError> {
        let mut props = Vec::new();
        
        // Nhận diện cặp ngoặc chứa thuộc tính: tag(mau: do, co: 16)
        if self.match_token(&TokenKind::LParen) {
            while !self.check(&TokenKind::RParen) && !self.is_at_end() {
                // Hỗ trợ viết tắt không cần key cho tham số nội dung đầu tiên (vd: text("Chào Bạn"))
                if props.is_empty() && (matches!(self.current().kind, TokenKind::StringLit(_)) || matches!(self.current().kind, TokenKind::NumberLit(_, _))) {
                    let val = self.parse_value()?;
                    props.push(("noi_dung".to_string(), val));
                } else {
                    let key = self.expect_identifier_like()?;
                    self.consume(&TokenKind::Colon, "Mong đợi dấu ':' sau tên thuộc tính")?;
                    let val = self.parse_value()?;
                    props.push((key, val));
                }
                self.skip_comma();
            }
            self.consume(&TokenKind::RParen, "Mong đợi ')' để kết thúc danh sách thuộc tính")?;
        }

        let mut children = Vec::new();
        let mut events = Vec::new();
        let mut responsive = Vec::new();
        let mut animation = AnimationProps::default();

        // Nhận diện cặp ngoặc nhọn chứa tập node con hoặc khối xử lý sự kiện
        if self.match_token(&TokenKind::LBrace) {
            while !self.check(&TokenKind::RBrace) && !self.is_at_end() {
                if let Some(event_name) = self.match_event_name() {
                    let e_pos = self.current_pos();
                    self.consume(&TokenKind::LBrace, "Mong đợi '{' để mở đầu khối hành động của sự kiện")?;
                    let mut body = Vec::new();
                    while !self.check(&TokenKind::RBrace) && !self.is_at_end() {
                        body.push(self.parse_action()?);
                    }
                    self.consume(&TokenKind::RBrace, "Mong đợi '}' để đóng sự kiện")?;
                    events.push(EventNode {
                        name: event_name,
                        body,
                        pos: e_pos,
                    });
                } else if self.check(&TokenKind::At) {
                    // Xử lý chỉ thị Responsive: @di_dong, @may_tinh... hoặc hiệu ứng
                    self.advance(); // tiêu thụ '@'
                    let r_pos = self.current_pos();
                    let name = self.expect_identifier_like()?;
                    match name.as_str() {
                        "di_dong" | "mobile" => {
                            let overrides = self.parse_responsive_props()?;
                            responsive.push(ResponsiveNode { breakpoint: Breakpoint::DiDong, overrides, pos: r_pos });
                        }
                        "may_tinh_bang" | "tablet" => {
                            let overrides = self.parse_responsive_props()?;
                            responsive.push(ResponsiveNode { breakpoint: Breakpoint::MayTinhBang, overrides, pos: r_pos });
                        }
                        "may_tinh" | "desktop" => {
                            let overrides = self.parse_responsive_props()?;
                            responsive.push(ResponsiveNode { breakpoint: Breakpoint::MayTinh, overrides, pos: r_pos });
                        }
                        "hieu_ung" => {
                            self.consume(&TokenKind::Colon, "Mong đợi ':' sau chỉ thị @hieu_ung")?;
                            match &self.current().kind {
                                TokenKind::Identifier(act) => {
                                    animation.hieu_ung = Some(act.clone());
                                    self.advance();
                                }
                                other => {
                                    return Err(self.error(format!(
                                        "Mong đợi tên hiệu ứng (identifier) sau '@hieu_ung:', nhận được {}",
                                        other
                                    )))
                                }
                            }
                        }
                        _ => return Err(self.error(format!("Chỉ thị directive '@{}' không được hỗ trợ", name))),
                    }
                } else {
                    children.push(self.parse_child()?);
                }
            }
            self.consume(&TokenKind::RBrace, "Mong đợi '}' để kết thúc khối component")?;
        }

        Ok(Element {
            tag,
            props,
            children,
            events,
            responsive,
            animation,
            pos,
        })
    }

    /// Hoàn thiện việc parse một cú pháp gọi component tự định nghĩa
    pub(crate) fn parse_component_call_rest(&mut self, name: String, pos: vibao_ast::Pos) -> Result<ComponentCall, ParseError> {
        let mut props = Vec::new();
        if self.match_token(&TokenKind::LParen) {
            while !self.check(&TokenKind::RParen) && !self.is_at_end() {
                let key = self.expect_identifier_like()?;
                self.consume(&TokenKind::Colon, "Mong đợi dấu ':' sau tên thuộc tính thành phần")?;
                let val = self.parse_value()?;
                props.push((key, val));
                self.skip_comma();
            }
            self.consume(&TokenKind::RParen, "Mong đợi ')' để kết thúc tham số gọi thành phần")?;
        }

        let mut children = Vec::new();
        if self.match_token(&TokenKind::LBrace) {
            while !self.check(&TokenKind::RBrace) && !self.is_at_end() {
                children.push(self.parse_child()?);
            }
            self.consume(&TokenKind::RBrace, "Mong đợi '}' để đóng khối nội dung gọi thành phần")?;
        }

        Ok(ComponentCall { name, props, children, pos })
    }

    /// Khớp và tiêu thụ tên sự kiện, trả về enum tương thích
    fn match_event_name(&mut self) -> Option<EventName> {
        let name = match &self.current().kind {
            TokenKind::OnClick => Some(EventName::OnClick),
            TokenKind::OnHover => Some(EventName::OnHover),
            TokenKind::OnBlur => Some(EventName::OnBlur),
            TokenKind::OnFocus => Some(EventName::OnFocus),
            TokenKind::OnChange => Some(EventName::OnChange),
            TokenKind::OnSubmit => Some(EventName::OnSubmit),
            TokenKind::OnScroll => Some(EventName::OnScroll),
            _ => None,
        };
        if name.is_some() {
            self.advance();
        }
        name
    }

    /// Trích xuất danh sách ghi đè thuộc tính bên trong khối responsive
    fn parse_responsive_props(&mut self) -> Result<PropsMap, ParseError> {
        self.consume(&TokenKind::LBrace, "Mong đợi '{' sau tên breakpoint")?;
        let mut overrides = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_at_end() {
            let key = self.expect_identifier_like()?;
            self.consume(&TokenKind::Colon, "Mong đợi dấu ':' sau thuộc tính ghi đè responsive")?;
            let val = self.parse_value()?;
            overrides.push((key, val));
            self.skip_comma();
        }
        self.consume(&TokenKind::RBrace, "Mong đợi '}' để đóng khối responsive")?;
        Ok(overrides)
    }
}
