// ============================================================
// VIBAO COMPILER (Rust) — lexer/tables.rs
// KEYWORD & COLOR TABLES: bảng tra từ khóa, component, tên màu
// ============================================================

use super::token::TokenKind;
use std::collections::HashMap;

// ════════════════════════════════════════════════════════════
// 2. KEYWORD & COLOR TABLES
// ════════════════════════════════════════════════════════════

pub(crate) fn keyword_map() -> HashMap<&'static str, TokenKind> {
    let mut m = HashMap::new();
    m.insert("trang", TokenKind::Trang);
    m.insert("ung_dung", TokenKind::UngDung);
    m.insert("layout", TokenKind::Layout);
    m.insert("theme", TokenKind::Theme);
    m.insert("state", TokenKind::State);

    m.insert("neu", TokenKind::Neu);
    m.insert("khong_thi", TokenKind::KhongThi);
    m.insert("neu_nhieu", TokenKind::NeuNhieu);
    m.insert("truong_hop", TokenKind::TruongHop);
    m.insert("mac_dinh", TokenKind::MacDinh);
    m.insert("vong_lap", TokenKind::VongLap);

    m.insert("on_click", TokenKind::OnClick);
    m.insert("on_hover", TokenKind::OnHover);
    m.insert("on_blur", TokenKind::OnBlur);
    m.insert("on_focus", TokenKind::OnFocus);
    m.insert("on_change", TokenKind::OnChange);
    m.insert("on_submit", TokenKind::OnSubmit);
    m.insert("on_scroll", TokenKind::OnScroll);
    m.insert("on_tai", TokenKind::OnTai);
    m.insert("on_huy", TokenKind::OnHuy);
    m
}

pub(crate) fn component_set() -> Vec<&'static str> {
    vec![
        // Text
        "text", "h1", "h2", "h3", "p", "nhan",
        // Media
        "image", "video", "icon",
        // Interactive
        "button", "input", "link", "lien_ket",
        // Layout
        "flex", "grid", "stack", "box", "scroll", "container", "layer",
        "dinh_dau", "dinh_man_hinh",
        // Spacing
        "spacer", "divider",
        // Form
        "form", "nhom_input", "chon_mot", "hop_kiem", "lua_chon",
        // UI phức tạp
        "modal", "tabs", "accordion", "carousel", "xuong_trang",
        "vong_quay", "thanh_tien_trinh", "bang", "bieu_do", "ban_do",
        "thanh_nav", "trinh_soan_thao",
        // Feedback / actions (dùng cả như "hàm hành động" trong event)
        "thong_bao", "canh_bao", "dieu_huong", "mo_tab_moi",
        "mo_modal", "dong_modal", "cuon_den", "cuon_len_dau",
        "luu_du_lieu", "tai_du_lieu", "dang_xuat", "sao_chep",
        "gia_tien", "ngay", "rut_gon", "hoa_chu", "phan_tram",
        "lam_tron", "goi_api",
    ]
}

pub(crate) fn color_map() -> HashMap<&'static str, &'static str> {
    let mut m = HashMap::new();
    m.insert("trang", "#FFFFFF");
    m.insert("den", "#000000");
    m.insert("do", "#E53E3E");
    m.insert("xanh", "#3182CE");
    m.insert("xanh_la", "#38A169");
    m.insert("vang", "#F59E0B");
    m.insert("hong", "#D53F8C");
    m.insert("tim", "#805AD5");
    m.insert("cam", "#DD6B20");
    m.insert("xam", "#718096");
    m.insert("xam_nhat", "#F7FAFC");
    m.insert("xam_dam", "#2D3748");
    m.insert("luc", "#25855A");
    m.insert("nau", "#7B341E");
    m
}

/// Trả về mã hex cho 1 tên màu ViBao, dùng ở codegen sau này.
pub fn resolve_color_name(name: &str) -> String {
    color_map()
        .get(name)
        .map(|s| s.to_string())
        .unwrap_or_else(|| name.to_string())
}
