// ============================================================
// VIBAO COMPILER (Rust) — codegen/mod.rs
// Điểm lắp ráp toàn bộ codegen: CodegenContext (trạng thái tích
// luỹ CSS/JS/state qua quá trình sinh), Codegen (struct chính,
// implement ElementCodegenHost để element.rs/control.rs có thể
// gọi ngược lại genChildren một cách đệ quy), và entry point
// generate(). Tương đương phần đầu + "1/2/3/4/8" của
// 11-codegen-core.ts.
// ============================================================

pub mod action;
pub mod component;
pub mod control;
pub mod css;
pub mod element;
pub mod expr;
pub mod layout;
pub mod props;

use vibao_ast::{App, Child, ColorFuncKind, ColorValue, Expr, Page, Program};
use crate::codegen::component::ComponentRegistry;
use crate::codegen::css::BASE_CSS;
use crate::codegen::element::ElementCodegenHost;
use crate::codegen::expr::expr_to_js_default;
use std::collections::HashMap;

// ════════════════════════════════════════════════════════════
// OPTIONS
// ════════════════════════════════════════════════════════════

/// Chế độ build — hiện chỉ có "spa" (single page app); giữ enum (thay vì
/// String) để compiler Rust bắt lỗi gõ nhầm tại compile-time, khác với
/// bản TS cũ dùng string "spa" tự do. Impl Default thủ công (thay vì
/// #[derive(Default)] + #[default] trên variant) để không phụ thuộc
/// rustc ≥ 1.62 — an toàn hơn khi build trên Termux với version rustc
/// không chắc chắn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BuildMode {
    Spa,
}

impl Default for BuildMode {
    fn default() -> Self {
        BuildMode::Spa
    }
}

#[derive(Debug, Clone)]
pub struct CodegenOptions {
    pub mode: BuildMode,
    pub minify: bool,
    pub source_map: bool,
    pub base_url: String,
}

impl Default for CodegenOptions {
    fn default() -> Self {
        CodegenOptions {
            mode: BuildMode::Spa,
            minify: false,
            source_map: false,
            base_url: "/".to_string(),
        }
    }
}

// ════════════════════════════════════════════════════════════
// CODEGEN CONTEXT — trạng thái tích luỹ trong lúc sinh code
// ════════════════════════════════════════════════════════════

pub struct CodegenContext {
    pub options: CodegenOptions,
    id_counter: u32,
    css_blocks: Vec<String>,
    js_blocks: Vec<String>,
    media_queries: Vec<String>,
    /// Dùng Vec<(String, Expr)> thay vì HashMap để giữ thứ tự khai báo
    /// state — quan trọng khi in ra JS (thứ tự khai báo biến ảnh hưởng
    /// tính dễ đọc, và ở 1 số ngôn ngữ scripting có thể ảnh hưởng thực
    /// thi nếu biến sau phụ thuộc biến trước — dù JS hiện hoisting nên
    /// không bắt buộc, giữ thứ tự vẫn là lựa chọn an toàn hơn HashMap).
    state_vars: Vec<(String, Expr)>,
    global_vars: Vec<(String, Expr)>,
    component_scope: Option<String>,
}

impl CodegenContext {
    pub fn new(options: CodegenOptions) -> Self {
        CodegenContext {
            options,
            id_counter: 0,
            css_blocks: Vec::new(),
            js_blocks: Vec::new(),
            media_queries: Vec::new(),
            state_vars: Vec::new(),
            global_vars: Vec::new(),
            component_scope: None,
        }
    }

    pub fn next_id(&mut self, tag: &str) -> String {
        self.id_counter += 1;
        format!("vb-{}-{}", tag, self.id_counter)
    }

    pub fn add_css(&mut self, block: &str) {
        if !block.trim().is_empty() {
            self.css_blocks.push(block.to_string());
        }
    }

    pub fn add_media_query(&mut self, mq: &str) {
        if !mq.trim().is_empty() {
            self.media_queries.push(mq.to_string());
        }
    }

    pub fn get_css(&self) -> String {
        self.css_blocks.iter().chain(self.media_queries.iter()).cloned().collect::<Vec<_>>().join("\n\n")
    }

