// ============================================================
// VIBAO COMPILER (Rust) — codegen/layout.rs
// Sinh CSS cho các LAYOUT ELEMENT (flex, grid, box, stack, scroll,
// container, layer, dinh_dau, dinh_man_hinh) và cho các chỉ thị
// responsive (@di_dong, @may_tinh_bang, @may_tinh). Tương đương
// 07-parser-layout.ts của bản TS cũ.
// ============================================================

use vibao_ast::{Breakpoint, PropsMap, ResponsiveNode};
use crate::codegen::css::OrderedMap;
use crate::codegen::expr::get_static_value;

/// Tập hợp các tag được coi là "layout element" — dùng ở element.rs để
/// quyết định gọi resolve_layout_css() (module này) hay expand_props()
/// (props.rs) cho 1 Element cụ thể.
pub const LAYOUT_TAGS: [&str; 9] = [
    "flex",
    "grid",
    "box",
    "stack",
    "scroll",
    "container",
    "layer",
    "dinh_dau",
    "dinh_man_hinh",
];

pub fn is_layout_tag(tag: &str) -> bool {
    LAYOUT_TAGS.contains(&tag)
}

/// CSS đã resolve cho 1 layout element — dùng OrderedMap tự viết (thay
/// vì HashMap) để GIỮ THỨ TỰ khai báo property, quan trọng khi in ra
/// chuỗi CSS thật (một vài property phụ thuộc thứ tự, vd border-style
/// phải đứng sau border-width nếu cùng ghi đè "border"). Khớp PropsMap
/// trong ast.rs vốn cũng dùng Vec thay HashMap vì cùng lý do. Không dùng
/// crate ngoài (indexmap) để giữ đúng triết lý "không phụ thuộc mạng
/// lúc build" đã ghi trong Cargo.toml/README của dự án.
pub type LayoutCss = OrderedMap;

/// Phân phối theo tag để tính CSS layout tương ứng — tương đương
/// resolveLayoutCSS() ở bản TS cũ.
pub fn resolve_layout_css(tag: &str, props: &PropsMap) -> LayoutCss {
    match tag {
        "flex" => resolve_flex(props),
        "grid" => resolve_grid(props),
        "box" => resolve_box(props),
        "stack" => resolve_stack(props),
        "scroll" => resolve_scroll(props),
        "container" => resolve_container(props),
        "layer" => resolve_layer(),
        "dinh_dau" => resolve_sticky_top(props),
        "dinh_man_hinh" => resolve_fixed(props),
        _ => {
            let mut css = LayoutCss::new();
            css.insert("display".to_string(), "block".to_string());
            css
        }
    }
}

fn resolve_flex(props: &PropsMap) -> LayoutCss {
    let mut css = LayoutCss::new();
    css.insert("display".to_string(), "flex".to_string());
    for (key, expr) in props {
        let v = get_static_value(expr);
        match key.as_str() {
            "huong" => {
                css.insert(
                    "flexDirection".to_string(),
                    if v == "column" { "column".to_string() } else { "row".to_string() },
                );
            }
            "gap" => {
                css.insert("gap".to_string(), px(&v));
            }
            "gap_doc" => {
                css.insert("rowGap".to_string(), px(&v));
            }
            "gap_ngang" => {
                css.insert("columnGap".to_string(), px(&v));
            }
            "can" => {
                css.insert("justifyContent".to_string(), map_justify(&v));
            }
            "doc" => {
                css.insert("alignItems".to_string(), map_align_items(&v));
            }
            "boc" => {
                if v == "true" {
                    css.insert("flexWrap".to_string(), "wrap".to_string());
                }
            }
            "width" => {
                css.insert("width".to_string(), size(&v));
            }
            "height" => {
                css.insert("height".to_string(), size(&v));
            }
            "dem" => {
                css.insert("padding".to_string(), spacing(&v));
            }
            "color" => {
                css.insert("backgroundColor".to_string(), v);
            }
            "radius" => {
                css.insert("borderRadius".to_string(), radius(&v));
            }
            _ => {}
        }
    }
    css
}

