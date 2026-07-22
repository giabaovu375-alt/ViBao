// ============================================================
// VIBAO COMPILER (Rust) — codegen/props.rs
// Mở rộng PropsMap (danh sách key:value tiếng Việt trên 1 Element)
// thành style CSS, thuộc tính HTML (attrs) và binding động
// (dynamic). Tương đương expandProps() + các hàm ensurePx/
// expandSpacing/expandRadius/mapAlign*/... của 06-parser-expr.ts
// (phần cuối, dùng riêng cho SIMPLE ELEMENT — khác với layout.rs
// vốn dùng cho LAYOUT ELEMENT).
// ============================================================

use vibao_ast::PropsMap;
use crate::codegen::css::OrderedMap;
use crate::codegen::expr::{expr_to_js_default, resolve_value, ResolvedValue};

/// Kết quả mở rộng props của 1 Element thường (simple element — text,
/// button, image, input...). Giữ 3 nhóm tách biệt vì mỗi nhóm render ra
/// một phần khác nhau của thẻ HTML (style="...", các attr rời, và
/// data-vb-bind-* cho phần cần cập nhật động lúc runtime). Dùng
/// OrderedMap (không phải HashMap) để giữ thứ tự khai báo — khớp cách
/// PropsMap/LayoutCss xử lý thứ tự trong toàn bộ codegen.
#[derive(Debug, Clone, Default)]
pub struct ExpandedProps {
    /// key CSS dạng camelCase (vd "backgroundColor") → giá trị CSS.
    /// Dùng camelCase ở bước trung gian này để khớp style object JS gốc;
    /// css.rs sẽ chuyển sang kebab-case khi in ra chuỗi CSS thật.
    pub style: OrderedMap,
    /// attr HTML thường (vd alt, placeholder, type, value...).
    pub attrs: OrderedMap,
    /// key CSS/attr cần binding động → biểu thức JS để runtime theo dõi.
    pub dynamic: OrderedMap,
    /// Tên prop KHÔNG nằm trong danh sách prop được ViBao định nghĩa
    /// (rơi vào nhánh `other` bên dưới). Vẫn được ghi ra `attrs` như cũ
    /// (không đổi hành vi — cho phép truyền attr HTML tuỳ ý kiểu
    /// `data-*`/`aria-*`), nhưng được thu thập riêng ở đây để nơi gọi
    /// (element.rs) có thể phát cảnh báo build cho các trường hợp khả
    /// nghi là gõ sai tên prop (vd "mua" thay vì "mau").
    pub unknown_keys: Vec<String>,
}

/// Danh sách đầy đủ tên prop mà `expand_props` nhận diện — PHẢI khớp
/// từng nhánh `"..." =>` trong match bên dưới (trừ nhánh `other`).
/// Dùng để phân biệt "prop lạ nhưng có chủ đích" (data-*, aria-*) với
/// "khả năng gõ sai chính tả" khi phát cảnh báo — xem gen_simple_element
/// ở element.rs.
const KNOWN_PROP_KEYS: [&str; 44] = [
    "color", "mau", "mau_chu", "mau_vien", "width", "height", "max_rong",
    "radius", "dem", "le", "vien", "kieu_vien", "bong", "overflow",
    "tang_z", "co", "dam", "nghieng", "gach_chan", "can", "hang",
    "khoang_chu", "bien_doi", "font", "huong", "gap", "doc", "boc",
    "fit", "alt", "lazy", "loai", "chu_tro", "den", "bat_buoc",
    "vo_hieu", "gia_tri", "noi_dung",
    // Prop hoạt hình — xử lý riêng ở animation.rs nhưng vẫn hợp lệ
    // nếu xuất hiện trong PropsMap chung (xem nhánh rỗng tương ứng
    // trong match bên dưới).
    "hieu_ung", "thoi_gian", "tre", "lap", "hieu_ung_hover", "hieu_ung_cuon",
];

