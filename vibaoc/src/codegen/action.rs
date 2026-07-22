// ============================================================
// VIBAO COMPILER (Rust) — codegen/action.rs
// Biên dịch Action (lệnh bên trong khối sự kiện: nhan_click,
// on_tai, ...) thành statement JS thực thi được. Tương đương phần
// action-codegen của 10-parser-event.ts (compileAction/
// compileAssign/compileFunctionCall/compileApiCall/compileIfAction/
// compileEventHandler/compilePageLoad).
//
// ── GHI CHÚ VỀ ACTION REGISTRY (Rust/WASM dispatcher) ──────────────
// Toàn bộ hàm compile_*() PHÍA TRÊN (JS-emitting, giữ NGUYÊN không đổi)
// là cách làm GỐC — sinh JS thuần, gọi các hàm runtime JS (__vb.toast,
// __api.call...) KHÔNG CÒN TỒN TẠI kể từ khi runtime chuyển hẳn sang
// Rust/WASM (xem vibao-runtime::action). Các hàm này được GIỮ LẠI
// nguyên vẹn (không xoá) chỉ để không phá 14 test hiện có đang assert
// đúng chuỗi JS cụ thể — KHÔNG NÊN dùng chúng cho pipeline build thật
// nữa.
//
// Thay vào đó, `compile_event_handler_registry()` (hàm MỚI, cuối file)
// là đường dẫn ĐÚNG cho pipeline hiện tại: đăng ký `Vec<Action>` (thân
// event handler) vào 1 registry nội bộ (thread_local, giống hệt cơ chế
// expr registry ở expr.rs), trả về HTML attribute
// `data-vb-on-<dom-event>="<actionId>"` để nhúng thẳng vào element —
// KHÔNG sinh JS nào cả. Runtime WASM (dom.rs::bind_events) đọc action
// id này, tra registry, thực thi bằng action::dispatch_all() (Rust
// thuần) khi người dùng bấm.
// ============================================================

use std::cell::RefCell;

use vibao_ast::{Action, EventName, PageEvent, PageEventName};
use crate::codegen::css::indent;
use crate::codegen::expr::expr_to_js_default;

thread_local! {
    /// Bảng tích luỹ toàn bộ "thân event handler" (Vec<Action>) đã đăng
    /// ký trong lượt build hiện tại. Index trong Vec CHÍNH LÀ action id
    /// dùng ở "data-vb-on-click='<id>'" — đối xứng hoàn toàn với
    /// EXPR_REGISTRY ở expr.rs.
    static ACTION_REGISTRY: RefCell<Vec<Vec<Action>>> = RefCell::new(Vec::new());
}

/// Đăng ký 1 chuỗi Action (thân 1 event handler) vào registry, trả về
/// id để nhúng vào HTML như "data-vb-on-<event>=\"<id>\"".
pub fn register_action_body(actions: Vec<Action>) -> usize {
    ACTION_REGISTRY.with(|reg| {
        let mut reg = reg.borrow_mut();
        reg.push(actions);
        reg.len() - 1
    })
}

/// Lấy toàn bộ registry đã tích luỹ VÀ xoá sạch (reset cho lượt build
/// tiếp theo) — gọi ở cuối `gen_app_js`, cùng lúc với `take_expr_registry()`.
pub fn take_action_registry() -> Vec<Vec<Action>> {
    ACTION_REGISTRY.with(|reg| std::mem::take(&mut *reg.borrow_mut()))
}

/// Ánh xạ tên sự kiện ViBao (EventName enum) sang tên sự kiện DOM thật.
/// Khớp EVENT_DOM_MAP ở bản TS cũ.
fn event_to_dom(name: &EventName) -> &'static str {
    match name {
        EventName::OnClick => "click",
        EventName::OnHover => "mouseenter",
        EventName::OnBlur => "blur",
        EventName::OnFocus => "focus",
        EventName::OnChange => "change",
        EventName::OnSubmit => "submit",
        EventName::OnScroll => "scroll",
    }
}

/// Tên định danh dùng để đặt tên hàm handler duy nhất — vd "on_click".
fn event_name_ident(name: &EventName) -> &'static str {
    match name {
        EventName::OnClick => "on_click",
        EventName::OnHover => "on_hover",
        EventName::OnBlur => "on_blur",
        EventName::OnFocus => "on_focus",
        EventName::OnChange => "on_change",
        EventName::OnSubmit => "on_submit",
        EventName::OnScroll => "on_scroll",
    }
}