    pub fn add_js(&mut self, block: &str) {
        if !block.trim().is_empty() {
            self.js_blocks.push(block.to_string());
        }
    }

    pub fn get_js(&self) -> String {
        self.js_blocks.join("\n\n")
    }

    pub fn add_state_var(&mut self, name: &str, val: &Expr) {
        self.state_vars.push((name.to_string(), val.clone()));
    }

    pub fn add_global_var(&mut self, name: &str, val: &Expr) {
        self.global_vars.push((name.to_string(), val.clone()));
    }

    pub fn get_state_vars(&self) -> &[(String, Expr)] {
        &self.state_vars
    }

    pub fn get_global_vars(&self) -> &[(String, Expr)] {
        &self.global_vars
    }

    pub fn set_scope(&mut self, name: Option<String>) {
        self.component_scope = name;
    }

    pub fn get_scope(&self) -> Option<&str> {
        self.component_scope.as_deref()
    }

    /// Reset trạng thái riêng của từng trang (JS blocks, state vars)
    /// trước khi bắt đầu sinh 1 trang mới — CSS/JS blocks toàn cục
    /// (BASE_CSS, global vars) KHÔNG bị reset.
    ///
    /// QUAN TRỌNG: KHÔNG reset `id_counter` ở đây (đã sửa — trước đây có
    /// `self.id_counter = 0`, đây là 1 BUG THẬT phát hiện qua build thử
    /// 1 app nhiều trang: vì kiến trúc build hiện tại là SPA THẬT (mọi
    /// trang gộp chung 1 index.html, xem main.rs::cmd_build), reset
    /// id_counter về 0 mỗi trang khiến MỌI trang đều sinh ra id trùng
    /// nhau tuyệt đối (vd "vb-box-1" xuất hiện ở cả trang "/" lẫn
    /// "/gioi-thieu" trong CÙNG 1 DOM). Hậu quả: mọi
    /// document.getElementById()/querySelector("#...") chỉ trúng đúng
    /// phần tử của trang ĐẦU TIÊN sinh ra, khiến binding/style của các
    /// trang sau bị "vỡ" một cách âm thầm (không báo lỗi gì, chỉ chạy
    /// sai). id_counter giờ đếm LIÊN TỤC xuyên suốt toàn bộ app, đảm
    /// bảo id là duy nhất toàn cục — đúng yêu cầu bắt buộc của HTML khi
    /// nhiều "trang" cùng tồn tại trong 1 DOM thật.
    pub fn reset_page(&mut self) {
        self.js_blocks.clear();
        self.state_vars.clear();
    }
}

// ════════════════════════════════════════════════════════════
// OUTPUT — kết quả cuối cùng của generate()
// ════════════════════════════════════════════════════════════

pub struct CodegenOutput {
    /// HTML của trang "/" (hoặc trang đầu tiên nếu không có "/").
    pub html: String,
    pub css: String,
    pub js: String,
    /// HTML của TỪNG trang theo route — dùng khi build multi-page.
    pub pages: HashMap<String, String>,
    /// Cảnh báo tích luỹ trong lúc sinh (component trùng tên, gọi
    /// component chưa định nghĩa...) — caller (CLI/bundler) quyết định
    /// hiển thị. Khác bản TS cũ (console.warn trực tiếp), xem ghi chú ở
    /// component.rs.
    pub warnings: Vec<String>,
}

// ════════════════════════════════════════════════════════════
// CODEGEN — struct chính
// ════════════════════════════════════════════════════════════

pub struct Codegen {
    pub ctx: CodegenContext,
    registry: ComponentRegistry,
}

impl Codegen {
    pub fn new(options: CodegenOptions) -> Self {
        Codegen { ctx: CodegenContext::new(options), registry: ComponentRegistry::new() }
    }

    /// Điểm vào chính — biên dịch toàn bộ Program thành HTML/CSS/JS.
    /// Tương đương Codegen.generate().
    pub fn generate(&mut self, program: &Program) -> CodegenOutput {
        let app = &program.app;

        for def in &app.components {
            self.registry.register(def.clone());
        }
        for v in &app.variables {
            self.ctx.add_global_var(&v.name, &v.value);
        }
        self.ctx.add_css(BASE_CSS);

        let mut pages: HashMap<String, String> = HashMap::new();
        for page in &app.pages {
            self.ctx.reset_page();
            let html = self.gen_page(page);
            pages.insert(page.route.clone(), html);
        }

        let html = pages.get("/").cloned().unwrap_or_else(|| pages.values().next().cloned().unwrap_or_default());
        let js = self.gen_app_js(app);
        let warnings = self.registry.warnings.clone();

        CodegenOutput { html, css: self.ctx.get_css(), js, pages, warnings }
    }