/// Mở rộng 1 PropsMap thành style/attrs/dynamic cho SIMPLE ELEMENT.
/// `tag` cần thiết vì 1 vài prop có ý nghĩa khác nhau tuỳ loại thẻ (vd
/// "can" nghĩa là text-align trên thẻ text nhưng justify-content trên
/// thẻ layout — dù layout dùng layout.rs riêng, "can" vẫn xuất hiện ở
/// đây cho các simple element chứa text).
pub fn expand_props(tag: &str, props: &PropsMap) -> ExpandedProps {
    let mut out = ExpandedProps::default();

    for (key, expr) in props {
        let resolved = resolve_value(expr);
        let is_dynamic = resolved.is_dynamic();
        let css_val = resolved.as_css();

        match key.as_str() {
            "color" => {
                if is_dynamic {
                    out.dynamic.insert("backgroundColor".to_string(), expr_to_js_default(expr));
                } else {
                    out.style.insert("backgroundColor".to_string(), css_val);
                }
            }
            "mau" | "mau_chu" => {
                if is_dynamic {
                    out.dynamic.insert("color".to_string(), expr_to_js_default(expr));
                } else {
                    out.style.insert("color".to_string(), css_val);
                }
            }
            "mau_vien" => {
                if is_dynamic {
                    out.dynamic.insert("borderColor".to_string(), expr_to_js_default(expr));
                } else {
                    out.style.insert("borderColor".to_string(), css_val);
                }
            }
            // LƯU Ý: bản TS gốc có bug ở đây — width/height/max_rong không
            // hề kiểm tra isDynamic, nên "width: $bien" luôn sinh ra
            // style["width"] = "px" (rỗng + "px") mà KHÔNG binding động,
            // width bị mất hoàn toàn lúc runtime. Bản Rust này sửa đúng
            // (đưa vào dynamic giống hệt cách xử lý color/mau ở trên) vì
            // person yêu cầu không cần giữ nguyên 100% các chỗ đã sai.
            "width" => {
                if is_dynamic {
                    out.dynamic.insert("width".to_string(), expr_to_js_default(expr));
                } else {
                    out.style.insert("width".to_string(), size_or_px(&resolved, &css_val));
                }
            }
            "height" => {
                if is_dynamic {
                    out.dynamic.insert("height".to_string(), expr_to_js_default(expr));
                } else {
                    out.style.insert("height".to_string(), size_or_px(&resolved, &css_val));
                }
            }
            "max_rong" => {
                if is_dynamic {
                    out.dynamic.insert("maxWidth".to_string(), expr_to_js_default(expr));
                } else {
                    out.style.insert("maxWidth".to_string(), size_or_px(&resolved, &css_val));
                }
            }
            "radius" => {
                out.style.insert("borderRadius".to_string(), expand_radius(&css_val));
            }
            "dem" => {
                out.style.insert("padding".to_string(), expand_spacing(&css_val));
            }
            "le" => {
                out.style.insert("margin".to_string(), expand_spacing(&css_val));
            }
            "vien" => {
                out.style.insert("borderWidth".to_string(), ensure_px(&css_val));
                out.style.entry_or_insert_with("borderStyle", || "solid".to_string());
            }
            "kieu_vien" => {
                out.style.insert("borderStyle".to_string(), css_val);
            }
            "bong" => {
                out.style.insert("boxShadow".to_string(), css_val);
            }
            "overflow" => {
                out.style.insert("overflow".to_string(), css_val);
            }
            "tang_z" => {
                out.style.insert("zIndex".to_string(), css_val);
            }
            "co" => {
                out.style.insert("fontSize".to_string(), ensure_px(&css_val));
            }
            "dam" => {
                if css_val == "true" {
                    out.style.insert("fontWeight".to_string(), "bold".to_string());
                }
            }
            "nghieng" => {
                if css_val == "true" {
                    out.style.insert("fontStyle".to_string(), "italic".to_string());
                }
            }
            "gach_chan" => {
                if css_val == "true" {
                    out.style.insert("textDecoration".to_string(), "underline".to_string());
                }
            }
            "can" => {
                // TEXT_TAGS ở bản TS gốc là 1 Set 6 phần tử bao gồm cả
                // "nhan" — giữ đúng cấu trúc đó (thay vì OR riêng) để dễ
                // đối chiếu và mở rộng sau này.
                const TEXT_TAGS: [&str; 6] = ["text", "h1", "h2", "h3", "p", "nhan"];
                if TEXT_TAGS.contains(&tag) {
                    out.style.insert("textAlign".to_string(), map_align(&css_val));
                } else {
                    out.style.insert("justifyContent".to_string(), map_justify(&css_val));
                }
            }
            "hang" => {
                out.style.insert("lineHeight".to_string(), css_val);
            }
            "khoang_chu" => {
                out.style.insert("letterSpacing".to_string(), ensure_px(&css_val));
            }
            "bien_doi" => {
                out.style.insert("textTransform".to_string(), css_val);
            }
            "font" => {
                out.style.insert("fontFamily".to_string(), css_val);
            }
            "huong" => {
                out.style.insert(
                    "flexDirection".to_string(),
                    if css_val == "column" { "column".to_string() } else { "row".to_string() },
                );
            }
            "gap" => {
                out.style.insert("gap".to_string(), ensure_px(&css_val));
            }
            "doc" => {
                out.style.insert("alignItems".to_string(), map_align_items(&css_val));
            }
            "boc" => {
                if css_val == "true" {
                    out.style.insert("flexWrap".to_string(), "wrap".to_string());
                }
            }
            "fit" => {
                out.style.insert("objectFit".to_string(), css_val);
            }
            "alt" => {
                out.attrs.insert("alt".to_string(), css_val);
            }
            "lazy" => {
                if css_val == "true" {
                    out.attrs.insert("loading".to_string(), "lazy".to_string());
                }
            }
            "loai" => {
                out.attrs.insert("type".to_string(), css_val);
            }
            "chu_tro" => {
                out.attrs.insert("placeholder".to_string(), css_val);
            }
            "den" => {
                // Prop riêng cho link/lien_ket: "đến" — đích điều hướng.
                // Sinh ra data-vb-link để router.rs (runtime) tự chặn
                // click và điều hướng SPA thật (History API, không reload)
                // — xem runtime/router.rs::setup_link_interception.
                // Đồng thời set luôn href thường để: (1) hiện đúng URL khi
                // hover chuột vào link, (2) vẫn hoạt động nếu JS/WASM lỗi
                // hoặc bị tắt (progressive enhancement — click vẫn ra đúng
                // trang, chỉ là full reload thay vì SPA navigate).
                //
                // GIỚI HẠN: chỉ hoạt động đúng với giá trị TĨNH (string
                // literal hoặc biến CSS var qua ResolvedValue::Static/Size)
                // — nếu "den" là 1 biểu thức ĐỘNG (vd $route_hien_tai),
                // as_css() trả về chuỗi rỗng (xem ResolvedValue::Dynamic),
                // khiến href/data-vb-link rỗng. Route động cho link cần
                // 1 cơ chế binding riêng (tương tự data-vb-attr-*), chưa
                // triển khai — dùng button + dieu_huong() nếu cần đích
                // điều hướng phụ thuộc state.
                out.attrs.insert("href".to_string(), css_val.clone());
                out.attrs.insert("data-vb-link".to_string(), css_val);
            }
            "bat_buoc" => {
                if css_val == "true" {
                    out.attrs.insert("required".to_string(), "true".to_string());
                }
            }
            "vo_hieu" => {
                if css_val == "true" {
                    out.attrs.insert("disabled".to_string(), "true".to_string());
                }
            }
            "gia_tri" => {
                out.attrs.insert("value".to_string(), css_val);
            }
            // Các prop hoạt hình được xử lý riêng ở animation.rs (đọc
            // trực tiếp từ AnimationProps trên Element, không qua PropsMap
            // chung này) — bỏ qua ở đây để không sinh nhầm attr thừa.
            "hieu_ung" | "thoi_gian" | "tre" | "lap" | "hieu_ung_hover" | "hieu_ung_cuon" => {}
            "noi_dung" => {
                if is_dynamic {
                    out.dynamic.insert("noi_dung".to_string(), expr_to_js_default(expr));
                } else {
                    out.attrs.insert("noi_dung".to_string(), css_val);
                }
            }
            other => {
                if !KNOWN_PROP_KEYS.contains(&other) {
                    out.unknown_keys.push(other.to_string());
                }
                if is_dynamic {
                    out.dynamic.insert(other.to_string(), expr_to_js_default(expr));
                } else {
                    out.attrs.insert(other.to_string(), css_val);
                }
            }
        }
    }

    out
}

