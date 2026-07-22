// ============================================================
// VIBAO COMPILER (Rust) — parser/app.rs
// Định nghĩa logic parse cấu trúc tổng thể: ứng dụng (app),
// theme, biến toàn cục, trang (page), và định nghĩa component (@the).
// ============================================================

use super::{ParseError, Parser};
use vibao_ast::{App, ComponentDef, Page, Theme, VarDecl, StateDecl, ParamDef, DataType, Child, ColorValue};
use crate::lexer::TokenKind;

impl Parser {
    /// Kiểm tra token hiện tại có phải "@the" (2 token: At, rồi
    /// Identifier có GIÁ TRỊ đúng là "the") hay không.
    ///
    /// BUG ĐÃ SỬA: trước đây điều kiện dùng `check_at(1,
    /// &TokenKind::Identifier("the".to_string()))` — nhưng `check`/
    /// `check_at` chỉ so sánh std::mem::discriminant (LOẠI biến thể
    /// enum), KHÔNG so sánh giá trị String bên trong. Kết quả: MỌI
    /// identifier sau dấu "@" (vd "@banner", "@item", "@x") đều bị
    /// nhận nhầm là "@the", khiến parse_component_def() vô tình nuốt
    /// mất 1 token mà không kiểm tra đúng nó có phải "the" hay không —
    /// gây parse sai cấu trúc âm thầm, không báo lỗi gì. Hàm này kiểm
    /// tra ĐÚNG giá trị chuỗi bên trong token Identifier.
    fn is_at_symbol_the(&self) -> bool {
        if !self.check(&TokenKind::At) {
            return false;
        }
        matches!(&self.peek(1).kind, TokenKind::Identifier(s) if s == "the")
    }

    /// Parse điểm vào chính của ứng dụng: ung_dung("Tên") { ... }
    pub(crate) fn parse_app(&mut self) -> Result<App, ParseError> {
        let pos = self.current_pos();
        self.consume(&TokenKind::UngDung, "Mong đợi từ khóa 'ung_dung'")?;
        self.consume(&TokenKind::LParen, "Mong đợi '(' sau từ khóa 'ung_dung'")?;
        
        let name = match self.advance().kind {
            TokenKind::StringLit(s) => s,
            other => return Err(self.error(format!("Mong đợi chuỗi tên ứng dụng, nhận được {}", other))),
        };
        
        self.consume(&TokenKind::RParen, "Mong đợi ')' sau tên ứng dụng")?;
        self.consume(&TokenKind::LBrace, "Mong đợi '{' để mở đầu khối ứng dụng")?;

        let mut variables = Vec::new();
        let mut themes = Vec::new();
        let mut components = Vec::new();
        let mut pages = Vec::new();

        while !self.check(&TokenKind::RBrace) && !self.is_at_end() {
            if self.check(&TokenKind::Theme) {
                themes.push(self.parse_theme()?);
            } else if self.check(&TokenKind::Trang) {
                pages.push(self.parse_page()?);
            } else if self.check(&TokenKind::State) {
                variables.push(self.parse_var_decl()?);
            } else if self.is_at_symbol_the() {
                components.push(self.parse_component_def()?);
            } else if let TokenKind::Variable(_) = &self.current().kind {
                variables.push(self.parse_var_decl()?);
            } else {
                return Err(self.error(format!("Thành phần không hợp lệ bên trong khối ứng dụng: {}", self.current().kind)));
            }
        }

        self.consume(&TokenKind::RBrace, "Mong đợi '}' để đóng khối ứng dụng")?;

        Ok(App {
            name,
            variables,
            themes,
            components,
            pages,
            pos,
        })
    }

    /// Parse cấu trúc Theme: theme TênTheme { $bien = gia_tri }
    fn parse_theme(&mut self) -> Result<Theme, ParseError> {
        let pos = self.current_pos();
        self.advance(); // tiêu thụ 'theme'
        
        let name = match self.advance().kind {
            TokenKind::Identifier(s) | TokenKind::StringLit(s) => s,
            other => return Err(self.error(format!("Mong đợi tên định danh theme, nhận được {}", other))),
        };

        self.consume(&TokenKind::LBrace, "Mong đợi '{' sau tên theme")?;
        let mut variables = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_at_end() {
            variables.push(self.parse_var_decl()?);
        }
        self.consume(&TokenKind::RBrace, "Mong đợi '}' để đóng theme")?;

        Ok(Theme { name, variables, pos })
    }

