// ============================================================
// VIBAO COMPILER (Rust) — codegen/element.rs
// Sinh HTML cho 1 Element cụ thể — phân luồng theo 3 loại: SIMPLE
// (text, button, image...), LAYOUT (flex, box, grid...), và
// COMPLEX built-in (form, modal, tabs...). Tương đương phần
// "5. ELEMENT GENERATION" của 11-codegen-core.ts.
// ============================================================

use vibao_ast::{get_prop, AnimationProps, Element, LapValue};
use crate::codegen::css::{esc_attr, indent2, layout_css_to_string, layout_css_to_string_inline, style_map_to_string, OrderedMap};
use crate::codegen::expr::{expr_to_js_default, resolve_value, ResolvedValue};
use crate::codegen::layout::{is_layout_tag, resolve_layout_css, resolve_responsive_css, build_media_query};
use crate::codegen::props::expand_props;

/// Danh sách 10 built-in complex component — element.rs chỉ sinh 1 thẻ
/// placeholder cho các tag này (nội dung thật do runtime tự dựng lúc
/// chạy, xem component.rs cho phần mount tương ứng). Khớp BUILTIN_COMPLEX
/// ở bản TS cũ.
pub const BUILTIN_COMPLEX: [&str; 10] = [
    "form",
    "modal",
    "tabs",
    "accordion",
    "carousel",
    "xuong_trang",
    "bang",
    "bieu_do",
    "ban_do",
    "trinh_soan_thao",
];

pub fn is_builtin_complex(tag: &str) -> bool {
    BUILTIN_COMPLEX.contains(&tag)
}

/// Ánh xạ tag ViBao → tên thẻ HTML thật. Tag không có trong bảng (vd
/// các layout tag, component custom) mặc định ra "div". Khớp TAG_MAP.
fn tag_to_html(tag: &str) -> &str {
    match tag {
        "text" => "p",
        "h1" => "h1",
        "h2" => "h2",
        "h3" => "h3",
        "p" => "p",
        "nhan" => "span",
        "button" => "button",
        "link" | "lien_ket" => "a",
        "image" => "img",
        "video" => "video",
        "icon" => "span",
        "input" => "input",
        "spacer" => "div",
        "divider" => "hr",
        "vong_quay" => "div",
        "thanh_tien_trinh" => "div",
        "scroll" => "div",
        "container" => "div",
        _ => "div",
    }
}

const SELF_CLOSING: [&str; 4] = ["img", "input", "hr", "br"];

fn is_self_closing(html_tag: &str) -> bool {
    SELF_CLOSING.contains(&html_tag)
}

/// Context tối thiểu cần truyền cho hàm sinh element — con trỏ ngược tới
/// Codegen thật (nằm ở mod.rs) sẽ implement trait này để genElement() có
/// thể gọi lại genChildren() một cách đệ quy mà không cần import chéo
/// vòng lặp giữa mod.rs và element.rs. Tương đương tham số `codegen` ở
/// getContent() bản TS cũ.
pub trait ElementCodegenHost {
    /// Sinh id mới duy nhất trong trang hiện tại, dùng prefix `tag`.
    fn next_id(&mut self, tag: &str) -> String;
    /// Sinh HTML cho toàn bộ Vec<Child> — đệ quy quay lại genChild ở mod.rs.
    fn gen_children(&mut self, children: &[vibao_ast::Child]) -> String;
    /// Đăng ký 1 khối JS event handler / animation binding vào output JS.
    fn add_js(&mut self, code: &str);
    /// Đăng ký 1 khối CSS rule (không phải media query) vào stylesheet.
    fn add_css(&mut self, code: &str);
    /// Đăng ký 1 khối @media query vào stylesheet.
    fn add_media_query(&mut self, code: &str);
    /// Đăng ký 1 cảnh báo build (in ra terminal với prefix ⚠️, không
    /// dừng build) — dùng cho các trường hợp cú pháp HỢP LỆ về mặt
    /// parse nhưng khả nghi về ý nghĩa, vd tên prop không nhận diện
    /// được (khả năng gõ sai chính tả). Có default no-op để các
    /// implementor CŨ (test double ở component.rs/element.rs) không
    /// cần sửa gì vẫn compile được.
    fn add_warning(&mut self, _msg: String) {}
    /// Biên dịch 1 EventNode (vd nhan_click) thành JS binding — cần ở
    /// đây vì cả simple lẫn layout element đều có thể có events/animation.
    ///
    /// LƯU Ý: hàm này thuộc kiến trúc CŨ (sinh JS qua add_js) — không còn
    /// khớp với runtime Rust/WASM hiện tại (không có "__vb"/"__api" JS
    /// global nào để gọi tới nữa). Đường dẫn ĐÚNG cho pipeline hiện tại
    /// là `compile_event_handler_registry()` bên dưới — hàm cũ vẫn giữ
    /// trong trait (không xoá) để không phá các implementor hiện có (vd
    /// test double ở component.rs/element.rs), nhưng KHÔNG được gọi từ
    /// gen_simple_element/gen_layout_element nữa (xem chỗ dùng).
    fn compile_event_handler(&self, event: &vibao_ast::EventNode, id: &str) -> String;

