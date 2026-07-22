// ============================================================
// VIBAO COMPILER (Rust) — codegen/component.rs
// Đăng ký + sinh HTML/JS cho lời gọi component custom (@the). Tương
// đương ComponentRegistry của 09-parser-component.ts + phần
// "7. COMPONENT CALL GENERATION" / genComponentDef() của
// 11-codegen-core.ts.
//
// LƯU Ý THIẾT KẾ: bản TS gốc dùng `globalRegistry` là 1 biến module
// toàn cục (singleton, sống suốt process). Ở Rust, global mutable state
// kiểu đó cần unsafe hoặc Mutex — không cần thiết và không idiomatic ở
// đây, vì compiler luôn chạy hết đời trong 1 lần gọi generate() duy
// nhất. Ta để ComponentRegistry là 1 field sở hữu bởi Codegen (xem
// mod.rs), truyền qua tham số thay vì static — cùng hành vi, an toàn
// hơn, và dễ test độc lập (mỗi test tạo 1 registry riêng, không rò rỉ
// state giữa các test như biến toàn cục có thể gây ra).
// ============================================================

use vibao_ast::{ComponentDef, ComponentCall};
use crate::codegen::css::{esc_attr, indent2};
use crate::codegen::element::ElementCodegenHost;
use crate::codegen::expr::{expr_to_js_default, json_string};
use std::collections::HashMap;

/// Sổ đăng ký các @the component definition — tra cứu theo tên khi gặp
/// 1 lời gọi component trong cây AST.
#[derive(Debug, Clone, Default)]
pub struct ComponentRegistry {
    defs: HashMap<String, ComponentDef>,
    /// Ghi lại các cảnh báo phát sinh trong lúc đăng ký/tra cứu (component
    /// định nghĩa trùng tên, gọi component chưa định nghĩa) — bản TS cũ
    /// dùng console.warn trực tiếp; ở đây ta thu thập lại để caller (vd
    /// error-handler.rs/CLI) quyết định hiển thị thế nào, thay vì in
    /// thẳng ra stderr ngay trong logic thuần của registry.
    pub warnings: Vec<String>,
}

impl ComponentRegistry {
    pub fn new() -> Self {
        ComponentRegistry::default()
    }

    /// Đăng ký 1 định nghĩa @the — ghi cảnh báo (không panic) nếu tên đã
    /// tồn tại, sau đó GHI ĐÈ bằng định nghĩa mới, khớp hành vi
    /// `this.defs.set(...)` luôn chạy vô điều kiện ở bản TS cũ.
    pub fn register(&mut self, def: ComponentDef) {
        if self.defs.contains_key(&def.name) {
            self.warnings.push(format!("[ViBao] Component \"@the {}\" bị định nghĩa lại", def.name));
        }
        self.defs.insert(def.name.clone(), def);
    }

    pub fn get(&self, name: &str) -> Option<&ComponentDef> {
        self.defs.get(name)
    }

    pub fn has(&self, name: &str) -> bool {
        self.defs.contains_key(name)
    }

    pub fn get_all(&self) -> Vec<&ComponentDef> {
        self.defs.values().collect()
    }

    pub fn clear(&mut self) {
        self.defs.clear();
    }
}

/// Sinh HTML + JS mount cho 1 lời gọi component (vd <TheCard tieu_de="...">).
/// Tương đương genComponentCall() (Codegen.genComponentCall). Nhận
/// `registry` tường minh thay vì đọc biến toàn cục (xem ghi chú đầu
/// file); trả về (html, warning) — warning là None nếu component tồn
/// tại, Some(msg) nếu không tìm thấy định nghĩa (caller quyết định log
/// thế nào, xem error-handler.rs).
pub fn gen_component_call(
    node: &ComponentCall,
    registry: &ComponentRegistry,
    host: &mut dyn ElementCodegenHost,
) -> (String, Option<String>) {
    let def = match registry.get(&node.name) {
        Some(d) => d,
        None => {
            let warning = format!("[ViBao] Component \"{}\" chưa được định nghĩa với @the", node.name);
            return (format!("<!-- unknown component: {} -->", node.name), Some(warning));
        }
    };

    let props_js = node
        .props
        .iter()
        .map(|(k, v)| format!("{}: {}", k, expr_to_js_default(v)))
        .collect::<Vec<_>>()
        .join(", ");

    let id = host.next_id(&node.name);
    let children_html = host.gen_children(&def.children);

    host.add_js(&format!("__vb.mountComponent('{}', '{}', {{ {} }});", id, node.name, props_js));

    // data-vb-props chứa JSON của map {key: js_expr_string} — mỗi value
    // trong JSON là CHUỖI chứa mã JS (không phải giá trị đã eval), khớp
    // đúng `JSON.stringify(Object.fromEntries(...exprToJS(v)))` ở bản TS
    // cũ: runtime sẽ tự eval lại các chuỗi này khi hydrate component.
    let props_json = build_props_json(&node.props);

    let html = format!(
        "<div id=\"{}\" data-vb-component=\"{}\" data-vb-props=\"{}\">\n{}\n</div>",
        id,
        node.name,
        esc_attr(&props_json),
        indent2(&children_html)
    );

    (html, None)
}