fn resolve_grid(props: &PropsMap) -> LayoutCss {
    let mut css = LayoutCss::new();
    css.insert("display".to_string(), "grid".to_string());
    for (key, expr) in props {
        let v = get_static_value(expr);
        match key.as_str() {
            "cot" => {
                css.insert("gridTemplateColumns".to_string(), repeat_or_raw(&v));
            }
            "hang_luoi" => {
                css.insert("gridTemplateRows".to_string(), repeat_or_raw(&v));
            }
            "gap" => {
                css.insert("gap".to_string(), px(&v));
            }
            "gap_doc" => {
                css.insert("rowGap".to_string(), px(&v));
            }
            "gap_ngang" => {
                css.insert("columnGap".to_string(), px(&v));
            }
            "width" => {
                css.insert("width".to_string(), size(&v));
            }
            "dem" => {
                css.insert("padding".to_string(), spacing(&v));
            }
            "color" => {
                css.insert("backgroundColor".to_string(), v);
            }
            _ => {}
        }
    }
    css
}

fn resolve_box(props: &PropsMap) -> LayoutCss {
    let mut css = LayoutCss::new();
    css.insert("display".to_string(), "block".to_string());
    for (key, expr) in props {
        let v = get_static_value(expr);
        match key.as_str() {
            "color" => {
                css.insert("backgroundColor".to_string(), v);
            }
            "width" => {
                css.insert("width".to_string(), size(&v));
            }
            "height" => {
                css.insert("height".to_string(), size(&v));
            }
            "min_rong" => {
                css.insert("minWidth".to_string(), size(&v));
            }
            "max_rong" => {
                css.insert("maxWidth".to_string(), size(&v));
            }
            "min_cao" => {
                css.insert("minHeight".to_string(), size(&v));
            }
            "max_cao" => {
                css.insert("maxHeight".to_string(), size(&v));
            }
            "radius" => {
                css.insert("borderRadius".to_string(), radius(&v));
            }
            "dem" => {
                css.insert("padding".to_string(), spacing(&v));
            }
            "le" => {
                css.insert("margin".to_string(), spacing(&v));
            }
            "vien" => {
                css.insert("border".to_string(), border(&v, props));
            }
            "bong" => {
                css.insert("boxShadow".to_string(), v);
            }
            "overflow" => {
                css.insert("overflow".to_string(), v);
            }
            "tran_x" => {
                css.insert("transform".to_string(), format!("translateX({})", px(&v)));
            }
            "tran_y" => {
                css.insert("transform".to_string(), format!("translateY({})", px(&v)));
            }
            "tang_z" => {
                css.insert("zIndex".to_string(), v);
            }
            _ => {}
        }
    }
    css
}

fn resolve_stack(props: &PropsMap) -> LayoutCss {
    let mut css = LayoutCss::new();
    css.insert("display".to_string(), "grid".to_string());
    css.insert("gridTemplateColumns".to_string(), "1fr".to_string());
    css.insert("gridTemplateRows".to_string(), "1fr".to_string());
    for (key, expr) in props {
        let v = get_static_value(expr);
        match key.as_str() {
            "can" => {
                css.insert("justifyItems".to_string(), map_justify(&v));
            }
            "doc" => {
                css.insert("alignItems".to_string(), map_align_items(&v));
            }
            "width" => {
                css.insert("width".to_string(), size(&v));
            }
            "height" => {
                css.insert("height".to_string(), size(&v));
            }
            _ => {}
        }
    }
    css
}

fn resolve_scroll(props: &PropsMap) -> LayoutCss {
    let mut css = LayoutCss::new();
    css.insert("display".to_string(), "block".to_string());
    css.insert("overflow".to_string(), "auto".to_string());
    for (key, expr) in props {
        let v = get_static_value(expr);
        match key.as_str() {
            "huong" => {
                css.insert("overflow".to_string(), "hidden".to_string());
                css.insert(
                    "overflowX".to_string(),
                    if v == "ngang" { "auto".to_string() } else { "hidden".to_string() },
                );
                css.insert(
                    "overflowY".to_string(),
                    if v == "doc" { "auto".to_string() } else { "hidden".to_string() },
                );
            }
            "height" => {
                css.insert("height".to_string(), size(&v));
            }
            "width" => {
                css.insert("width".to_string(), size(&v));
            }
            _ => {}
        }
    }
    css
}