    // ════════════════════════════════════════════════════════
    // PAGE GENERATION
    // ════════════════════════════════════════════════════════

    fn gen_page(&mut self, page: &Page) -> String {
        for s in &page.states {
            self.ctx.add_state_var(&s.name, &s.value);
        }
        // on_tai/on_huy giờ đi qua action registry (Rust thuần), KHÔNG
        // còn sinh JS qua compile_page_load() (kiến trúc cũ) — 2 action
        // id (nếu có) được nhúng thẳng vào chính div .vb-page dưới dạng
        // data-vb-on-tai/data-vb-on-huy; router.rs (runtime) tự đọc và
        // dispatch đúng lúc trang được activate/rời khỏi.
        let (id_on_tai, id_on_huy) = action::compile_page_load_registry(&page.events);
        let lifecycle_attrs = [
            id_on_tai.map(|id| format!("data-vb-on-tai=\"{}\"", id)),
            id_on_huy.map(|id| format!("data-vb-on-huy=\"{}\"", id)),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");

        let children_html = page
            .children
            .iter()
            .map(|c| self.gen_child(c))
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("\n");

        let bg_style = page
            .mau_nen
            .as_ref()
            .map(|cv| format!("style=\"background-color:{}\"", resolve_page_bg_color(cv)))
            .unwrap_or_default();

        format!(
            "<div class=\"vb-page\" data-route=\"{}\" {} {}>\n{}\n</div>",
            page.route,
            bg_style,
            lifecycle_attrs,
            css::indent2(&children_html)
        )
    }

    // ════════════════════════════════════════════════════════
    // CHILD DISPATCH
    // ════════════════════════════════════════════════════════

    /// Sinh HTML cho 1 Child — phân luồng theo biến thể. Tương đương
    /// genChild(). StateDecl/VarDecl/PageEvent không sinh HTML (chỉ có
    /// tác dụng phụ lên ctx), trả về "" để bị filter ở nơi gọi.
    pub fn gen_child(&mut self, child: &Child) -> String {
        match child {
            Child::StateDecl(s) => {
                self.ctx.add_state_var(&s.name, &s.value);
                String::new()
            }
            Child::VarDecl(v) => {
                self.ctx.add_global_var(&v.name, &v.value);
                String::new()
            }
            Child::PageEvent(pe) => {
                let js = action::compile_page_load(std::slice::from_ref(pe));
                self.ctx.add_js(&js);
                String::new()
            }
            Child::If(node) => control::gen_if(node, self),
            Child::Switch(node) => control::gen_switch(node, self),
            Child::Loop(node) => control::gen_loop(node, self),
            Child::Element(el) => element::gen_element(el, self),
            Child::ComponentCall(call) => {
                // LƯU Ý BORROW CHECKER: không thể gọi
                // component::gen_component_call(call, &self.registry, self)
                // trực tiếp — Rust sẽ từ chối vì vừa mượn bất biến
                // self.registry vừa mượn khả biến self (do tham số host:
                // &mut dyn ElementCodegenHost cần &mut self). Giải pháp:
                // tạm thời "mượn tay" (take) registry ra khỏi self bằng
                // std::mem::take (registry impl Default), gọi hàm với
                // self như &mut host bình thường, rồi trả registry lại.
                // An toàn vì không có code nào khác truy cập self.registry
                // trong lúc nó rỗng (chỉ tồn tại trong phạm vi khối này).
                let registry = std::mem::take(&mut self.registry);
                let (html, warning) = component::gen_component_call(call, &registry, self);
                self.registry = registry;
                if let Some(w) = warning {
                    self.registry.warnings.push(w);
                }
                html
            }
        }
    }

    fn gen_children_internal(&mut self, children: &[Child]) -> String {
        children.iter().map(|c| self.gen_child(c)).filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n")
    }

    // ════════════════════════════════════════════════════════
    // APP JS GENERATION
    // ════════════════════════════════════════════════════════

    fn gen_app_js(&self, app: &App) -> String {
        let vars_js = self
            .ctx
            .get_global_vars()
            .iter()
            .map(|(k, v)| format!("  const {} = {};", k, expr_to_js_default(v)))
            .collect::<Vec<_>>()
            .join("\n");

        let component_defs = app
            .components
            .iter()
            .map(component::gen_component_def)
            .collect::<Vec<_>>()
            .join("\n\n");

        // Lấy toàn bộ Expr đã đăng ký qua expr_to_js_registry() trong suốt
        // lượt build trang này (nếu có nơi nào dùng), serialize ra JSON để
        // nhúng vào __vb.boot(). WASM đọc mảng này lúc khởi động, deserialize
        // lại thành Vec<Expr> thật, và evaluator Rust (vibao-runtime) tính
        // theo đúng index đã dùng ở "__vb.evalExpr(<id>)" trong output JS.
        // Nếu chưa nơi nào dùng expr_to_js_registry() (mặc định hiện tại,
        // vì mọi nơi khác vẫn gọi expr_to_js_default() như cũ), mảng này
        // rỗng ("[]") — hoàn toàn vô hại, không ảnh hưởng hành vi cũ.
        let expr_registry = expr::take_expr_registry();
        let expr_registry_json = serde_json::to_string(&expr_registry)
            .unwrap_or_else(|_| "[]".to_string());

        // Đối xứng expr_registry — mỗi phần tử là Vec<Action> (thân 1
        // event handler), đăng ký qua action::compile_event_handler_registry()
        // (gọi từ element.rs khi sinh HTML cho 1 element có event). WASM
        // (action_registry.rs) đọc mảng này lúc boot, tra theo id nhúng ở
        // "data-vb-on-click='<id>'" trong HTML.
        let action_registry = action::take_action_registry();
        let action_registry_json = serde_json::to_string(&action_registry)
            .unwrap_or_else(|_| "[]".to_string());

        // ── WASM BOOTSTRAP ──────────────────────────────────────────
        // `wasm-bindgen` sinh ra 1 file JS "glue" riêng (vd
        // "vibao_runtime.js") lúc build bằng `wasm-bindgen-cli`, KHÔNG
        // phải lúc chạy `vibaoc` — file đó export 1 hàm khởi tạo mặc
        // định (default export, gọi là `init`) để nạp file ".wasm" đi
        // kèm, và export class `VbRuntime` (đã khai `#[wasm_bindgen]`
        // ở dom.rs).
        //
        // QUAN TRỌNG: `output.js` (kết quả hàm này) được nhúng vào HTML
        // dạng `<script>...</script>` THƯỜNG (classic script), không
        // phải `<script type="module">` — main.rs/CLI chỉ in JS thuần,
        // không tự quyết định thẻ script bọc ngoài. Vì vậy KHÔNG dùng
        // cú pháp `import ... from ...` tĩnh ở đây (chỉ hợp lệ trong
        // module script) — thay vào đó dùng `import()` ĐỘNG (dynamic
        // import, là 1 HÀM trả Promise), cú pháp này hợp lệ trong cả
        // classic script lẫn module script.
        //
        // Đường dẫn giả định layout output chuẩn của `wasm-pack build`:
        //   pkg/vibao_runtime.js
        //   pkg/vibao_runtime_bg.wasm
        // Nếu dự án thật đặt file ở vị trí khác, sửa hằng số
        // WASM_JS_PATH này (không cần sửa gì khác trong codegen).
        const WASM_JS_PATH: &str = "./pkg/vibao_runtime.js";

        let bootstrap_js = format!(
            r#"
// ── WASM Bootstrap (dynamic import — hoạt động trong classic script) ──
(async function __vbBoot() {{
  try {{
    const wasmModule = await import("{wasm_path}");
    await wasmModule.default(); // nạp file .wasm đi kèm, chờ WASM sẵn sàng
    const optsJson = JSON.stringify({{
      baseURL: "{base_url}",
      exprRegistry: {expr_registry_json},
      actionRegistry: {action_registry_json}
    }});
    window.__vbRuntime = new wasmModule.VbRuntime(optsJson); // giữ tham chiếu để debug console
  }} catch (err) {{
    console.error("[ViBao] Không khởi động được runtime:", err);
  }}
}})();"#,
            wasm_path = WASM_JS_PATH,
            base_url = self.ctx.options.base_url,
            expr_registry_json = expr_registry_json,
            action_registry_json = action_registry_json,
        );

        format!(
            "// ViBao Generated JS — DO NOT EDIT\n(function() {{\n'use strict';\n\n// ── Global vars ──\n{}\n\n// ── Components ──\n{}\n\n// ── App init ──\n{}\n}})();\n{}",
            vars_js,
            component_defs,
            self.ctx.get_js(),
            bootstrap_js,
        )
    }
}

// ════════════════════════════════════════════════════════════
// ElementCodegenHost — cho phép element.rs/control.rs/component.rs gọi
// ngược lại Codegen::gen_child() một cách đệ quy mà không tạo vòng lặp
// import giữa các module con và mod.rs.
// ════════════════════════════════════════════════════════════

impl ElementCodegenHost for Codegen {
    fn next_id(&mut self, tag: &str) -> String {
        self.ctx.next_id(tag)
    }