    /// Biên dịch 1 EventNode thành `(tên_attribute, action_id)` để nhúng
    /// thẳng vào HTML dưới dạng `data-vb-on-<event>="<id>"`, KHÔNG sinh
    /// JS. Đây LÀ đường dẫn dùng bởi pipeline build thật hiện tại — xem
    /// codegen/action.rs::compile_event_handler_registry.
    ///
    /// Có giá trị mặc định gọi thẳng `crate::codegen::action::compile_event_handler_registry`
    /// — implementor thường KHÔNG cần override hàm này, vì nó không phụ
    /// thuộc trạng thái nào của `self` (khác `compile_event_handler` cũ,
    /// vốn cần override vì mỗi implementor có thể muốn add_js theo cách
    /// khác nhau). Default method giúp các implementor CŨ (test double)
    /// tự động có luôn hành vi mới mà không cần sửa gì.
    fn compile_event_handler_registry(&self, event: &vibao_ast::EventNode) -> (String, String) {
        crate::codegen::action::compile_event_handler_registry(event)
    }
    fn compile_hover_animation(&self, id: &str, effect: &str, duration_ms: u32) -> String;
    fn compile_scroll_animation(&self, id: &str, effect: &str, duration_ms: u32, delay_ms: u32) -> String;
}

/// Điểm vào chính — phân luồng theo loại tag. Tương đương genElement().
pub fn gen_element(node: &Element, host: &mut dyn ElementCodegenHost) -> String {
    let id = host.next_id(&node.tag);
    if is_builtin_complex(&node.tag) {
        gen_complex_component(node, &id)
    } else if is_layout_tag(&node.tag) {
        gen_layout_element(node, &id, host)
    } else {
        gen_simple_element(node, &id, host)
    }
}

// ════════════════════════════════════════════════════════════
// SIMPLE ELEMENT
// ════════════════════════════════════════════════════════════