    /// Parse khai báo biến/trạng thái: $ten_bien = gia_tri hoặc state $ten = gia_tri
    pub(crate) fn parse_var_decl(&mut self) -> Result<VarDecl, ParseError> {
        let pos = self.current_pos();
        if self.check(&TokenKind::State) {
            self.advance();
        }
        
        let name = match self.advance().kind {
            TokenKind::Variable(n) => n,
            other => return Err(self.error(format!("Mong đợi tên biến bắt đầu bằng dấu $, nhận được {}", other))),
        };

        self.consume(&TokenKind::Equals, "Mong đợi dấu '=' sau tên biến")?;
        let value = self.parse_value()?;
        self.skip_comma();

        Ok(VarDecl { name, value, pos })
    }

    /// Parse 1 giá trị màu (dùng cho named option "mau_nen: ..."): chấp
    /// nhận mã hex (#RRGGBB, đã qua lexer dạng ColorHex), tên màu tiếng
    /// Việt (ColorName — lexer đã resolve raw name, tự thân token này
    /// giữ lại TÊN gốc chứ không phải hex, nên vẫn cần bọc ColorValue::Name
    /// để codegen tự resolve ra hex đúng lúc sinh CSS), hoặc biến "$ten".
    fn parse_color_value(&mut self) -> Result<ColorValue, ParseError> {
        match self.advance().kind {
            TokenKind::ColorHex(hex) => Ok(ColorValue::Hex(hex)),
            TokenKind::ColorName(name) => Ok(ColorValue::Name(name)),
            TokenKind::Variable(name) => Ok(ColorValue::Variable(name)),
            other => Err(self.error(format!(
                "Mong đợi giá trị màu (mã hex, tên màu, hoặc biến) cho \"mau_nen\", nhận được {}",
                other
            ))),
        }
    }

    /// Parse cấu trúc Trang: trang("/route", "Tên Trang", mau_nen: xanh) { ... }
    fn parse_page(&mut self) -> Result<Page, ParseError> {
        let pos = self.current_pos();
        self.advance(); // tiêu thụ 'trang'
        self.consume(&TokenKind::LParen, "Mong đợi '(' sau từ khóa 'trang'")?;

        let route = match self.advance().kind {
            TokenKind::StringLit(s) => s,
            other => return Err(self.error(format!("Mong đợi chuỗi định tuyến (route), nhận được {}", other))),
        };

        let mut name = None;
        let mut mau_nen = None;

        // Sau route, có thể có thêm: 1 chuỗi tên trang (vị trí, không tên),
        // và/hoặc named option "mau_nen: ..." — thứ tự không bắt buộc,
        // đọc lặp qua dấu phẩy tới khi gặp ')'.
        while self.match_token(&TokenKind::Comma) {
            // Named option dạng "key: value" (hiện chỉ hỗ trợ mau_nen).
            if let TokenKind::Identifier(k) = &self.current().kind {
                if k == "mau_nen" {
                    if self.check_at(1, &TokenKind::Colon) {
                        self.advance(); // tiêu thụ "mau_nen"
                        self.advance(); // tiêu thụ ':'
                        mau_nen = Some(self.parse_color_value()?);
                        continue;
                    }
                    // BUG ĐÃ SỬA: trước đây nếu thiếu dấu ':' sau
                    // "mau_nen", code rơi xuống nhánh bên dưới và âm
                    // thầm hiểu "mau_nen" là TÊN TRANG (vị trí) — sai cú
                    // pháp bị nuốt không báo lỗi. "mau_nen" không phải
                    // tên trang hợp lý (đây là tên named option), nên
                    // báo lỗi rõ ràng ngay khi thiếu ':' thay vì đoán mò.
                    return Err(self.error(
                        "Mong đợi ':' sau \"mau_nen\" (named option cần dạng \"mau_nen: <màu>\")".to_string()
                    ));
                }
            }
            // Không phải named option -> coi là tên trang (vị trí, như cũ).
            name = match self.advance().kind {
                TokenKind::StringLit(s) | TokenKind::Identifier(s) => Some(s),
                other => return Err(self.error(format!("Mong đợi tên trang hợp lệ, nhận được {}", other))),
            };
        }

        self.consume(&TokenKind::RParen, "Mong đợi ')' sau khai báo trang")?;
        self.consume(&TokenKind::LBrace, "Mong đợi '{' để mở đầu khối nội dung trang")?;

        let mut states = Vec::new();
        let mut events = Vec::new();
        let mut children = Vec::new();

        while !self.check(&TokenKind::RBrace) && !self.is_at_end() {
            if self.check(&TokenKind::State) {
                let var = self.parse_var_decl()?;
                states.push(StateDecl {
                    name: var.name,
                    value: var.value,
                    pos: var.pos,
                });
            } else if self.check(&TokenKind::OnTai) || self.check(&TokenKind::OnHuy) {
                events.push(self.parse_page_event()?);
            } else {
                children.push(self.parse_child()?);
            }
        }

        self.consume(&TokenKind::RBrace, "Mong đợi '}' để đóng khối trang")?;

        Ok(Page {
            route,
            name,
            mau_nen,
            states,
            events,
            children,
            pos,
        })
    }