    fn gen_children(&mut self, children: &[Child]) -> String {
        self.gen_children_internal(children)
    }

    fn add_js(&mut self, code: &str) {
        self.ctx.add_js(code);
    }

    fn add_css(&mut self, code: &str) {
        self.ctx.add_css(code);
    }

    fn add_media_query(&mut self, code: &str) {
        self.ctx.add_media_query(code);
    }

    fn add_warning(&mut self, msg: String) {
        self.registry.warnings.push(msg);
    }

    fn compile_event_handler(&self, event: &vibao_ast::EventNode, id: &str) -> String {
        action::compile_event_handler(event, id)
    }

    fn compile_hover_animation(&self, id: &str, effect: &str, duration_ms: u32) -> String {
        action::compile_hover_animation(id, effect, duration_ms)
    }

    fn compile_scroll_animation(&self, id: &str, effect: &str, duration_ms: u32, delay_ms: u32) -> String {
        action::compile_scroll_animation(id, effect, duration_ms, delay_ms)
    }
}

// ════════════════════════════════════════════════════════════
// PAGE BACKGROUND COLOR RESOLUTION
// ════════════════════════════════════════════════════════════

/// Resolve 1 ColorValue (dùng riêng cho mau_nen cấp trang) thành chuỗi
/// CSS. Tương đương resolvePageBgColor().
pub fn resolve_page_bg_color(cv: &ColorValue) -> String {
    match cv {
        ColorValue::Hex(hex) => hex.clone(),
        ColorValue::Name(name) => resolve_color_name(name),
        ColorValue::Variable(name) => format!("var(--{})", name.replace('_', "-")),
        ColorValue::Func { func, args } => {
            let args_str = args
                .iter()
                .map(|a| expr::get_static_value(a))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}({})", color_func_name(*func), args_str)
        }
    }
}