fn gen_simple_element(node: &Element, id: &str, host: &mut dyn ElementCodegenHost) -> String {
    let expanded = expand_props(&node.tag, &node.props);
    for key in &expanded.unknown_keys {
        // "data_*"/"aria_*" là passthrough HTML attr có chủ đích (sinh
        // ra data-*/aria-* thật ở codegen) — không cảnh báo cho các
        // trường hợp này, chỉ cảnh báo prop khả nghi là gõ sai chính tả.
        if !(key.starts_with("data_") || key.starts_with("aria_")) {
            host.add_warning(format!(
                "prop '{}' trên thẻ '{}' không được ViBao nhận diện — có thể do gõ sai tên prop. \
                 Prop lạ vẫn được ghi thẳng thành attribute HTML '{}=\"...\"' (không tự đổi thành \
                 kebab-case), nên nếu đây là attribute tuỳ ý có chủ đích, hãy kiểm tra tên có đúng \
                 định dạng attribute HTML mong muốn chưa.",
                key, node.tag, key,
            ));
        }
    }
    let style_str = style_map_to_string(&expanded.style);
    let attrs_str = attrs_to_string_ordered(&expanded.attrs, &["noi_dung"]);
    let dynamic_attrs = expanded
        .dynamic
        .iter()
        .map(|(k, v)| format!("data-vb-bind-{}=\"{}\"", k, esc_attr(v)))
        .collect::<Vec<_>>()
        .join(" ");
    let anim_attrs = gen_anim_attrs(&node.animation);

    // Đường dẫn ĐÚNG cho pipeline hiện tại: mỗi EventNode được đăng ký
    // vào action registry, sinh ra 1 HTML attribute
    // "data-vb-on-<dom-event>=\"<actionId>\"" — KHÔNG sinh JS nào cả
    // (khác hẳn compile_event_handler cũ, add_js — xem ghi chú ở trait
    // ElementCodegenHost). Runtime WASM (dom.rs::bind_events) tự đọc
    // attribute này lúc bind trang.
    let event_attrs = node
        .events
        .iter()
        .map(|e| {
            let (attr_name, action_id) = host.compile_event_handler_registry(e);
            format!("{}=\"{}\"", attr_name, action_id)
        })
        .collect::<Vec<_>>()
        .join(" ");

    // Animation hover/scroll giờ được gen_anim_attrs() xử lý luôn cùng
    // với animation load-in (data-vb-anim) — xem sửa đổi ở gen_anim_attrs
    // bên dưới. KHÔNG còn add_js(...) ở đây (kiến trúc JS cũ đã bỏ — xem
    // ghi chú ở action.rs::compile_hover_animation/compile_scroll_animation,
    // 2 hàm đó vẫn giữ nguyên chỉ để không phá test hiện có, không dùng
    // ở pipeline build thật nữa).

    if !node.responsive.is_empty() {
        let bp_css = resolve_responsive_css(&node.tag, &node.responsive);
        for bp in &bp_css {
            let mq = build_media_query(&format!("#{}", id), bp);
            if !mq.is_empty() {
                host.add_media_query(&mq);
            }
        }
    }

    let content = get_content(&node.tag, &node.props, &node.children, host);
    let html_tag = tag_to_html(&node.tag);
    let self_closing = is_self_closing(html_tag);

    let all_attrs = [
        format!("id=\"{}\"", id),
        if style_str.is_empty() { String::new() } else { format!("style=\"{}\"", style_str) },
        attrs_str,
        dynamic_attrs,
        anim_attrs,
        event_attrs,
        if !node.events.is_empty() { "data-vb-interactive".to_string() } else { String::new() },
    ]
    .into_iter()
    .filter(|s| !s.is_empty())
    .collect::<Vec<_>>()
    .join(" ");

    if self_closing {
        format!("<{} {} />", html_tag, all_attrs)
    } else {
        format!("<{} {}>{}</{}>", html_tag, all_attrs, content, html_tag)
    }
}

/// Nội dung bên trong thẻ: ưu tiên prop `noi_dung` (text literal hoặc
/// binding động qua <span data-vb-text>), rồi mới tới children lồng
/// bên trong. Tương đương getContent() ở bản TS cũ — LƯU Ý: bản TS gốc
/// dùng key "_content", nhưng parser Rust (parser/element.rs, dòng
/// parse_element_rest) tự đặt tên "noi_dung" cho tham số nội dung viết
/// tắt (vd text("Xin chào")) — codegen phải khớp với parser Rust THẬT
/// đang chạy, không phải bản TS gốc.
fn get_content(tag: &str, props: &vibao_ast::PropsMap, children: &[vibao_ast::Child], host: &mut dyn ElementCodegenHost) -> String {
    let _ = tag; // giữ tham số để khớp chữ ký gốc dù hiện chưa dùng riêng
    if let Some(content_expr) = get_prop(props, "noi_dung") {
        let resolved = resolve_value(content_expr);
        return match resolved {
            ResolvedValue::Dynamic(_) => {
                format!("<span data-vb-text=\"{}\"></span>", esc_attr(&expr_to_js_default(content_expr)))
            }
            ResolvedValue::Static(s) => s,
            // Size/Color không có ý nghĩa làm nội dung text — bản TS cũ
            // chỉ trả String(resolved.value) khi kind === "static", các
            // kind khác trả "" (xem nhánh cuối `String(resolved.kind ===
            // "static" ? resolved.value : "")`).
            _ => String::new(),
        };
    }
    if !children.is_empty() {
        format!("\n{}\n", indent2(&host.gen_children(children)))
    } else {
        String::new()
    }
}