    /// Parse sự kiện vòng đời trang: on_tai { ... } hoặc on_huy { ... }
    fn parse_page_event(&mut self) -> Result<vibao_ast::PageEvent, ParseError> {
        let pos = self.current_pos();
        let tok = self.advance();
        let name = match tok.kind {
            TokenKind::OnTai => vibao_ast::PageEventName::OnTai,
            TokenKind::OnHuy => vibao_ast::PageEventName::OnHuy,
            _ => unreachable!(),
        };

        self.consume(&TokenKind::LBrace, "Mong đợi '{' sau tên sự kiện trang")?;
        let mut body = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_at_end() {
            body.push(self.parse_action()?);
        }
        self.consume(&TokenKind::RBrace, "Mong đợi '}' để đóng sự kiện trang")?;

        Ok(vibao_ast::PageEvent { name, body, pos })
    }

    /// Parse định nghĩa thành phần custom: @the TenThanhPhan($param: kieu) { ... }
    fn parse_component_def(&mut self) -> Result<ComponentDef, ParseError> {
        let pos = self.current_pos();
        // Hàm này chỉ được gọi khi is_at_symbol_the() đã xác nhận đúng
        // 2 token liên tiếp: TokenKind::At rồi TokenKind::Identifier("the").
        // Lexer không bao giờ emit 1 token gộp "the" riêng, nên luôn
        // tiêu thụ đúng 2 token này.
        self.advance(); // tiêu thụ @
        self.advance(); // tiêu thụ 'the'

        let name = match self.advance().kind {
            TokenKind::Identifier(s) | TokenKind::Component(s) => s,
            other => return Err(self.error(format!("Mong đợi tên định nghĩa thành phần, nhận được {}", other))),
        };

        let mut params = Vec::new();
        if self.match_token(&TokenKind::LParen) {
            while !self.check(&TokenKind::RParen) && !self.is_at_end() {
                let p_pos = self.current_pos();
                let p_name = match self.advance().kind {
                    TokenKind::Variable(n) | TokenKind::Identifier(n) => n,
                    other => return Err(self.error(format!("Mong đợi tên tham số, nhận được {}", other))),
                };

                let mut data_type = DataType::Any;
                if self.match_token(&TokenKind::Colon) {
                    data_type = match self.advance().kind {
                        TokenKind::Identifier(s) => match s.as_str() {
                            "chuoi" => DataType::Chuoi,
                            "so" => DataType::So,
                            "mau" => DataType::Mau,
                            "bool" => DataType::Bool,
                            "mang" => DataType::Mang,
                            "doi_tuong" => DataType::DoiTuong,
                            "hanh_dong" => DataType::HanhDong,
                            _ => DataType::Any,
                        },
                        _ => DataType::Any,
                    };
                }

                let mut default_value = None;
                if self.match_token(&TokenKind::Equals) {
                    default_value = Some(self.parse_value()?);
                }

                params.push(ParamDef {
                    name: p_name,
                    data_type,
                    default_value,
                    pos: p_pos,
                });
                self.skip_comma();
            }
            self.consume(&TokenKind::RParen, "Mong đợi ')' đóng danh sách tham số")?;
        }

        self.consume(&TokenKind::LBrace, "Mong đợi '{' bắt đầu khối nội dung thành phần")?;
        let mut children = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_at_end() {
            children.push(self.parse_child()?);
        }
        self.consume(&TokenKind::RBrace, "Mong đợi '}' đóng khối nội dung thành phần")?;

        Ok(ComponentDef {
            name,
            params,
            children,
            pos,
        })
    }