fn resolve_container(props: &PropsMap) -> LayoutCss {
    let mut css = LayoutCss::new();
    css.insert("display".to_string(), "block".to_string());
    css.insert("width".to_string(), "100%".to_string());
    css.insert("marginLeft".to_string(), "auto".to_string());
    css.insert("marginRight".to_string(), "auto".to_string());
    for (key, expr) in props {
        let v = get_static_value(expr);
        match key.as_str() {
            "max_rong" => {
                css.insert("maxWidth".to_string(), size(&v));
            }
            "dem" => {
                css.insert("padding".to_string(), spacing(&v));
            }
            _ => {}
        }
    }
    css
}

fn resolve_layer() -> LayoutCss {
    let mut css = LayoutCss::new();
    css.insert("display".to_string(), "block".to_string());
    css.insert("position".to_string(), "relative".to_string());
    css.insert("width".to_string(), "100%".to_string());
    css.insert("height".to_string(), "100%".to_string());
    css
}

fn resolve_sticky_top(props: &PropsMap) -> LayoutCss {
    let mut css = LayoutCss::new();
    css.insert("display".to_string(), "block".to_string());
    css.insert("position".to_string(), "sticky".to_string());
    css.insert("top".to_string(), "0".to_string());
    css.insert("zIndex".to_string(), "100".to_string());
    for (key, expr) in props {
        let v = get_static_value(expr);
        if key == "offset" {
            css.insert("top".to_string(), px(&v));
        }
    }
    css
}

fn resolve_fixed(props: &PropsMap) -> LayoutCss {
    let mut css = LayoutCss::new();
    css.insert("display".to_string(), "block".to_string());
    css.insert("position".to_string(), "fixed".to_string());
    css.insert("zIndex".to_string(), "200".to_string());
    for (key, expr) in props {
        let v = get_static_value(expr);
        match key.as_str() {
            "vi_tri" => match v.as_str() {
                "tren" => {
                    css.insert("top".to_string(), "0".to_string());
                    css.insert("left".to_string(), "0".to_string());
                    css.insert("right".to_string(), "0".to_string());
                }
                "duoi" => {
                    css.insert("bottom".to_string(), "0".to_string());
                    css.insert("left".to_string(), "0".to_string());
                    css.insert("right".to_string(), "0".to_string());
                }
                "trai" => {
                    css.insert("top".to_string(), "0".to_string());
                    css.insert("left".to_string(), "0".to_string());
                    css.insert("bottom".to_string(), "0".to_string());
                }
                "phai" => {
                    css.insert("top".to_string(), "0".to_string());
                    css.insert("right".to_string(), "0".to_string());
                    css.insert("bottom".to_string(), "0".to_string());
                }
                _ => {}
            },
            "width" => {
                css.insert("width".to_string(), size(&v));
            }
            "height" => {
                css.insert("height".to_string(), size(&v));
            }
            _ => {}
        }
    }
    css
}

// ════════════════════════════════════════════════════════════
// RESPONSIVE (@di_dong, @may_tinh_bang, @may_tinh)
// ════════════════════════════════════════════════════════════

/// Media query CSS cho 1 breakpoint sau khi đã resolve — selector kèm
/// điều kiện @media và danh sách override property→value.
pub struct ResponsiveCss {
    pub media_condition: String,
    pub overrides: LayoutCss,
}

fn breakpoint_media_condition(bp: Breakpoint) -> &'static str {
    match bp {
        Breakpoint::DiDong => "(max-width: 639px)",
        Breakpoint::MayTinhBang => "(min-width: 640px) and (max-width: 1023px)",
        Breakpoint::MayTinh => "(min-width: 1024px)",
    }
}