fn attrs_to_string_ordered(attrs: &OrderedMap, skip: &[&str]) -> String {
    attrs
        .iter()
        .filter(|(k, _)| !skip.contains(&k.as_str()))
        .map(|(k, v)| format!("{}=\"{}\"", k, v))
        .collect::<Vec<_>>()
        .join(" ")
}

// ════════════════════════════════════════════════════════════
// LAYOUT ELEMENT
// ════════════════════════════════════════════════════════════

fn gen_layout_element(node: &Element, id: &str, host: &mut dyn ElementCodegenHost) -> String {
    let layout_css = resolve_layout_css(&node.tag, &node.props);
    let style_str = layout_css_to_string_inline(&layout_css);

    // "color" trên layout tag chỉ binding động nếu prop là 1 Variable
    // thuần (không phải biểu thức phức tạp hơn) — khớp check
    // `props["color"]?.type === "Variable"` ở bản TS cũ, cụ thể hơn so
    // với is_dynamic() chung của resolve_value().
    let dynamic_color = match get_prop(&node.props, "color") {
        Some(expr @ vibao_ast::Expr::Variable(_, _)) => {
            format!("data-vb-bind-backgroundColor=\"{}\"", esc_attr(&expr_to_js_default(expr)))
        }
        _ => String::new(),
    };
    let anim_attrs = gen_anim_attrs(&node.animation);

    host.add_css(&layout_css_to_string(&format!("#{}", id), &layout_css));

    if !node.responsive.is_empty() {
        let bp_css = resolve_responsive_css(&node.tag, &node.responsive);
        for bp in &bp_css {
            let mq = build_media_query(&format!("#{}", id), bp);
            if !mq.is_empty() {
                host.add_media_query(&mq);
            }
        }
    }
    if let Some(hover) = &node.animation.hieu_ung_hover {
        host.add_js(&host.compile_hover_animation(id, hover, node.animation.thoi_gian.unwrap_or(300)));
    }
    if let Some(scroll) = &node.animation.hieu_ung_cuon {
        host.add_js(&host.compile_scroll_animation(
            id,
            scroll,
            node.animation.thoi_gian.unwrap_or(600),
            node.animation.tre.unwrap_or(0),
        ));
    }

    let children_html = if node.tag == "stack" {
        node.children
            .iter()
            .map(|c| {
                let html = host.gen_children(std::slice::from_ref(c));
                if html.is_empty() { String::new() } else { wrap_stack_child(&html) }
            })
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        host.gen_children(&node.children)
    };

    let extra_style = if node.tag == "layer" { "position:relative;" } else { "" };

    format!(
        "<div id=\"{}\" style=\"{}{}\" {} {}>\n{}\n</div>",
        id,
        extra_style,
        style_str,
        dynamic_color,
        anim_attrs,
        indent2(&children_html)
    )
}

fn wrap_stack_child(html: &str) -> String {
    format!("<div style=\"grid-area:1/1/2/2\">{}</div>", html)
}

// ════════════════════════════════════════════════════════════
// COMPLEX BUILT-IN COMPONENT (placeholder — nội dung thật do runtime dựng)
// ════════════════════════════════════════════════════════════

fn gen_complex_component(node: &Element, id: &str) -> String {
    format!("<div id=\"{}\" data-vb-component=\"{}\"><!-- {} --></div>", id, node.tag, node.tag)
}

/// Tên class CSS quy ước cho layout element — hiện chưa dùng trực tiếp
/// trong gen_layout_element() (bản TS cũ có tính nhưng cuối cùng dùng id
/// làm selector CSS, không dùng className), giữ lại để khớp API công
/// khai makeClassName() gốc, phòng khi cần ở nơi khác (vd debug tooling).
pub fn make_class_name(tag: &str, index: u32) -> String {
    format!("vb-{}-{}", tag, index)
}

// ════════════════════════════════════════════════════════════
// ANIMATION ATTRS
// ════════════════════════════════════════════════════════════