// ════════════════════════════════════════════════════════════
// EVENT HANDLER — REGISTRY (đường dẫn ĐÚNG cho pipeline hiện tại)
// ════════════════════════════════════════════════════════════

/// Biên dịch 1 EventNode thành 1 HTML attribute
/// `data-vb-on-<dom-event>="<actionId>"`, KHÔNG sinh JS. Đây là hàm
/// THAY THẾ `compile_event_handler()` (JS-emitting, ở trên) cho pipeline
/// build thật — xem ghi chú đầu file.
///
/// Trả về `(tên_attribute, giá_trị)` thay vì 1 chuỗi HTML hoàn chỉnh, để
/// caller (element.rs) tự quyết định cách chèn vào chuỗi attribute
/// đang xây dựng (giữ nguyên style hiện có của element.rs, không áp đặt
/// format string ở đây).
pub fn compile_event_handler_registry(event: &vibao_ast::EventNode) -> (String, String) {
    let dom_event = event_to_dom(&event.name);
    let attr_name = format!("data-vb-on-{}", dom_event);
    let action_id = register_action_body(event.body.clone());
    (attr_name, action_id.to_string())
}

// ════════════════════════════════════════════════════════════
// COMPILE 1 ACTION → JS STATEMENT
// ════════════════════════════════════════════════════════════

/// Biên dịch 1 Action thành 1 (hoặc nhiều dòng) statement JS. Tương
/// đương compileAction(). `depth` chỉ ảnh hưởng IfAction (độ sâu lồng
/// nhau, dùng để tương thích chữ ký gốc dù hiện thân hàm dùng indent()
/// tương đối chứ không dựa trực tiếp vào depth để tính số khoảng trắng).
pub fn compile_action(action: &Action, depth: usize) -> String {
    match action {
        Action::Assign { .. } => compile_assign(action),
        Action::FunctionCall { .. } => compile_function_call(action),
        Action::ApiCall { .. } => compile_api_call(action),
        Action::IfAction { .. } => compile_if_action(action, depth),
    }
}

/// Biên dịch 1 danh sách Action liên tiếp, tuỳ chọn bọc trong IIFE async.
/// Tương đương compileActions().
pub fn compile_actions(actions: &[Action], depth: usize, wrap_async: bool) -> String {
    let body = actions.iter().map(|a| compile_action(a, depth)).collect::<Vec<_>>().join("\n");
    if wrap_async {
        wrap_async_iife(&body)
    } else {
        body
    }
}

fn wrap_async_iife(body: &str) -> String {
    format!("(async () => {{\n{}\n}})()", indent(body, 2))
}

// ════════════════════════════════════════════════════════════
// ASSIGN ($x = ...)
// ════════════════════════════════════════════════════════════

fn compile_assign(action: &Action) -> String {
    let (target, value) = match action {
        Action::Assign { target, value, .. } => (target, value),
        _ => unreachable!("compile_assign chỉ nhận Action::Assign"),
    };

    // Nếu vế phải là 1 Call trên method mảng/object của chính state đó
    // (vd $ds.them(x) — hiện PARSER RUST CHƯA sinh ra dạng Call này, chỉ
    // dừng ở MemberAccess vì parse_variable() không tiếp tục đọc dấu '('
    // sau '.field'; xem parser/expr.rs), ta vẫn giữ logic dịch đúng ở
    // đây để khi parser được mở rộng sau này (hỗ trợ $var.method(args)),
    // codegen không cần sửa gì thêm. Với parser hiện tại, nhánh này chỉ
    // kích hoạt nếu callee của Call chứa dấu '.' (vd "ds.them").
    if let vibao_ast::Expr::Call { callee, .. } = value {
        if callee.contains('.') {
            return compile_state_mutation(target, value);
        }
    }

    format!("__setState('{}', {});", target, expr_to_js_default(value))
}