/// Với prop kích thước (width/height/max_rong), nếu ResolvedValue đã là
/// Size (có đơn vị CSS sẵn từ literal) thì dùng nguyên; ngược lại (chuỗi
/// tĩnh không phải số, hiếm gặp) thì ép thêm "px" — khớp `resolved.kind
/// === "size" ? resolved.css : cssVal + "px"` ở bản TS cũ.
fn size_or_px(resolved: &ResolvedValue, css_val: &str) -> String {
    match resolved {
        ResolvedValue::Size(s) => s.clone(),
        _ => format!("{}px", css_val),
    }
}

// ════════════════════════════════════════════════════════════
// GIÁ TRỊ CSS TIỆN ÍCH (ensurePx, spacing, radius, align maps)
// ════════════════════════════════════════════════════════════

/// Thêm hậu tố "px" cho 1 giá trị số thuần nếu nó chưa có đơn vị nào —
/// tương đương ensurePx() ở bản TS cũ. Chuỗi rỗng trả về "0px".
pub fn ensure_px(val: &str) -> String {
    if val.is_empty() {
        return "0px".to_string();
    }
    if is_plain_number(val) {
        format!("{}px", val)
    } else {
        val.to_string()
    }
}

/// Áp ensure_px() cho từng phần tách bởi khoảng trắng — dùng cho các
/// prop có thể nhận nhiều giá trị viền/lề kiểu CSS shorthand (vd "dem"
/// nhận "16 24" nghĩa là padding: 16px 24px).
pub fn expand_spacing(val: &str) -> String {
    if val.is_empty() {
        return "0".to_string();
    }
    val.split_whitespace().map(ensure_px).collect::<Vec<_>>().join(" ")
}