/// Resolve danh sách ResponsiveNode (đã parse từ @di_dong { ... } etc.)
/// thành CSS override cho từng breakpoint. Tương đương
/// resolveResponsiveCSS() ở bản TS cũ.
pub fn resolve_responsive_css(_tag: &str, responsive: &[ResponsiveNode]) -> Vec<ResponsiveCss> {
    responsive
        .iter()
        .map(|r| {
            let mut overrides = LayoutCss::new();
            for (key, expr) in &r.overrides {
                let v = get_static_value(expr);
                match key.as_str() {
                    "cot" => {
                        overrides.insert("grid-template-columns".to_string(), repeat_or_raw(&v));
                    }
                    "huong" => {
                        overrides.insert(
                            "flex-direction".to_string(),
                            if v == "column" { "column".to_string() } else { "row".to_string() },
                        );
                    }
                    "co" => {
                        overrides.insert("font-size".to_string(), px(&v));
                    }
                    "width" => {
                        overrides.insert("width".to_string(), size(&v));
                    }
                    "height" => {
                        overrides.insert("height".to_string(), size(&v));
                    }
                    "dem" => {
                        overrides.insert("padding".to_string(), spacing(&v));
                    }
                    "an" => {
                        if v == "true" {
                            overrides.insert("display".to_string(), "none".to_string());
                        }
                    }
                    other => {
                        overrides.insert(crate::codegen::css::camel_to_kebab(other), v);
                    }
                }
            }
            ResponsiveCss {
                media_condition: breakpoint_media_condition(r.breakpoint).to_string(),
                overrides,
            }
        })
        .collect()
}

/// Build 1 khối @media hoàn chỉnh cho 1 selector — trả về chuỗi rỗng nếu
/// không có override nào (tương đương check `Object.keys(overrides).length
/// === 0` ở bản TS cũ, để caller có thể lọc bỏ khối rỗng).
pub fn build_media_query(selector: &str, bp_css: &ResponsiveCss) -> String {
    if bp_css.overrides.is_empty() {
        return String::new();
    }
    let rules = bp_css
        .overrides
        .iter()
        .map(|(k, v)| format!("    {}: {};", k, v))
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        "@media {} {{\n  {} {{\n{}\n  }}\n}}",
        bp_css.media_condition, selector, rules
    )
}

// ════════════════════════════════════════════════════════════
// CSS VALUE HELPERS (px, size, spacing, radius, border, align maps)
// ════════════════════════════════════════════════════════════

/// Nếu chuỗi là "__dynamic__" (sentinel từ get_static_value) hoặc rỗng,
/// trả về nguyên trạng — layout CSS không hỗ trợ binding động cho các
/// prop layout (khác với props.rs, layout tag hiếm khi cần thay đổi
/// runtime); nếu là số thuần thì thêm "px".
pub fn px(val: &str) -> String {
    if val.is_empty() || val == "__dynamic__" {
        return val.to_string();
    }
    if is_plain_number(val) {
        format!("{}px", val)
    } else {
        val.to_string()
    }
}

/// Giống px() nhưng chỉ chấp nhận số không âm không dấu chấm phần thập
/// phân dùng riêng cho size — thực chất bản TS cũ dùng chung 1 regex
/// `/^[\d.]+$/` (không cho phép dấu trừ) khác với px() cho phép "-16px".
pub fn size(val: &str) -> String {
    if val.is_empty() || val == "__dynamic__" {
        return val.to_string();
    }
    let is_unsigned_number = !val.is_empty() && val.chars().all(|c| c.is_ascii_digit() || c == '.');
    if is_unsigned_number {
        format!("{}px", val)
    } else {
        val.to_string()
    }
}

pub fn spacing(val: &str) -> String {
    if val.is_empty() || val == "__dynamic__" {
        return val.to_string();
    }
    val.split_whitespace().map(px).collect::<Vec<_>>().join(" ")
}

pub fn radius(val: &str) -> String {
    spacing(val)
}

fn border(val: &str, props: &PropsMap) -> String {
    let width = px(val);
    let style = props
        .iter()
        .find(|(k, _)| k == "kieu_vien")
        .map(|(_, e)| get_static_value(e))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "solid".to_string());
    let color = props
        .iter()
        .find(|(k, _)| k == "mau_vien")
        .map(|(_, e)| get_static_value(e))
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "#000".to_string());
    format!("{} {} {}", width, style, color)
}

fn repeat_or_raw(val: &str) -> String {
    if !val.is_empty() && val.chars().all(|c| c.is_ascii_digit()) {
        format!("repeat({}, 1fr)", val)
    } else {
        val.to_string()
    }
}