/// Xây chuỗi JSON object {key: "js_expr_string", ...} từ PropsMap — mỗi
/// value là JS code dạng chuỗi (không phải giá trị runtime), y hệt cách
/// bản TS cũ dùng JSON.stringify(Object.fromEntries(entries.map(...))).
fn build_props_json(props: &vibao_ast::PropsMap) -> String {
    let entries = props
        .iter()
        .map(|(k, v)| format!("{}:{}", json_string(k), json_string(&expr_to_js_default(v))))
        .collect::<Vec<_>>()
        .join(",");
    format!("{{{}}}", entries)
}

/// Sinh định nghĩa JS runtime cho 1 @the component — chỉ khai báo hàm
/// nhận props (destructure theo tên tham số); phần render HTML thật đã
/// được xử lý tĩnh lúc biên dịch (mỗi lời gọi component render sẵn HTML
/// riêng ở gen_component_call, không cần render lại lúc runtime). Tương
/// đương genComponentDef().
pub fn gen_component_def(def: &ComponentDef) -> String {
    let param_names = def.params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join(", ");
    format!(
        "__vb.defineComponent('{}', function(__props) {{\n  const {{ {} }} = __props;\n  // render handled by HTML template\n}});",
        def.name, param_names
    )
}

// ════════════════════════════════════════════════════════════
// UNIT TESTS
// ════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use vibao_ast::{Child, Expr, ParamDef, DataType, Pos};

    fn p() -> Pos {
        Pos { line: 1, column: 1 }
    }

    struct FakeHost {
        counter: u32,
        js: Vec<String>,
    }

    impl FakeHost {
        fn new() -> Self {
            FakeHost { counter: 0, js: vec![] }
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
                    Child::Element(el) => crate::codegen::element::gen_element(el, self),
                    _ => String::new(),
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
        fn add_js(&mut self, code: &str) {
            self.js.push(code.to_string());
        }
        fn add_css(&mut self, _code: &str) {}
        fn add_media_query(&mut self, _code: &str) {}
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

    #[test]
    fn test_register_and_get() {
        let mut registry = ComponentRegistry::new();
        let def = ComponentDef { name: "The_Card".to_string(), params: vec![], children: vec![], pos: p() };
        registry.register(def);
        assert!(registry.has("The_Card"));
        assert!(registry.get("The_Card").is_some());
    }

    #[test]
    fn test_register_duplicate_warns_but_overwrites() {
        let mut registry = ComponentRegistry::new();
        registry.register(ComponentDef { name: "X".to_string(), params: vec![], children: vec![], pos: p() });
        registry.register(ComponentDef {
            name: "X".to_string(),
            params: vec![ParamDef { name: "a".to_string(), data_type: DataType::Chuoi, default_value: None, pos: p() }],
            children: vec![],
            pos: p(),
        });
        assert_eq!(registry.warnings.len(), 1);
        assert_eq!(registry.get("X").unwrap().params.len(), 1);
    }

    #[test]
    fn test_gen_component_call_unknown_returns_warning() {
        let registry = ComponentRegistry::new();
        let mut host = FakeHost::new();
        let call = ComponentCall { name: "KhongTonTai".to_string(), props: vec![], children: vec![], pos: p() };
        let (html, warning) = gen_component_call(&call, &registry, &mut host);
        assert!(html.contains("unknown component: KhongTonTai"));
        assert!(warning.is_some());
    }

    #[test]
    fn test_gen_component_call_known_generates_mount_js() {
        let mut registry = ComponentRegistry::new();
        registry.register(ComponentDef { name: "The_Card".to_string(), params: vec![], children: vec![], pos: p() });
        let mut host = FakeHost::new();
        let call = ComponentCall {
            name: "The_Card".to_string(),
            props: vec![("tieu_de".to_string(), Expr::literal_str("Xin chào", p()))],
            children: vec![],
            pos: p(),
        };
        let (html, warning) = gen_component_call(&call, &registry, &mut host);
        assert!(warning.is_none());
        assert!(html.contains("data-vb-component=\"The_Card\""));
        assert_eq!(host.js.len(), 1);
        assert!(host.js[0].contains("__vb.mountComponent"));
    }

    #[test]
    fn test_gen_component_def_shape() {
        let def = ComponentDef {
            name: "The_Card".to_string(),
            params: vec![
                ParamDef { name: "tieu_de".to_string(), data_type: DataType::Chuoi, default_value: None, pos: p() },
                ParamDef { name: "gia".to_string(), data_type: DataType::So, default_value: None, pos: p() },
            ],
            children: vec![],
            pos: p(),
        };
        let out = gen_component_def(&def);
        assert!(out.contains("__vb.defineComponent('The_Card'"));
        assert!(out.contains("const { tieu_de, gia } = __props;"));
    }
}