/// Sinh các data-vb-anim-* attr từ AnimationProps — chỉ sinh gì khi có
/// `hieu_ung` (hiệu ứng khi tải/hiện); hover/scroll animation không sinh
/// attr ở đây vì chúng được xử lý qua JS binding riêng (add_js ở trên).
/// Tương đương genAnimAttrs() ở bản TS cũ.
/// Sinh mọi HTML attribute liên quan animation của 1 element — gộp cả
/// animation load-in (hieu_ung, chạy ngay khi mount) VÀ hover/scroll
/// (hieu_ung_hover/hieu_ung_cuon, chạy theo sự kiện). Tất cả đều là
/// ATTRIBUTE THUẦN, không sinh JS — runtime WASM (dom.rs) tự đọc và xử
/// lý bằng Rust (web-sys IntersectionObserver/mouseenter/mouseleave),
/// khớp đúng kiến trúc "Rust thuần, không eval JS" đã chọn cho toàn bộ
/// runtime. Đây LÀ đường dẫn ĐÚNG cho pipeline build hiện tại — thay
/// thế hoàn toàn compile_hover_animation/compile_scroll_animation (JS
/// cũ ở action.rs, giữ lại chỉ để không phá test, KHÔNG dùng nữa).
pub fn gen_anim_attrs(anim: &AnimationProps) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(hieu_ung) = &anim.hieu_ung {
        parts.push(format!("data-vb-anim=\"{}\"", hieu_ung));
        if let Some(ms) = anim.thoi_gian {
            parts.push(format!("data-vb-anim-duration=\"{}\"", ms));
        }
        if let Some(ms) = anim.tre {
            parts.push(format!("data-vb-anim-delay=\"{}\"", ms));
        }
        if let Some(lap) = &anim.lap {
            let lap_str = match lap {
                LapValue::Count(n) => n.to_string(),
                LapValue::MaiMai => "infinite".to_string(),
            };
            parts.push(format!("data-vb-anim-repeat=\"{}\"", lap_str));
        }
    }

    // Hover: "<ten_hieu_ung>:<thoi_gian_ms>" — 1 attribute duy nhất chứa
    // cả 2 giá trị (phân tách bằng ":") để đơn giản hoá phía runtime
    // (chỉ cần split 1 chuỗi, không cần đọc thêm attribute -duration
    // riêng như nhánh load-in ở trên).
    if let Some(hover) = &anim.hieu_ung_hover {
        let dur = anim.thoi_gian.unwrap_or(300);
        parts.push(format!("data-vb-anim-hover=\"{}:{}\"", hover, dur));
    }

    // Scroll: "<ten_hieu_ung>:<thoi_gian_ms>:<tre_ms>".
    if let Some(scroll) = &anim.hieu_ung_cuon {
        let dur = anim.thoi_gian.unwrap_or(600);
        let delay = anim.tre.unwrap_or(0);
        parts.push(format!("data-vb-anim-scroll=\"{}:{}:{}\"", scroll, dur, delay));
    }

    parts.join(" ")
}