fn color_func_name(func: ColorFuncKind) -> &'static str {
    match func {
        ColorFuncKind::TrongSuot => "trong_suot",
        ColorFuncKind::LamSang => "lam_sang",
        ColorFuncKind::LamToi => "lam_toi",
    }
}

/// Bảng tên màu tiếng Việt → mã hex — tương đương resolveColorName2() ở
/// bản TS cũ (đặt tên khác resolveColorName ở lexer/expr để tránh trùng,
/// dù cùng bảng dữ liệu — đây thực chất là 1 bảng màu DUY NHẤT bị lặp
/// định nghĩa 2 nơi trong bản TS gốc do cách bundler gộp code; ở Rust ta
/// giữ đúng 1 bảng const để tránh out-of-sync giữa 2 nơi).
pub const COLOR_NAME_MAP: [(&str, &str); 14] = [
    ("trang", "#FFFFFF"),
    ("den", "#000000"),
    ("do", "#E53E3E"),
    ("xanh", "#3182CE"),
    ("xanh_la", "#38A169"),
    ("vang", "#F59E0B"),
    ("hong", "#D53F8C"),
    ("tim", "#805AD5"),
    ("cam", "#DD6B20"),
    ("xam", "#718096"),
    ("xam_nhat", "#F7FAFC"),
    ("xam_dam", "#2D3748"),
    ("luc", "#25855A"),
    ("nau", "#7B341E"),
];