    /// Parse một node con bất kỳ nằm trong cây UI layout
    pub(crate) fn parse_child(&mut self) -> Result<Child, ParseError> {
        let pos = self.current_pos();
        if self.check(&TokenKind::Neu) {
            Ok(Child::If(Box::new(self.parse_if_node()?)))
        } else if self.check(&TokenKind::VongLap) {
            Ok(Child::Loop(Box::new(self.parse_loop_node()?)))
        } else if self.check(&TokenKind::TruongHop) {
            Ok(Child::Switch(Box::new(self.parse_switch_node()?)))
        } else if self.check(&TokenKind::State) {
            let var = self.parse_var_decl()?;
            Ok(Child::StateDecl(StateDecl { name: var.name, value: var.value, pos: var.pos }))
        } else if let TokenKind::Variable(_) = &self.current().kind {
            let var = self.parse_var_decl()?;
            Ok(Child::VarDecl(var))
        } else {
            if let TokenKind::Component(tag) = &self.current().kind {
                let tag = tag.clone();
                self.advance();
                let el = self.parse_element_rest(tag, pos)?;
                Ok(Child::Element(el))
            } else if let TokenKind::Identifier(name) = &self.current().kind {
                let name = name.clone();
                self.advance();
                if self.check(&TokenKind::LParen) || self.check(&TokenKind::LBrace) || self.check(&TokenKind::Colon) {
                    let call = self.parse_component_call_rest(name, pos)?;
                    Ok(Child::ComponentCall(call))
                } else {
                    let el = self.parse_element_rest(name, pos)?;
                    Ok(Child::Element(el))
                }
            } else {
                Err(self.error(format!("Cấu trúc không hợp lệ trong cây giao diện: {}", self.current().kind)))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;

    fn parse_page_from_source(src: &str) -> Page {
        let tokens = tokenize(src).unwrap();
        let mut p = Parser::new(tokens);
        p.parse_page().unwrap()
    }

    #[test]
    fn test_parse_page_without_mau_nen_stays_none() {
        let page = parse_page_from_source(r#"trang("/") { }"#);
        assert!(page.mau_nen.is_none());
    }

    #[test]
    fn test_parse_page_with_name_only_still_works() {
        let page = parse_page_from_source(r#"trang("/", "Trang chủ") { }"#);
        assert_eq!(page.name, Some("Trang chủ".to_string()));
        assert!(page.mau_nen.is_none());
    }

    #[test]
    fn test_parse_page_with_mau_nen_color_name() {
        let page = parse_page_from_source(r#"trang("/", mau_nen: xanh) { }"#);
        match page.mau_nen {
            Some(ColorValue::Name(n)) => assert_eq!(n, "xanh"),
            other => panic!("Kỳ vọng ColorValue::Name(\"xanh\"), nhận được {:?}", other),
        }
    }

    #[test]
    fn test_parse_page_with_mau_nen_hex() {
        let page = parse_page_from_source(r#"trang("/", mau_nen: #FF0000) { }"#);
        match page.mau_nen {
            Some(ColorValue::Hex(h)) => assert_eq!(h, "#FF0000"),
            other => panic!("Kỳ vọng ColorValue::Hex, nhận được {:?}", other),
        }
    }

    #[test]
    fn test_parse_page_mau_nen_missing_colon_is_error() {
        // BUG ĐÃ SỬA: trước đây "mau_nen" thiếu dấu ':' bị âm thầm hiểu
        // nhầm thành tên trang (positional name) — không báo lỗi gì.
        // Giờ phải trả về lỗi rõ ràng.
        let tokens = tokenize(r#"trang("/", mau_nen) { }"#).unwrap();
        let mut p = Parser::new(tokens);
        let result = p.parse_page();
        assert!(result.is_err(), "Kỳ vọng lỗi parse khi 'mau_nen' thiếu dấu ':', nhưng parse thành công");
    }

    #[test]
    fn test_parse_page_with_name_and_mau_nen_together() {
        let page = parse_page_from_source(r#"trang("/", "Trang chủ", mau_nen: xam_nhat) { }"#);
        assert_eq!(page.name, Some("Trang chủ".to_string()));
        match page.mau_nen {
            Some(ColorValue::Name(n)) => assert_eq!(n, "xam_nhat"),
            other => panic!("Kỳ vọng ColorValue::Name, nhận được {:?}", other),
        }
    }

    #[test]
    fn test_parse_page_mau_nen_before_name_also_works() {
        // Thứ tự không bắt buộc: mau_nen trước, tên trang sau.
        let page = parse_page_from_source(r#"trang("/", mau_nen: do, "Trang chủ") { }"#);
        assert_eq!(page.name, Some("Trang chủ".to_string()));
        match page.mau_nen {
            Some(ColorValue::Name(n)) => assert_eq!(n, "do"),
            other => panic!("Kỳ vọng ColorValue::Name, nhận được {:?}", other),
        }
    }
}