/// expandRadius dùng chung logic với expand_spacing ở bản TS cũ (chỉ là
/// alias) — giữ nguyên tên hàm riêng để khớp ngữ nghĩa gọi ở nơi dùng.
pub fn expand_radius(val: &str) -> String {
    expand_spacing(val)
}

/// Kiểm tra khớp đúng regex gốc `/^-?[\d.]+$/`: tối đa 1 dấu "-" và CHỈ
/// được ở vị trí đầu tiên, phần còn lại toàn digit/dấu chấm. (Bug đã sửa
/// so với bản nháp đầu: dùng .all() đơn thuần sẽ cho phép "-" ở giữa
/// chuỗi như "1-2", không khớp ý nghĩa regex gốc.)
fn is_plain_number(val: &str) -> bool {
    if val.is_empty() {
        return false;
    }
    let rest = val.strip_prefix('-').unwrap_or(val);
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit() || c == '.')
}

/// Ánh xạ tên căn lề tiếng Việt (dùng cho text-align) sang giá trị CSS.
pub fn map_align(val: &str) -> String {
    match val {
        "trai" => "left".to_string(),
        "phai" => "right".to_string(),
        "giua" => "center".to_string(),
        "deu" => "justify".to_string(),
        other => other.to_string(),
    }
}

/// Ánh xạ tên căn chỉnh sang justify-content — dùng khi "can" xuất hiện
/// trên thẻ không phải văn bản (container chứa nhiều con).
pub fn map_justify(val: &str) -> String {
    match val {
        "start" => "flex-start".to_string(),
        "end" => "flex-end".to_string(),
        "center" => "center".to_string(),
        "space-between" => "space-between".to_string(),
        "space-around" => "space-around".to_string(),
        other => other.to_string(),
    }
}