/// Biên dịch phép gán dạng "method mutation" ($ds = $ds.them(x)) thành
/// lệnh mutation runtime tương ứng. Tương đương compileStateMutation().
/// Xem ghi chú ở compile_assign() về giới hạn hiện tại của parser.
fn compile_state_mutation(target: &str, call: &vibao_ast::Expr) -> String {
    let (callee, args) = match call {
        vibao_ast::Expr::Call { callee, args, .. } => (callee, args),
        _ => unreachable!("compile_state_mutation chỉ nhận Expr::Call"),
    };
    let method = callee.rsplit('.').next().unwrap_or(callee);
    let args_js = args.iter().map(expr_to_js_default).collect::<Vec<_>>().join(", ");

    match method {
        "them" => format!("__statePush('{}', {});", target, args_js),
        "xoa" => format!("__stateRemove('{}', {});", target, args_js),
        "xoa_het" => format!("__setState('{}', []);", target),
        "cap_nhat" => format!("__stateUpdate('{}', {});", target, args_js),
        _ => format!("__setState('{}', {});", target, expr_to_js_default(call)),
    }
}

// ════════════════════════════════════════════════════════════
// FUNCTION CALL (dieu_huong(...), thong_bao(...), custom_fn(...), ...)
// ════════════════════════════════════════════════════════════

/// Ánh xạ tên hành động built-in tiếng Việt sang hàm JS runtime tương
/// ứng. Tên không có trong bảng được giữ nguyên (custom function do
/// người dùng định nghĩa). Khớp fnMap ở bản TS cũ.
fn map_action_fn(name: &str) -> &str {
    match name {
        "dieu_huong" => "__router.push",
        "mo_tab_moi" => "__router.openTab",
        "mo_modal" => "__vb.openModal",
        "dong_modal" => "__vb.closeModal",
        "cuon_den" => "__vb.scrollTo",
        "cuon_len_dau" => "__vb.scrollTop",
        "thong_bao" => "__vb.toast",
        "canh_bao" => "__vb.alert",
        "dang_xuat" => "__auth.logout",
        "sao_chep" => "__vb.copyText",
        "luu_du_lieu" => "__vb.save",
        "tai_du_lieu" => "__vb.load",
        other => other,
    }
}

/// Danh sách hành động cần `await` khi gọi (bất đồng bộ) — khớp
/// isAsyncAction() ở bản TS cũ.
fn is_async_action(name: &str) -> bool {
    matches!(name, "goi_api" | "tai_du_lieu" | "luu_du_lieu" | "dang_xuat" | "dang_nhap")
}

fn compile_function_call(action: &Action) -> String {
    let (name, args, opts, assign_to) = match action {
        Action::FunctionCall { name, args, opts, assign_to, .. } => (name, args, opts, assign_to),
        _ => unreachable!("compile_function_call chỉ nhận Action::FunctionCall"),
    };

    let args_js = args.iter().map(expr_to_js_default).collect::<Vec<_>>().join(", ");
    let opts_js = if !opts.is_empty() {
        let entries = opts.iter().map(|(k, v)| format!("{}: {}", k, expr_to_js_default(v))).collect::<Vec<_>>().join(", ");
        format!(", {{ {} }}", entries)
    } else {
        String::new()
    };

    let js_fn = map_action_fn(name);
    let call = format!("{}({}{})", js_fn, args_js, opts_js);

    if let Some(target) = assign_to {
        return format!("__setState('{}', await {});", target, call);
    }

    let needs_await = is_async_action(name);
    format!("{}{};", if needs_await { "await " } else { "" }, call)
}

// ════════════════════════════════════════════════════════════
// API CALL (goi_api(...))
// ════════════════════════════════════════════════════════════

fn compile_api_call(action: &Action) -> String {
    let (method, endpoint, data, assign_to, on_success, on_failure) = match action {
        Action::ApiCall { method, endpoint, data, assign_to, on_success, on_failure, .. } => {
            (method, endpoint, data, assign_to, on_success, on_failure)
        }
        _ => unreachable!("compile_api_call chỉ nhận Action::ApiCall"),
    };

    let method_js = crate::codegen::expr::json_string(method);
    let endpoint_js = expr_to_js_default(endpoint);
    let data_js = data.as_ref().map(expr_to_js_default).unwrap_or_else(|| "undefined".to_string());

    let mut code = format!("const __apiResult = await __api.call({}, {}, {});\n", method_js, endpoint_js, data_js);

    if let Some(target) = assign_to {
        code.push_str(&format!("__setState('{}', __apiResult);\n", target));
    }

    if let Some(success) = on_success {
        if !success.is_empty() {
            let success_js = success.iter().map(|a| compile_action(a, 1)).collect::<Vec<_>>().join("\n");
            code.push_str(&format!("if (__apiResult.__ok) {{\n{}\n}}\n", indent(&success_js, 2)));
        }
    }
    if let Some(failure) = on_failure {
        if !failure.is_empty() {
            let fail_js = failure.iter().map(|a| compile_action(a, 1)).collect::<Vec<_>>().join("\n");
            code.push_str(&format!("else {{\n{}\n}}\n", indent(&fail_js, 2)));
        }
    }

    code.trim().to_string()
}