/// Kiểm tra khớp đúng regex gốc của px() ở bản TS cũ: `/^-?[\d.]+$/` —
/// tối đa 1 dấu "-" và CHỈ ở vị trí đầu. (Cùng bug đã sửa như trong
/// codegen/props.rs — .all() đơn thuần cho phép "-" ở giữa chuỗi, sai
/// so với ý nghĩa regex gốc.)
fn is_plain_number(val: &str) -> bool {
    if val.is_empty() {
        return false;
    }
    let rest = val.strip_prefix('-').unwrap_or(val);
    !rest.is_empty() && rest.chars().all(|c| c.is_ascii_digit() || c == '.')
}

pub fn map_justify(val: &str) -> String {
    match val {
        "start" => "flex-start".to_string(),
        "end" => "flex-end".to_string(),
        "center" | "giua" => "center".to_string(),
        "space-between" => "space-between".to_string(),
        "space-around" => "space-around".to_string(),
        other => other.to_string(),
    }
}

pub fn map_align_items(val: &str) -> String {
    match val {
        "start" => "flex-start".to_string(),
        "end" => "flex-end".to_string(),
        "center" | "giua" => "center".to_string(),
        "stretch" | "deu" => "stretch".to_string(),
        // BUG ĐÃ SỬA: trước đây nhánh "other => other.to_string()" khiến
        // giá trị tiếng Việt không khớp case nào (vd "deu") bị in THẲNG
        // ra CSS làm "align-items:deu" — đây là giá trị CSS KHÔNG HỢP
        // LỆ (align-items không có khái niệm "đều"/space-between, đó là
        // khái niệm của justify-content, khác trục). Trình duyệt âm
        // thầm bỏ qua thuộc tính không hợp lệ (không báo lỗi gì), khiến
        // layout sai lệch mà không ai biết nguyên nhân — phát hiện qua
        // build thử 1 app thật (dist_ver_0_0_6). "deu" giờ map về
        // "stretch" (giá trị CSS hợp lệ gần nghĩa nhất: phần tử giãn
        // chiếm hết chiều ngang trục cắt — gần với ý "đều" nhất trong
        // các lựa chọn align-items có sẵn).
        //
        // Với giá trị KHÔNG nhận diện được nào khác (không phải lỗi gõ
        // tiếng Việt đã biết), vẫn pass-through nguyên văn — cho phép
        // Dev viết thẳng giá trị CSS hợp lệ khác (vd "baseline") mà
        // bảng này chưa liệt kê tường minh, thay vì chặn cứng.
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
    fn test_flex_default_display() {
        let props: PropsMap = vec![];
        let css = resolve_flex(&props);
        assert_eq!(css.get("display"), Some(&"flex".to_string()));
    }

    #[test]
    fn test_map_align_items_deu_maps_to_stretch() {
        // Test hồi quy trực tiếp cho bug đã sửa: trước đây "deu" không
        // khớp case nào trong map_align_items, bị in thẳng ra CSS thành
        // "align-items:deu" — giá trị KHÔNG HỢP LỆ, trình duyệt âm thầm
        // bỏ qua khiến layout sai (phát hiện qua build thử app thật).
        assert_eq!(map_align_items("deu"), "stretch");
    }

    #[test]
    fn test_map_align_items_known_values() {
        assert_eq!(map_align_items("start"), "flex-start");
        assert_eq!(map_align_items("end"), "flex-end");
        assert_eq!(map_align_items("center"), "center");
        assert_eq!(map_align_items("giua"), "center");
        assert_eq!(map_align_items("stretch"), "stretch");
    }

    #[test]
    fn test_map_align_items_unknown_passthrough() {
        // Giá trị lạ KHÔNG nằm trong danh sách lỗi tiếng Việt đã biết
        // vẫn pass-through nguyên văn — cho phép Dev viết thẳng giá trị
        // CSS hợp lệ khác (vd baseline) chưa được liệt kê tường minh.
        assert_eq!(map_align_items("baseline"), "baseline");
    }

    #[test]
    fn test_doc_prop_end_to_end_no_invalid_css_value() {
        // Test hồi quy mức cao hơn: mô phỏng đúng cách bug thật xảy ra —
        // qua resolve_flex() với prop "doc: deu" (không gọi thẳng
        // map_align_items) — xác nhận CSS sinh ra không còn chứa "deu"
        // (giá trị không hợp lệ) mà đã là "stretch".
        let props: PropsMap = vec![("doc".to_string(), Expr::literal_str("deu", p()))];
        let css = resolve_flex(&props);
        assert_eq!(css.get("alignItems"), Some(&"stretch".to_string()));
    }

    #[test]
    fn test_flex_huong_column() {
        let props: PropsMap = vec![("huong".to_string(), Expr::literal_str("column", p()))];
        let css = resolve_flex(&props);
        assert_eq!(css.get("flexDirection"), Some(&"column".to_string()));
    }

    #[test]
    fn test_grid_cot_numeric_becomes_repeat() {
        let props: PropsMap = vec![("cot".to_string(), Expr::literal_str("3", p()))];
        let css = resolve_grid(&props);
        assert_eq!(css.get("gridTemplateColumns"), Some(&"repeat(3, 1fr)".to_string()));
    }

    #[test]
    fn test_grid_cot_raw_value_kept() {
        let props: PropsMap = vec![("cot".to_string(), Expr::literal_str("1fr 2fr", p()))];
        let css = resolve_grid(&props);
        assert_eq!(css.get("gridTemplateColumns"), Some(&"1fr 2fr".to_string()));
    }

    #[test]
    fn test_box_border_uses_kieu_vien_and_mau_vien() {
        let props: PropsMap = vec![
            ("vien".to_string(), Expr::literal_num(2.0, p())),
            ("kieu_vien".to_string(), Expr::literal_str("dashed", p())),
            ("mau_vien".to_string(), Expr::Literal(vibao_ast::LiteralValue::Color("#FF0000".to_string()), p())),
        ];
        let css = resolve_box(&props);
        assert_eq!(css.get("border"), Some(&"2px dashed #FF0000".to_string()));
    }

    #[test]
    fn test_stack_forces_grid_1fr() {
        let props: PropsMap = vec![];
        let css = resolve_stack(&props);
        assert_eq!(css.get("gridTemplateColumns"), Some(&"1fr".to_string()));
        assert_eq!(css.get("gridTemplateRows"), Some(&"1fr".to_string()));
    }

    #[test]
    fn test_fixed_vi_tri_tren() {
        let props: PropsMap = vec![("vi_tri".to_string(), Expr::literal_str("tren", p()))];
        let css = resolve_fixed(&props);
        assert_eq!(css.get("top"), Some(&"0".to_string()));
        assert_eq!(css.get("left"), Some(&"0".to_string()));
        assert_eq!(css.get("right"), Some(&"0".to_string()));
    }

    #[test]
    fn test_build_media_query_empty_overrides_returns_empty_string() {
        let bp_css = ResponsiveCss {
            media_condition: "(max-width: 639px)".to_string(),
            overrides: LayoutCss::new(),
        };
        assert_eq!(build_media_query("#foo", &bp_css), "");
    }

    #[test]
    fn test_build_media_query_with_overrides() {
        let mut overrides = LayoutCss::new();
        overrides.insert("display".to_string(), "none".to_string());
        let bp_css = ResponsiveCss {
            media_condition: "(max-width: 639px)".to_string(),
            overrides,
        };
        let out = build_media_query("#vb-box-1", &bp_css);
        assert!(out.contains("@media (max-width: 639px)"));
        assert!(out.contains("#vb-box-1"));
        assert!(out.contains("display: none;"));
    }

    #[test]
    fn test_px_helper() {
        assert_eq!(px("16"), "16px");
        assert_eq!(px("-16"), "-16px");
        assert_eq!(px("50%"), "50%");
        assert_eq!(px("__dynamic__"), "__dynamic__");
    }

    #[test]
    fn test_px_helper_rejects_dash_in_middle() {
        // Regression test cùng bug đã sửa ở props.rs — xem ghi chú tại
        // is_plain_number().
        assert_eq!(px("1-2"), "1-2");
    }

    #[test]
    fn test_size_helper_rejects_negative() {
        // size() dùng regex không dấu trừ ở bản TS gốc — "-16" không khớp
        // /^[\d.]+$/ nên được giữ nguyên, không thêm "px".
        assert_eq!(size("16"), "16px");
        assert_eq!(size("-16"), "-16");
    }

    #[test]
    fn test_is_layout_tag() {
        assert!(is_layout_tag("flex"));
        assert!(is_layout_tag("box"));
        assert!(!is_layout_tag("text"));
        assert!(!is_layout_tag("button"));
    }
}