/// Ánh xạ giá trị "doc" (align-items) sang CSS.
pub fn map_align_items(val: &str) -> String {
    match val {
        "start" => "flex-start".to_string(),
        "end" => "flex-end".to_string(),
        "center" => "center".to_string(),
        "stretch" => "stretch".to_string(),
        other => other.to_string(),
    }
}

// ════════════════════════════════════════════════════════════
// UNIT TESTS
// ════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use vibao_ast::{Expr, Pos};

    fn p() -> Pos {
        Pos { line: 1, column: 1 }
    }

    #[test]
    fn test_color_prop_static() {
        let props: PropsMap = vec![("mau".to_string(), Expr::Literal(vibao_ast::LiteralValue::Color("#FF0000".to_string()), p()))];
        let out = expand_props("text", &props);
        assert_eq!(out.style.get("color"), Some(&"#FF0000".to_string()));
    }

    #[test]
    fn test_color_prop_dynamic_variable() {
        let props: PropsMap = vec![("mau".to_string(), Expr::Variable("mau_chinh".to_string(), p()))];
        let out = expand_props("text", &props);
        assert_eq!(out.dynamic.get("color"), Some(&"__s.mau_chinh".to_string()));
        assert!(!out.style.contains_key("color"));
    }

    #[test]
    fn test_width_uses_number_unit() {
        let props: PropsMap = vec![("width".to_string(), Expr::literal_num(200.0, p()))];
        let out = expand_props("box", &props);
        assert_eq!(out.style.get("width"), Some(&"200px".to_string()));
    }

    #[test]
    fn test_width_dynamic_variable_goes_to_dynamic_not_broken_style() {
        // Regression test cho bug đã sửa: bản TS gốc không xử lý width
        // dynamic, luôn sinh style["width"] = "px" (hỏng). Bản Rust này
        // phải đưa vào dynamic giống color/mau, không được set style rác.
        let props: PropsMap = vec![("width".to_string(), Expr::Variable("w".to_string(), p()))];
        let out = expand_props("box", &props);
        assert_eq!(out.dynamic.get("width"), Some(&"__s.w".to_string()));
        assert!(!out.style.contains_key("width"));
    }

    #[test]
    fn test_unknown_prop_key_collected_for_warning() {
        // BUG ĐÃ SỬA: trước đây prop gõ sai tên (vd "mua" thay vì "mau")
        // bị nuốt âm thầm thành attr HTML tuỳ ý, không có cách nào biết
        // được là gõ sai. Giờ unknown_keys phải chứa tên prop lạ.
        let props: PropsMap = vec![("mua".to_string(), Expr::Literal(vibao_ast::LiteralValue::Str("do".to_string()), p()))];
        let out = expand_props("text", &props);
        assert_eq!(out.unknown_keys, vec!["mua".to_string()]);
        // Hành vi cũ (passthrough vào attrs) vẫn giữ nguyên — không phá vỡ.
        assert_eq!(out.attrs.get("mua"), Some(&"do".to_string()));
    }

    #[test]
    fn test_known_prop_key_not_flagged_as_unknown() {
        let props: PropsMap = vec![("mau".to_string(), Expr::Literal(vibao_ast::LiteralValue::Color("#FF0000".to_string()), p()))];
        let out = expand_props("text", &props);
        assert!(out.unknown_keys.is_empty());
    }

    #[test]
    fn test_multiple_unknown_keys_all_collected() {
        let props: PropsMap = vec![
            ("mua".to_string(), Expr::Literal(vibao_ast::LiteralValue::Str("x".to_string()), p())),
            ("cann".to_string(), Expr::Literal(vibao_ast::LiteralValue::Str("y".to_string()), p())),
        ];
        let out = expand_props("text", &props);
        assert_eq!(out.unknown_keys.len(), 2);
        assert!(out.unknown_keys.contains(&"mua".to_string()));
        assert!(out.unknown_keys.contains(&"cann".to_string()));
    }


    #[test]
    fn test_width_percent_unit_preserved() {
        let props: PropsMap = vec![(
            "width".to_string(),
            Expr::literal_num_with_unit(50.0, Some("%".to_string()), p()),
        )];
        let out = expand_props("box", &props);
        assert_eq!(out.style.get("width"), Some(&"50%".to_string()));
    }

    #[test]
    fn test_bold_flag_true() {
        let props: PropsMap = vec![("dam".to_string(), Expr::literal_bool(true, p()))];
        let out = expand_props("text", &props);
        assert_eq!(out.style.get("fontWeight"), Some(&"bold".to_string()));
    }

    #[test]
    fn test_bold_flag_false_not_set() {
        let props: PropsMap = vec![("dam".to_string(), Expr::literal_bool(false, p()))];
        let out = expand_props("text", &props);
        assert!(!out.style.contains_key("fontWeight"));
    }

    #[test]
    fn test_can_text_align_on_text_tag() {
        let props: PropsMap = vec![("can".to_string(), Expr::literal_str("giua", p()))];
        let out = expand_props("text", &props);
        assert_eq!(out.style.get("textAlign"), Some(&"center".to_string()));
    }

    #[test]
    fn test_can_justify_content_on_non_text_tag() {
        let props: PropsMap = vec![("can".to_string(), Expr::literal_str("center", p()))];
        let out = expand_props("button", &props);
        assert_eq!(out.style.get("justifyContent"), Some(&"center".to_string()));
    }

    #[test]
    fn test_unknown_prop_goes_to_attrs() {
        let props: PropsMap = vec![("data_custom".to_string(), Expr::literal_str("xyz", p()))];
        let out = expand_props("box", &props);
        assert_eq!(out.attrs.get("data_custom"), Some(&"xyz".to_string()));
    }

    #[test]
    fn test_den_prop_emits_href_and_data_vb_link() {
        let props: PropsMap = vec![("den".to_string(), Expr::literal_str("/gioi-thieu", p()))];
        let out = expand_props("link", &props);
        assert_eq!(out.attrs.get("href"), Some(&"/gioi-thieu".to_string()));
        assert_eq!(out.attrs.get("data-vb-link"), Some(&"/gioi-thieu".to_string()));
    }

    #[test]
    fn test_animation_props_skipped() {
        let props: PropsMap = vec![("hieu_ung".to_string(), Expr::literal_str("fade_in", p()))];
        let out = expand_props("box", &props);
        assert!(out.attrs.is_empty());
        assert!(out.style.is_empty());
        assert!(out.dynamic.is_empty());
    }

    #[test]
    fn test_ensure_px_plain_number() {
        assert_eq!(ensure_px("16"), "16px");
        assert_eq!(ensure_px(""), "0px");
        assert_eq!(ensure_px("50%"), "50%");
    }

    #[test]
    fn test_ensure_px_rejects_dash_in_middle() {
        // Regression test: is_plain_number từng cho phép "-" ở BẤT KỲ vị
        // trí nào (bug), trong khi regex gốc /^-?[\d.]+$/ chỉ cho phép nó
        // ở đầu chuỗi. "1-2" không phải số hợp lệ, phải giữ nguyên chuỗi.
        assert_eq!(ensure_px("1-2"), "1-2");
        assert_eq!(ensure_px("-16"), "-16px");
    }

    #[test]
    fn test_expand_spacing_multiple_values() {
        assert_eq!(expand_spacing("16 24"), "16px 24px");
    }
}