// ════════════════════════════════════════════════════════════
// IF ACTION (neu ... khong_thi ... bên trong 1 sự kiện)
// ════════════════════════════════════════════════════════════

fn compile_if_action(action: &Action, depth: usize) -> String {
    let (condition, consequent, alternate) = match action {
        Action::IfAction { condition, consequent, alternate, .. } => (condition, consequent, alternate),
        _ => unreachable!("compile_if_action chỉ nhận Action::IfAction"),
    };

    let cond_js = expr_to_js_default(condition);
    let then_js = consequent.iter().map(|a| compile_action(a, depth + 1)).collect::<Vec<_>>().join("\n");
    let mut code = format!("if ({}) {{\n{}\n}}", cond_js, indent(&then_js, 2));

    if let Some(alt) = alternate {
        if !alt.is_empty() {
            let else_js = alt.iter().map(|a| compile_action(a, depth + 1)).collect::<Vec<_>>().join("\n");
            code.push_str(&format!(" else {{\n{}\n}}", indent(&else_js, 2)));
        }
    }

    code
}

// ════════════════════════════════════════════════════════════
// EVENT HANDLER WRAPPING (nhan_click { ... } trên 1 Element)
// ════════════════════════════════════════════════════════════

/// True nếu action cần await ở cấp trên (dùng để quyết định handler có
/// nên khai báo `async function` hay không). Tương đương needsAsync().
fn needs_async(action: &Action) -> bool {
    match action {
        Action::ApiCall { .. } => true,
        Action::FunctionCall { name, .. } => is_async_action(name),
        _ => false,
    }
}

/// Biên dịch 1 EventNode gắn trên Element (vd nhan_click) thành khối JS
/// hoàn chỉnh: định nghĩa hàm handler + đăng ký addEventListener. Tương
/// đương compileEventHandler().
pub fn compile_event_handler(event: &vibao_ast::EventNode, element_id: &str) -> String {
    let dom_event = event_to_dom(&event.name);
    let event_ident = event_name_ident(&event.name);
    let has_async = event.body.iter().any(needs_async);
    let body_js = compile_actions(&event.body, 1, false);

    // LƯU Ý: bản TS gốc dùng nguyên `elementId` (dạng "vb-button-1", có
    // dấu "-") để ghép thành tên hàm `__handler_${elementId}_${event}` —
    // đây là JS identifier KHÔNG hợp lệ (dấu "-" bị hiểu là phép trừ),
    // nên code sinh ra sẽ lỗi cú pháp thật sự khi chạy. Bản Rust này thay
    // "-" bằng "_" trong tên hàm (chỉ ở tên hàm, addEventListener vẫn
    // dùng elementId gốc cho getElementById vì đó là chuỗi, không phải
    // định danh) để JS output hợp lệ.
    let safe_id = element_id.replace('-', "_");
    let fn_keyword = if has_async { "async function" } else { "function" };
    let fn_name = format!("__handler_{}_{}", safe_id, event_ident);
    let func = format!("{} {}(e) {{\n{}\n}}", fn_keyword, fn_name, indent(&body_js, 2));
    let register = format!(
        "document.getElementById('{}')?.addEventListener('{}', {});",
        element_id, dom_event, fn_name
    );

    format!("{}\n{}", func, register)
}

// ════════════════════════════════════════════════════════════
// PAGE LOAD / UNLOAD — REGISTRY (đường dẫn ĐÚNG cho pipeline hiện tại)
// ════════════════════════════════════════════════════════════