// ════════════════════════════════════════════════════════════
// UNIT TESTS
// ════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use vibao_ast::{Child, Element, Expr, Pos};

    fn p() -> Pos {
        Pos { line: 1, column: 1 }
    }

    /// Host giả lập tối thiểu cho test — không cần Codegen thật để kiểm
    /// tra logic thuần của element.rs.
    struct FakeHost {
        counter: u32,
        js: Vec<String>,
        css: Vec<String>,
        media: Vec<String>,
        warnings: Vec<String>,
    }

    impl FakeHost {
        fn new() -> Self {
            FakeHost { counter: 0, js: vec![], css: vec![], media: vec![], warnings: vec![] }
        }
    }

    impl ElementCodegenHost for FakeHost {
        fn next_id(&mut self, tag: &str) -> String {
            self.counter += 1;
            format!("vb-{}-{}", tag, self.counter)
        }
        fn gen_children(&mut self, children: &[Child]) -> String {
            children
                .iter()
                .map(|c| match c {
                    Child::Element(el) => gen_element(el, self),
                    _ => String::new(),
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
        fn add_js(&mut self, code: &str) {
            self.js.push(code.to_string());
        }
        fn add_css(&mut self, code: &str) {
            self.css.push(code.to_string());
        }
        fn add_media_query(&mut self, code: &str) {
            self.media.push(code.to_string());
        }
        fn add_warning(&mut self, msg: String) {
            self.warnings.push(msg);
        }
        fn compile_event_handler(&self, _event: &vibao_ast::EventNode, _id: &str) -> String {
            String::new()
        }
        fn compile_hover_animation(&self, _id: &str, _effect: &str, _duration_ms: u32) -> String {
            String::new()
        }
        fn compile_scroll_animation(&self, _id: &str, _effect: &str, _duration_ms: u32, _delay_ms: u32) -> String {
            String::new()
        }
    }

    fn make_element(tag: &str, props: vibao_ast::PropsMap) -> Element {
        Element {
            tag: tag.to_string(),
            props,
            children: vec![],
            events: vec![],
            responsive: vec![],
            animation: AnimationProps::default(),
            pos: p(),
        }
    }

    #[test]
    fn test_gen_simple_element_warns_on_unknown_prop() {
        let props: vibao_ast::PropsMap = vec![(
            "mua".to_string(),
            Expr::Literal(vibao_ast::LiteralValue::Str("do".to_string()), p()),
        )];
        let el = make_element("text", props);
        let mut host = FakeHost::new();
        gen_element(&el, &mut host);
        assert_eq!(host.warnings.len(), 1);
        assert!(host.warnings[0].contains("mua"));
        assert!(host.warnings[0].contains("text"));
    }

    #[test]
    fn test_gen_simple_element_no_warning_for_data_prefixed_prop() {
        // "data_*" là passthrough HTML attr có chủ đích — không phải
        // gõ sai tên prop, nên KHÔNG được cảnh báo.
        let props: vibao_ast::PropsMap = vec![(
            "data_testid".to_string(),
            Expr::Literal(vibao_ast::LiteralValue::Str("hero".to_string()), p()),
        )];
        let el = make_element("text", props);
        let mut host = FakeHost::new();
        gen_element(&el, &mut host);
        assert!(host.warnings.is_empty());
    }

    #[test]
    fn test_gen_simple_element_no_warning_for_known_prop() {
        let props: vibao_ast::PropsMap = vec![(
            "mau".to_string(),
            Expr::Literal(vibao_ast::LiteralValue::Color("#FF0000".to_string()), p()),
        )];
        let el = make_element("text", props);
        let mut host = FakeHost::new();
        gen_element(&el, &mut host);
        assert!(host.warnings.is_empty());
    }

    #[test]
    fn test_tag_to_html_mapping() {
        assert_eq!(tag_to_html("text"), "p");
        assert_eq!(tag_to_html("link"), "a");
        assert_eq!(tag_to_html("lien_ket"), "a");
        assert_eq!(tag_to_html("image"), "img");
        assert_eq!(tag_to_html("unknown_custom_tag"), "div");
    }

    #[test]
    fn test_self_closing_tags() {
        assert!(is_self_closing("img"));
        assert!(is_self_closing("input"));
        assert!(!is_self_closing("div"));
        assert!(!is_self_closing("p"));
    }

    #[test]
    fn test_simple_element_text_content() {
        let mut host = FakeHost::new();
        let el = make_element("text", vec![("noi_dung".to_string(), Expr::literal_str("Xin chào", p()))]);
        let html = gen_simple_element(&el, "vb-text-1", &mut host);
        assert!(html.starts_with("<p "));
        assert!(html.contains(">Xin chào</p>"));
    }

    #[test]
    fn test_simple_element_self_closing_image() {
        let mut host = FakeHost::new();
        let el = make_element("image", vec![("alt".to_string(), Expr::literal_str("logo", p()))]);
        let html = gen_simple_element(&el, "vb-image-1", &mut host);
        assert!(html.starts_with("<img "));
        assert!(html.ends_with("/>"));
    }

    #[test]
    fn test_simple_element_dynamic_content_uses_span_binding() {
        let mut host = FakeHost::new();
        let el = make_element("text", vec![("noi_dung".to_string(), Expr::Variable("ten".to_string(), p()))]);
        let html = gen_simple_element(&el, "vb-text-1", &mut host);
        assert!(html.contains("data-vb-text="));
    }

    #[test]
    fn test_layout_element_is_div_with_css_registered() {
        let mut host = FakeHost::new();
        let el = make_element("flex", vec![]);
        let html = gen_layout_element(&el, "vb-flex-1", &mut host);
        assert!(html.starts_with("<div id=\"vb-flex-1\""));
        assert_eq!(host.css.len(), 1);
        assert!(host.css[0].contains("display: flex;"));
    }

    #[test]
    fn test_layout_element_layer_gets_relative_position() {
        let mut host = FakeHost::new();
        let el = make_element("layer", vec![]);
        let html = gen_layout_element(&el, "vb-layer-1", &mut host);
        assert!(html.contains("position:relative;"));
    }

    #[test]
    fn test_complex_component_placeholder() {
        let el = make_element("modal", vec![]);
        let html = gen_complex_component(&el, "vb-modal-1");
        assert_eq!(html, "<div id=\"vb-modal-1\" data-vb-component=\"modal\"><!-- modal --></div>");
    }

    #[test]
    fn test_gen_element_dispatches_to_complex_for_builtin() {
        let mut host = FakeHost::new();
        let el = make_element("tabs", vec![]);
        let html = gen_element(&el, &mut host);
        assert!(html.contains("data-vb-component=\"tabs\""));
    }

    #[test]
    fn test_gen_element_dispatches_to_layout_for_flex() {
        let mut host = FakeHost::new();
        let el = make_element("flex", vec![]);
        let html = gen_element(&el, &mut host);
        assert!(html.starts_with("<div id=\"vb-flex-1\""));
    }

    #[test]
    fn test_gen_anim_attrs_hover_only() {
        let anim = AnimationProps {
            hieu_ung: None,
            thoi_gian: Some(400),
            tre: None,
            lap: None,
            hieu_ung_hover: Some("phong_to".to_string()),
            hieu_ung_cuon: None,
        };
        let out = gen_anim_attrs(&anim);
        assert_eq!(out, "data-vb-anim-hover=\"phong_to:400\"");
    }

    #[test]
    fn test_gen_anim_attrs_hover_default_duration() {
        let anim = AnimationProps {
            hieu_ung_hover: Some("phong_to".to_string()),
            ..AnimationProps::default()
        };
        let out = gen_anim_attrs(&anim);
        assert_eq!(out, "data-vb-anim-hover=\"phong_to:300\"");
    }

    #[test]
    fn test_gen_anim_attrs_scroll_with_delay() {
        let anim = AnimationProps {
            hieu_ung_cuon: Some("truot_len".to_string()),
            thoi_gian: Some(500),
            tre: Some(150),
            ..AnimationProps::default()
        };
        let out = gen_anim_attrs(&anim);
        assert_eq!(out, "data-vb-anim-scroll=\"truot_len:500:150\"");
    }

    #[test]
    fn test_gen_anim_attrs_combines_load_and_hover() {
        let anim = AnimationProps {
            hieu_ung: Some("fade_in".to_string()),
            thoi_gian: Some(200),
            hieu_ung_hover: Some("phong_to".to_string()),
            ..AnimationProps::default()
        };
        let out = gen_anim_attrs(&anim);
        assert!(out.contains("data-vb-anim=\"fade_in\""));
        assert!(out.contains("data-vb-anim-hover=\"phong_to:200\""));
    }

    #[test]
    fn test_gen_anim_attrs_empty_without_hieu_ung() {
        let anim = AnimationProps::default();
        assert_eq!(gen_anim_attrs(&anim), "");
    }

    #[test]
    fn test_gen_anim_attrs_full() {
        let anim = AnimationProps {
            hieu_ung: Some("fade_in".to_string()),
            thoi_gian: Some(500),
            tre: Some(100),
            lap: Some(LapValue::MaiMai),
            hieu_ung_hover: None,
            hieu_ung_cuon: None,
        };
        let out = gen_anim_attrs(&anim);
        assert!(out.contains("data-vb-anim=\"fade_in\""));
        assert!(out.contains("data-vb-anim-duration=\"500\""));
        assert!(out.contains("data-vb-anim-delay=\"100\""));
        assert!(out.contains("data-vb-anim-repeat=\"infinite\""));
    }

    #[test]
    fn test_make_class_name() {
        assert_eq!(make_class_name("box", 3), "vb-box-3");
    }
}