pub fn resolve_color_name(name: &str) -> String {
    COLOR_NAME_MAP
        .iter()
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_string())
        .unwrap_or_else(|| name.to_string())
}

// ════════════════════════════════════════════════════════════
// UNIT TESTS
// ════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use vibao_ast::Pos;

    fn p() -> Pos {
        Pos { line: 1, column: 1 }
    }

    #[test]
    fn test_resolve_color_name_known() {
        assert_eq!(resolve_color_name("do"), "#E53E3E");
    }

    #[test]
    fn test_resolve_color_name_unknown_passthrough() {
        assert_eq!(resolve_color_name("mau_la_theo"), "mau_la_theo");
    }

    #[test]
    fn test_resolve_page_bg_color_hex() {
        assert_eq!(resolve_page_bg_color(&ColorValue::Hex("#123456".to_string())), "#123456");
    }

    #[test]
    fn test_resolve_page_bg_color_variable() {
        assert_eq!(
            resolve_page_bg_color(&ColorValue::Variable("mau_chinh".to_string())),
            "var(--mau-chinh)"
        );
    }

    #[test]
    fn test_context_next_id_increments() {
        let mut ctx = CodegenContext::new(CodegenOptions::default());
        assert_eq!(ctx.next_id("box"), "vb-box-1");
        assert_eq!(ctx.next_id("box"), "vb-box-2");
    }

    #[test]
    fn test_context_reset_page_clears_js_but_keeps_id_counter_global() {
        // ĐÃ SỬA (trước đây test này assert id_counter RESET về 1 sau
        // reset_page() — đó chính là hành vi BUG đã gây lỗi id trùng lặp
        // giữa các trang trong 1 app SPA thật nhiều trang, phát hiện qua
        // build thử thực tế. Test giờ assert đúng hành vi ĐÃ SỬA:
        // id_counter phải tiếp tục tăng liên tục xuyên suốt nhiều trang,
        // không reset — đảm bảo id duy nhất toàn cục trong SPA.
        let mut ctx = CodegenContext::new(CodegenOptions::default());
        ctx.next_id("box"); // "vb-box-1", giả lập đang sinh trang 1
        ctx.add_js("some_js();");
        ctx.add_global_var("g", &Expr::literal_num(1.0, p()));
        ctx.reset_page(); // chuyển sang sinh trang 2
        assert_eq!(ctx.next_id("box"), "vb-box-2"); // KHÔNG reset — tiếp tục đếm
        assert_eq!(ctx.get_js(), ""); // js cleared đúng (riêng theo từng trang)
        assert_eq!(ctx.get_global_vars().len(), 1); // global vars vẫn còn
    }

    #[test]
    fn test_multi_page_ids_never_collide() {
        // Test hồi quy trực tiếp cho bug đã sửa: mô phỏng sinh id cho 2
        // "trang" liên tiếp (gọi reset_page() giữa chừng, giống generate()
        // thật làm với mỗi page trong app.pages), xác nhận không id nào
        // trùng nhau — đây chính là điều kiện bắt buộc để
        // document.getElementById() trong 1 SPA nhiều trang không bao giờ
        // trúng nhầm phần tử của trang khác.
        let mut ctx = CodegenContext::new(CodegenOptions::default());
        let mut all_ids = std::collections::HashSet::new();

        for _page in 0..3 {
            for _el in 0..5 {
                let id = ctx.next_id("box");
                assert!(all_ids.insert(id.clone()), "Id bị trùng: {}", id);
            }
            ctx.reset_page();
        }
        assert_eq!(all_ids.len(), 15); // 3 trang * 5 phần tử, không trùng cái nào
    }

    #[test]
    fn test_generate_empty_program_has_base_css() {
        let program = Program {
            app: App {
                name: "test".to_string(),
                variables: vec![],
                themes: vec![],
                components: vec![],
                pages: vec![],
                pos: p(),
            },
        };
        let mut codegen = Codegen::new(CodegenOptions::default());
        let out = codegen.generate(&program);
        assert!(out.css.contains("ViBao Base CSS"));
    }
}