/// Biên dịch danh sách PageEvent (on_tai/on_huy) thành 2 action id
/// (đăng ký vào cùng registry với on_click/on_hover/...), KHÔNG sinh
/// JS. Trả về (id_on_tai, id_on_huy) dưới dạng chuỗi số — None nếu
/// trang không khai báo sự kiện tương ứng (không đăng ký action rỗng
/// vô ích vào registry).
///
/// Runtime (router.rs::activate_page) đọc 2 attribute
/// "data-vb-on-tai"/"data-vb-on-huy" nhúng trên chính div `.vb-page`
/// (xem codegen/mod.rs::gen_page) để biết action nào cần dispatch khi
/// trang được kích hoạt/rời khỏi.
pub fn compile_page_load_registry(events: &[PageEvent]) -> (Option<String>, Option<String>) {
    let on_tai_body: Vec<Action> = events
        .iter()
        .filter(|e| e.name == PageEventName::OnTai)
        .flat_map(|e| e.body.iter().cloned())
        .collect();
    let on_huy_body: Vec<Action> = events
        .iter()
        .filter(|e| e.name == PageEventName::OnHuy)
        .flat_map(|e| e.body.iter().cloned())
        .collect();

    let id_on_tai = if on_tai_body.is_empty() {
        None
    } else {
        Some(register_action_body(on_tai_body).to_string())
    };
    let id_on_huy = if on_huy_body.is_empty() {
        None
    } else {
        Some(register_action_body(on_huy_body).to_string())
    };

    (id_on_tai, id_on_huy)
}

// ════════════════════════════════════════════════════════════
// PAGE LOAD / UNLOAD (on_tai / on_huy ở cấp trang) — KIẾN TRÚC CŨ
// ════════════════════════════════════════════════════════════
// GIỮ NGUYÊN không đổi (JS-emitting) chỉ để không phá test hiện có —
// KHÔNG dùng trong pipeline build thật nữa, xem
// compile_page_load_registry() ở trên.

/// Biên dịch danh sách PageEvent (on_tai/on_huy) thành JS đăng ký
/// DOMContentLoaded / beforeunload. Tương đương compilePageLoad().
pub fn compile_page_load(events: &[PageEvent]) -> String {
    let on_tai_body: Vec<&vibao_ast::Action> = events
        .iter()
        .filter(|e| e.name == PageEventName::OnTai)
        .flat_map(|e| e.body.iter())
        .collect();
    let on_huy_body: Vec<&vibao_ast::Action> = events
        .iter()
        .filter(|e| e.name == PageEventName::OnHuy)
        .flat_map(|e| e.body.iter())
        .collect();

    let load_body = on_tai_body.iter().map(|a| compile_action(a, 1)).collect::<Vec<_>>().join("\n");
    let unload_body = on_huy_body.iter().map(|a| compile_action(a, 1)).collect::<Vec<_>>().join("\n");

    let mut code = String::new();
    if !load_body.is_empty() {
        code.push_str(&format!(
            "// on_tai\ndocument.addEventListener('DOMContentLoaded', async () => {{\n{}\n}});\n",
            indent(&load_body, 2)
        ));
    }
    if !unload_body.is_empty() {
        code.push_str(&format!(
            "// on_huy\nwindow.addEventListener('beforeunload', async () => {{\n{}\n}});\n",
            indent(&unload_body, 2)
        ));
    }
    code
}

// ════════════════════════════════════════════════════════════
// HOVER / SCROLL ANIMATION JS
// ════════════════════════════════════════════════════════════

/// Sinh IIFE gắn class hover animation khi rê chuột — tương đương
/// compileHoverAnimation().
pub fn compile_hover_animation(element_id: &str, anim_name: &str, duration_ms: u32) -> String {
    let css_class = format!("vb-hover-{}", anim_name);
    format!(
        "// Hover animation: {anim_name}\n(function() {{\n  const __el = document.getElementById('{id}');\n  if (!__el) return;\n  __el.style.transition = 'all {dur}ms ease';\n  __el.addEventListener('mouseenter', () => __el.classList.add('{cls}'));\n  __el.addEventListener('mouseleave', () => __el.classList.remove('{cls}'));\n}})();",
        anim_name = anim_name,
        id = element_id,
        dur = duration_ms,
        cls = css_class
    )
}

/// Sinh IIFE dùng IntersectionObserver để kích hoạt animation khi phần
/// tử cuộn vào khung nhìn — tương đương compileScrollAnimation().
pub fn compile_scroll_animation(element_id: &str, anim_name: &str, duration_ms: u32, delay_ms: u32) -> String {
    format!(
        "// Scroll animation: {anim}\n(function() {{\n  const __el = document.getElementById('{id}');\n  if (!__el) return;\n  __el.style.opacity = '0';\n  const __obs = new IntersectionObserver(([entry]) => {{\n    if (entry.isIntersecting) {{\n      setTimeout(() => {{\n        __el.style.transition = 'all {dur}ms ease';\n        __el.classList.add('vb-anim-{anim}');\n        __el.style.opacity = '1';\n      }}, {delay});\n      __obs.disconnect();\n    }}\n  }}, {{ threshold: 0.1 }});\n  __obs.observe(__el);\n}})();",
        anim = anim_name,
        id = element_id,
        dur = duration_ms,
        delay = delay_ms
    )
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
    fn test_compile_assign_simple() {
        let action = Action::Assign {
            target: "dem".to_string(),
            value: Expr::literal_num(5.0, p()),
            pos: p(),
        };
        assert_eq!(compile_assign(&action), "__setState('dem', 5);");
    }

    #[test]
    fn test_compile_state_mutation_them() {
        let call = Expr::Call {
            callee: "ds.them".to_string(),
            args: vec![Expr::literal_str("item mới", p())],
            pos: p(),
        };
        let action = Action::Assign { target: "ds".to_string(), value: call, pos: p() };
        assert_eq!(compile_assign(&action), "__statePush('ds', \"item mới\");");
    }

    #[test]
    fn test_compile_state_mutation_xoa_het() {
        let call = Expr::Call { callee: "ds.xoa_het".to_string(), args: vec![], pos: p() };
        let action = Action::Assign { target: "ds".to_string(), value: call, pos: p() };
        assert_eq!(compile_assign(&action), "__setState('ds', []);");
    }

    #[test]
    fn test_compile_function_call_mapped_name() {
        let action = Action::FunctionCall {
            name: "dieu_huong".to_string(),
            args: vec![Expr::literal_str("/trang-chu", p())],
            opts: vec![],
            assign_to: None,
            pos: p(),
        };
        assert_eq!(compile_function_call(&action), "__router.push(\"/trang-chu\");");
    }

    #[test]
    fn test_compile_function_call_with_assign_to_awaits() {
        let action = Action::FunctionCall {
            name: "tai_du_lieu".to_string(),
            args: vec![],
            opts: vec![],
            assign_to: Some("ket_qua".to_string()),
            pos: p(),
        };
        assert_eq!(compile_function_call(&action), "__setState('ket_qua', await __vb.load());");
    }

    #[test]
    fn test_compile_function_call_custom_needs_no_await() {
        let action = Action::FunctionCall {
            name: "ham_tuy_chinh".to_string(),
            args: vec![],
            opts: vec![],
            assign_to: None,
            pos: p(),
        };
        assert_eq!(compile_function_call(&action), "ham_tuy_chinh();");
    }

    #[test]
    fn test_compile_function_call_with_named_opts() {
        let action = Action::FunctionCall {
            name: "thong_bao".to_string(),
            args: vec![Expr::literal_str("Thành công!", p())],
            opts: vec![("kieu".to_string(), Expr::literal_str("thanh_cong", p()))],
            assign_to: None,
            pos: p(),
        };
        assert_eq!(
            compile_function_call(&action),
            "__vb.toast(\"Thành công!\", { kieu: \"thanh_cong\" });"
        );
    }

    #[test]
    fn test_compile_if_action_with_else() {
        let action = Action::IfAction {
            condition: Expr::Variable("dang_dang_nhap".to_string(), p()),
            consequent: vec![Action::FunctionCall {
                name: "dieu_huong".to_string(),
                args: vec![Expr::literal_str("/home".to_string(), p())],
                opts: vec![],
                assign_to: None,
                pos: p(),
            }],
            alternate: Some(vec![Action::FunctionCall {
                name: "dieu_huong".to_string(),
                args: vec![Expr::literal_str("/login".to_string(), p())],
                opts: vec![],
                assign_to: None,
                pos: p(),
            }]),
            pos: p(),
        };
        let out = compile_if_action(&action, 0);
        assert!(out.contains("if (__s.dang_dang_nhap)"));
        assert!(out.contains("else {"));
        assert!(out.contains("/home"));
        assert!(out.contains("/login"));
    }

    #[test]
    fn test_needs_async_api_call() {
        let action = Action::ApiCall {
            method: "GET".to_string(),
            endpoint: Expr::literal_str("/api/x", p()),
            data: None,
            assign_to: None,
            on_success: None,
            on_failure: None,
            pos: p(),
        };
        assert!(needs_async(&action));
    }

    #[test]
    fn test_compile_event_handler_click_sync() {
        let event = vibao_ast::EventNode {
            name: vibao_ast::EventName::OnClick,
            body: vec![Action::Assign { target: "dem".to_string(), value: Expr::literal_num(1.0, p()), pos: p() }],
            pos: p(),
        };
        let out = compile_event_handler(&event, "vb-button-1");
        // Tên hàm phải là 1 JS identifier hợp lệ (không chứa "-") — xem
        // ghi chú bug fix trong compile_event_handler().
        assert!(out.contains("function __handler_vb_button_1_on_click(e)"));
        assert!(out.contains("getElementById('vb-button-1')"));
        assert!(out.contains("addEventListener('click'"));
        assert!(!out.starts_with("async"));
    }

    #[test]
    fn test_compile_event_handler_with_api_call_is_async() {
        let event = vibao_ast::EventNode {
            name: vibao_ast::EventName::OnSubmit,
            body: vec![Action::ApiCall {
                method: "POST".to_string(),
                endpoint: Expr::literal_str("/api/submit", p()),
                data: None,
                assign_to: None,
                on_success: None,
                on_failure: None,
                pos: p(),
            }],
            pos: p(),
        };
        let out = compile_event_handler(&event, "vb-form-1");
        assert!(out.starts_with("async function"));
        assert!(out.contains("addEventListener('submit'"));
    }

    #[test]
    fn test_compile_page_load_on_tai_only() {
        let events = vec![PageEvent {
            name: PageEventName::OnTai,
            body: vec![Action::FunctionCall {
                name: "thong_bao".to_string(),
                args: vec![Expr::literal_str("Chào mừng".to_string(), p())],
                opts: vec![],
                assign_to: None,
                pos: p(),
            }],
            pos: p(),
        }];
        let out = compile_page_load(&events);
        assert!(out.contains("DOMContentLoaded"));
        assert!(!out.contains("beforeunload"));
    }

    #[test]
    fn test_compile_hover_animation_shape() {
        let out = compile_hover_animation("vb-box-1", "phong_to", 300);
        assert!(out.contains("vb-hover-phong_to"));
        assert!(out.contains("getElementById('vb-box-1')"));
        assert!(out.contains("300ms"));
    }

    #[test]
    fn test_compile_scroll_animation_shape() {
        let out = compile_scroll_animation("vb-box-1", "fade_in", 500, 100);
        assert!(out.contains("IntersectionObserver"));
        assert!(out.contains("vb-anim-fade_in"));
        assert!(out.contains("}, 100);"));
    }

    // ── Action registry (Rust/WASM dispatcher) ──────────────────────

    #[test]
    fn test_register_action_body_assigns_sequential_ids() {
        take_action_registry(); // dọn sạch registry thread này trước
        let a1 = vec![Action::Assign {
            target: "dem".to_string(),
            value: Expr::literal_num(1.0, p()),
            pos: p(),
        }];
        let a2 = vec![Action::Assign {
            target: "dem".to_string(),
            value: Expr::literal_num(2.0, p()),
            pos: p(),
        }];
        let id1 = register_action_body(a1);
        let id2 = register_action_body(a2);
        assert_eq!(id2, id1 + 1);
    }

    #[test]
    fn test_take_action_registry_drains_and_resets() {
        take_action_registry();
        register_action_body(vec![]);
        register_action_body(vec![]);
        let drained = take_action_registry();
        assert_eq!(drained.len(), 2);
        let empty = take_action_registry();
        assert_eq!(empty.len(), 0);
    }

    #[test]
    fn test_compile_event_handler_registry_emits_attribute_not_js() {
        take_action_registry();
        let event = vibao_ast::EventNode {
            name: EventName::OnClick,
            body: vec![Action::FunctionCall {
                name: "thong_bao".to_string(),
                args: vec![Expr::literal_str("Xin chào".to_string(), p())],
                opts: vec![],
                assign_to: None,
                pos: p(),
            }],
            pos: p(),
        };
        let (attr_name, action_id) = compile_event_handler_registry(&event);
        assert_eq!(attr_name, "data-vb-on-click");
        // action_id phải parse được thành số (không phải JS code)
        assert!(action_id.parse::<usize>().is_ok());

        // Registry phải thực sự chứa đúng action vừa đăng ký.
        let registry = take_action_registry();
        let id: usize = action_id.parse().unwrap();
        assert_eq!(registry[id].len(), 1);
    }
}
