// ============================================================
// VIBAO RUNTIME (Rust/WASM) — runtime/dom.rs
// Port trực tiếp của 18-runtime-event.ts sang Rust/WASM, dùng
// web-sys để thao tác DOM thật. Đây là nơi nối `expr_eval`/`state`
// (thuần Rust, không biết gì về DOM) với trang HTML thật trong
// trình duyệt.
//
// KHÁC BIỆT LỚN NHẤT so với bản JS gốc:
//   - Bản JS: mỗi binding có 1 "getter" — closure JS tuỳ ý, tự do viết
//     bất kỳ biểu thức nào (vd `() => __state.n + 1`), được tạo ra bởi
//     `__compileExprString` (eval chuỗi JS do codegen sinh).
//   - Bản Rust: mỗi binding có 1 `expr_id: usize` — trỏ vào
//     `expr_registry` (Vec<Expr> nạp từ JSON lúc boot). Khi cần giá
//     trị, gọi `expr_eval::eval_tracked(shared, &expr_registry::get(id))`.
//     KHÔNG CÓ EVAL CHUỖI JS Ở ĐÂU CẢ — toàn bộ tính toán chạy bằng
//     Rust thuần trong WASM.
//
// Vì mọi API DOM (`querySelectorAll`, `classList`, `dataset`...) đều là
// FFI qua web-sys/wasm-bindgen, code ở đây có nhiều `.ok()?`/`unwrap_or`
// hơn hẳn so với state.rs/expr_eval.rs (thuần Rust) — đây là chi phí cố
// hữu của việc bọc 1 API vốn "tự do kiểu JS" (DOM lúc nào cũng có thể
// null/không tồn tại) thành 1 API kiểu tĩnh, không tránh được.
// ============================================================

use std::rc::Rc;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use web_sys::{Document, Element, Event, HtmlElement, Window};

use super::expr_eval;
use super::expr_registry;
use super::state::{self, LoopFrame, SharedState};
use super::value::VbValue;
use vibao_ast::Expr;

// ════════════════════════════════════════════════════════════
// 1. HẰNG SỐ ATTRIBUTE — khớp tên với HTML mà codegen sinh ra
// ════════════════════════════════════════════════════════════
// Tên các data-* attribute PHẢI khớp chính xác với những gì
// `codegen/element.rs`/`control.rs` sinh ra trong HTML. Đặt thành hằng
// số ở đây (thay vì rải chuỗi literal khắp file) để dễ đối chiếu khi có
// thay đổi phía codegen — chỉ cần sửa 1 chỗ.

const ATTR_BIND_TEXT: &str = "data-vb-text";
const ATTR_BIND_ATTR_PREFIX: &str = "data-vb-attr-"; // data-vb-attr-src="<exprId>"
const ATTR_BIND_STYLE_PREFIX: &str = "data-vb-style-"; // data-vb-style-color="<exprId>"
const ATTR_BIND_CLASS: &str = "data-vb-class"; // toggle class theo bool expr
const ATTR_IF: &str = "data-vb-if"; // <exprId>
const ATTR_LOOP: &str = "data-vb-loop"; // JSON {iterableExprId,itemVar,indexVar?}
const ATTR_SWITCH: &str = "data-vb-switch"; // <exprId của subject>
const ATTR_CASE: &str = "data-vb-case"; // <exprId của case value>, trên mỗi div con
const ATTR_DEFAULT: &str = "data-vb-default"; // đánh dấu nhánh mặc định (không có giá trị)
const ATTR_LOOP_TEMPLATE_ID: &str = "data-vb-loop-template"; // id của <template>
const ATTR_EVENT_PREFIX: &str = "data-vb-on-"; // data-vb-on-click="<actionId>"
const ATTR_MODEL: &str = "data-vb-model"; // 2-way binding: tên state key

// ════════════════════════════════════════════════════════════
// 2. HELPER TRUY CẬP DOM
// ════════════════════════════════════════════════════════════

fn window() -> Window {
    web_sys::window().expect("[ViBao] không lấy được `window` — đang chạy ngoài trình duyệt?")
}

fn document() -> Document {
    window()
        .document()
        .expect("[ViBao] không lấy được `document`")
}

/// Duyệt mọi element trong `root` (kể cả chính `root`) khớp `selector`,
/// tương đương `root.querySelectorAll(selector)` + tự thêm root nếu nó
/// cũng khớp — bản JS gốc dùng cách tương tự để bind cả node gốc lẫn
/// con cháu khi mount 1 subtree mới (vd sau khi render xong 1 vòng lặp).
fn query_all(root: &Element, selector: &str) -> Vec<Element> {
    let mut out = Vec::new();
    if let Ok(true) = root.matches(selector) {
        out.push(root.clone());
    }
    if let Ok(list) = root.query_selector_all(selector) {
        for i in 0..list.length() {
            if let Some(node) = list.item(i) {
                if let Ok(el) = node.dyn_into::<Element>() {
                    out.push(el);
                }
            }
        }
    }
    out
}

/// Đọc 1 Expr theo id từ registry, trả VbValue::Null nếu id không hợp
/// lệ — không panic (xem lý do ở expr_registry.rs).
///
/// `loop_frame`: nếu binding này nằm bên trong 1 vòng lặp, đây là
/// snapshot LoopFrame CỦA ĐÚNG ITEM đó, được đóng gói sẵn (qua `move`)
/// trong closure gọi hàm này — KHÔNG đọc qua global loop-scope stack.
/// Hàm tự push frame này lên stack NGAY TRƯỚC khi eval, và pop NGAY SAU
/// — đảm bảo đúng bất kể closure chạy vào lúc nào (lần render đầu tiên,
/// hay re-run độc lập về sau khi 1 field bên trong item đó đổi).
///
/// SỬA LỖI: trước đây push/pop chỉ bao quanh lúc BIND (tạo closure) chứ
/// không bao quanh lúc RE-RUN (khi closure chạy lại độc lập sau này) —
/// nghĩa là closure re-run sẽ đọc sai (hoặc không đọc được) LoopFrame,
/// vì lúc đó global stack có thể đang mang frame của 1 item KHÁC (hoặc
/// rỗng). Cách sửa: mỗi closure tự mang theo LoopFrame của mình (tham số
/// `loop_frame` ở đây), tự push/pop NGAY TRONG LẦN GỌI ĐÓ — không phụ
/// thuộc ai đã push từ trước.
fn eval_expr_id_tracked(shared: &SharedState, id: usize, loop_frame: Option<&LoopFrame>) -> VbValue {
    if let Some(frame) = loop_frame {
        shared.borrow_mut().push_loop_scope(frame.clone());
    }

    let result = match expr_registry::get(id) {
        Some(expr) => expr_eval::eval_tracked(shared, &expr),
        None => {
            super::log::warn(&format!("[ViBao] expr id {} không tồn tại trong registry", id));
            VbValue::Null
        }
    };

    if loop_frame.is_some() {
        shared.borrow_mut().pop_loop_scope();
    }

    result
}

fn eval_expr_id(shared: &SharedState, id: usize) -> VbValue {
    match expr_registry::get(id) {
        Some(expr) => expr_eval::eval(shared, &expr),
        None => VbValue::Null,
    }
}

/// Parse 1 attribute value dạng "<số>" thành usize (expr id). Trả None
/// và log warning nếu không parse được — 1 attribute hỏng không nên
/// làm crash cả trang.
fn parse_expr_id(raw: &str, context: &str) -> Option<usize> {
    match raw.trim().parse::<usize>() {
        Ok(id) => Some(id),
        Err(_) => {
            super::log::warn(&format!(
                "[ViBao] \"{}\" không phải expr id hợp lệ (context: {})",
                raw, context
            ));
            None
        }
    }
}

// ════════════════════════════════════════════════════════════
// 3. BIND TEXT / ATTR / STYLE / CLASS
// ════════════════════════════════════════════════════════════

/// data-vb-text="<exprId>" — nội dung text của element = giá trị expr.
/// Tương đương __bindText ở bản JS cũ.
///
/// `loop_frame`: snapshot LoopFrame nếu `el` nằm bên trong 1 item của
/// vòng lặp — xem doc-comment của `eval_expr_id_tracked` để biết vì sao
/// cần tham số này thay vì đọc qua global stack.
fn bind_text(shared: &SharedState, el: &Element, loop_frame: Option<&LoopFrame>) {
    let raw = match el.get_attribute(ATTR_BIND_TEXT) {
        Some(r) => r,
        None => return,
    };
    let Some(id) = parse_expr_id(&raw, "data-vb-text") else {
        return;
    };

    let el = el.clone();
    let loop_frame = loop_frame.cloned();
    state::subscribe(
        shared,
        Box::new(move |sh: &SharedState| {
            let v = eval_expr_id_tracked(sh, id, loop_frame.as_ref());
            el.set_text_content(Some(&v.to_string()));
        }),
    );
}

/// data-vb-attr-<ten>="<exprId>" — set 1 attribute HTML tuỳ ý theo expr.
/// Ví dụ: data-vb-attr-src="12" -> set attribute "src" = evalExpr(12).
/// Tương đương __bindAttr ở bản JS cũ, nhưng gộp CHUNG 1 vòng lặp cho
/// mọi attribute "data-vb-attr-*" tìm thấy trên element (thay vì bản JS
/// liệt kê từng attribute quan tâm — cách này tổng quát hơn, tự động
/// khớp mọi attribute mới mà codegen thêm sau này mà không cần sửa
/// dom.rs).
fn bind_all_attrs(shared: &SharedState, el: &Element, loop_frame: Option<&LoopFrame>) {
    let attr_names = list_attribute_names_with_prefix(el, ATTR_BIND_ATTR_PREFIX);
    for full_name in attr_names {
        let target_attr = &full_name[ATTR_BIND_ATTR_PREFIX.len()..];
        let raw = match el.get_attribute(&full_name) {
            Some(r) => r,
            None => continue,
        };
        let id = match parse_expr_id(&raw, &full_name) {
            Some(id) => id,
            None => continue,
        };

        let el = el.clone();
        let target_attr = target_attr.to_string();
        let loop_frame = loop_frame.cloned();
        state::subscribe(
            shared,
            Box::new(move |sh: &SharedState| {
                let v = eval_expr_id_tracked(sh, id, loop_frame.as_ref());
                if v.is_null() {
                    let _ = el.remove_attribute(&target_attr);
                } else {
                    let _ = el.set_attribute(&target_attr, &v.to_string());
                }
            }),
        );
    }
}

/// data-vb-style-<ten-css>="<exprId>" — set 1 CSS property theo expr.
/// Ví dụ: data-vb-style-color="7" -> style.color = evalExpr(7).
/// Tên CSS property được chuyển từ kebab-case (như trong attribute) —
/// web-sys CSSStyleDeclaration::set_property nhận thẳng kebab-case nên
/// KHÔNG cần chuyển sang camelCase (khác với gán qua `element.style.x`
/// phía JS, vốn cần camelCase; set_property hoạt động ở tầng CSSOM,
/// nhận tên thuộc tính CSS thật).
fn bind_all_styles(shared: &SharedState, el: &Element, loop_frame: Option<&LoopFrame>) {
    let attr_names = list_attribute_names_with_prefix(el, ATTR_BIND_STYLE_PREFIX);
    for full_name in attr_names {
        let css_prop = &full_name[ATTR_BIND_STYLE_PREFIX.len()..];
        let raw = match el.get_attribute(&full_name) {
            Some(r) => r,
            None => continue,
        };
        let id = match parse_expr_id(&raw, &full_name) {
            Some(id) => id,
            None => continue,
        };

        let html_el: HtmlElement = match el.clone().dyn_into() {
            Ok(h) => h,
            Err(_) => continue, // không phải HtmlElement (vd SVG) — bỏ qua an toàn
        };
        let css_prop = css_prop.to_string();
        let loop_frame = loop_frame.cloned();
        state::subscribe(
            shared,
            Box::new(move |sh: &SharedState| {
                let v = eval_expr_id_tracked(sh, id, loop_frame.as_ref());
                let style = html_el.style();
                if v.is_null() {
                    let _ = style.remove_property(&css_prop);
                } else {
                    let _ = style.set_property(&css_prop, &v.to_string());
                }
            }),
        );
    }
}

/// data-vb-class="<exprId>:<ten-class>[,<exprId2>:<ten-class2>,...]"
/// Toggle nhiều class cùng lúc theo truthy-ness của từng expr tương ứng.
/// Tương đương __bindClass ở bản JS cũ (bản JS dùng object map, ở đây
/// dùng chuỗi CSV đơn giản vì codegen chỉ cần sinh 1 attribute value).
fn bind_class(shared: &SharedState, el: &Element, loop_frame: Option<&LoopFrame>) {
    let raw = match el.get_attribute(ATTR_BIND_CLASS) {
        Some(r) => r,
        None => return,
    };

    let pairs: Vec<(usize, String)> = raw
        .split(',')
        .filter_map(|entry| {
            let mut parts = entry.splitn(2, ':');
            let id_str = parts.next()?;
            let class_name = parts.next()?;
            let id = parse_expr_id(id_str, ATTR_BIND_CLASS)?;
            Some((id, class_name.to_string()))
        })
        .collect();

    if pairs.is_empty() {
        return;
    }

    let el = el.clone();
    let loop_frame = loop_frame.cloned();
    state::subscribe(
        shared,
        Box::new(move |sh: &SharedState| {
            let class_list = el.class_list();
            for (id, class_name) in &pairs {
                let v = eval_expr_id_tracked(sh, *id, loop_frame.as_ref());
                let _ = class_list.toggle_with_force(class_name, v.is_truthy());
            }
        }),
    );
}

/// Liệt kê tên mọi attribute của `el` bắt đầu bằng `prefix`. web-sys
/// không có API "list attribute names theo prefix" trực tiếp nên phải
/// tự duyệt qua `Element::attributes()` (NamedNodeMap).
fn list_attribute_names_with_prefix(el: &Element, prefix: &str) -> Vec<String> {
    let attrs = el.attributes();
    let mut out = Vec::new();
    for i in 0..attrs.length() {
        if let Some(attr) = attrs.item(i) {
            let name = attr.name();
            if name.starts_with(prefix) {
                out.push(name);
            }
        }
    }
    out
}

// ════════════════════════════════════════════════════════════
// 4. BIND IF — hiện/ẩn element theo 1 expr boolean
// ════════════════════════════════════════════════════════════
// Tương đương __bindIf ở bản JS cũ. Dùng cách ĐƠN GIẢN NHẤT tương
// thích ngược: toggle `display: none` thay vì tháo/gắn lại node khỏi
// DOM — giữ nguyên state của element con (form input đang gõ dở,
// animation...) khi điều kiện đổi qua đổi lại nhiều lần, giống hành vi
// bản JS gốc (nó cũng chỉ toggle style.display, không remove node).

fn bind_if(shared: &SharedState, el: &Element, loop_frame: Option<&LoopFrame>) {
    let raw = match el.get_attribute(ATTR_IF) {
        Some(r) => r,
        None => return,
    };
    let id = match parse_expr_id(&raw, ATTR_IF) {
        Some(id) => id,
        None => return,
    };

    let html_el: HtmlElement = match el.clone().dyn_into() {
        Ok(h) => h,
        Err(_) => return,
    };

    // Lưu lại display gốc (nếu có set trước đó) để khôi phục đúng giá
    // trị khi điều kiện chuyển từ false -> true, thay vì luôn set về ""
    // (có thể sai nếu phần tử vốn cần "flex"/"grid" chứ không phải mặc
    // định của tag). Đọc 1 LẦN lúc bind, không đọc lại mỗi lần re-run.
    let original_display = html_el.style().get_property_value("display").unwrap_or_default();
    let original_display = if original_display == "none" {
        String::new() // phòng trường hợp HTML gốc vô tình đã có display:none
    } else {
        original_display
    };

    let loop_frame = loop_frame.cloned();
    state::subscribe(
        shared,
        Box::new(move |sh: &SharedState| {
            let v = eval_expr_id_tracked(sh, id, loop_frame.as_ref());
            let style = html_el.style();
            if v.is_truthy() {
                let _ = style.set_property("display", &original_display);
            } else {
                let _ = style.set_property("display", "none");
            }
        }),
    );
}

// ════════════════════════════════════════════════════════════
// 5. BIND LOOP — render lại danh sách con theo 1 mảng động
// ════════════════════════════════════════════════════════════
// Tương đương __bindLoop ở bản JS cũ. Cách hoạt động:
//   1. HTML codegen sinh 1 <template data-vb-loop-template="tplId"> chứa
//      markup của MỘT phần tử lặp, đặt cạnh 1 element "anchor" mang
//      data-vb-loop mô tả nguồn dữ liệu.
//   2. Mỗi lần iterable đổi, xoá sạch mọi node đã render trước đó (nằm
//      giữa anchor và 1 "end marker" comment node), rồi clone template
//      N lần (N = độ dài mảng), lần lượt push 1 LoopFrame tương ứng vào
//      state trước khi bind subtree đó (để bên trong thân lặp,
//      Variable("item_var")/Variable("index_var") resolve đúng qua
//      `scope_resolve_tracked`), rồi pop lại.
//
// LƯU Ý QUAN TRỌNG: push/pop LoopFrame ở đây chỉ có tác dụng ĐÚNG lúc
// đang chạy vòng for khởi tạo binding cho từng item (bind_subtree bên
// dưới) — một khi binding đã đăng ký xong (closure đã được tạo), muốn
// closure đó tiếp tục thấy đúng LoopFrame của ĐÚNG item đó khi re-run
// độc lập (vd 1 field trong item đổi), closure phải tự chứa sẵn giá trị
// item (đóng gói bằng `move`) thay vì đọc lại từ stack global — vì stack
// global tại thời điểm subscriber A re-run có thể đang mang LoopFrame
// của item B (nếu 2 việc xảy ra xen kẽ). Cách xử lý: mỗi node con trong
// loop được bind với 1 "snapshot" LoopFrame ĐÓNG GÓI SẴN trong closure
// (không đọc qua global stack lúc re-run) — xem `bind_subtree_with_loop_frame`.

struct LoopBindingState {
    /// Node cuối cùng đã render (dùng để biết range cần xoá trước khi
    /// render lại). Không lưu Vec toàn bộ node vì chỉ cần biết
    /// "khoảng từ anchor đến node này" là đủ để xoá sạch.
    rendered_count: usize,
}

fn bind_loop(shared: &SharedState, anchor: &Element, outer_loop_frame: Option<&LoopFrame>) {
    let raw = match anchor.get_attribute(ATTR_LOOP) {
        Some(r) => r,
        None => return,
    };

    #[derive(serde::Deserialize)]
    struct LoopSpec {
        iterable_expr_id: usize,
        item_var: String,
        index_var: Option<String>,
        template_id: String,
    }

    let spec: LoopSpec = match serde_json::from_str(&raw) {
        Ok(s) => s,
        Err(err) => {
            super::log::error(&format!(
                "[ViBao] data-vb-loop JSON không hợp lệ: {} ({})",
                raw, err
            ));
            return;
        }
    };

    let doc = document();
    let template_el = match doc.get_element_by_id(&spec.template_id) {
        Some(t) => t,
        None => {
            super::log::error(&format!(
                "[ViBao] Không tìm thấy <template id=\"{}\"> cho vòng lặp",
                spec.template_id
            ));
            return;
        }
    };
    let template: web_sys::HtmlTemplateElement = match template_el.dyn_into() {
        Ok(t) => t,
        Err(_) => return,
    };

    let parent = match anchor.parent_node() {
        Some(p) => p,
        None => return,
    };

    let binding_state = Rc::new(std::cell::RefCell::new(LoopBindingState { rendered_count: 0 }));
    let anchor = anchor.clone();
    // Vòng lặp có thể LỒNG BÊN TRONG 1 vòng lặp khác (item của loop cha
    // chứa 1 loop con) — outer_loop_frame là frame của loop cha (nếu có),
    // cần truyền tiếp xuống lúc eval iterable_expr_id (vì iterable của
    // loop con có thể tham chiếu tới item của loop cha, vd "item_cha.ds_con").
    let outer_loop_frame = outer_loop_frame.cloned();

    state::subscribe(
        shared,
        Box::new(move |sh: &SharedState| {
            let iterable = eval_expr_id_tracked(sh, spec.iterable_expr_id, outer_loop_frame.as_ref());
            let items = iterable.as_array().cloned().unwrap_or_default();

            // Xoá toàn bộ node đã render trước đó (nằm ngay sau anchor).
            let mut bs = binding_state.borrow_mut();
            for _ in 0..bs.rendered_count {
                if let Some(next) = anchor.next_sibling() {
                    let _ = parent.remove_child(&next);
                }
            }
            bs.rendered_count = items.len();
            drop(bs);

            // Render lại từng item, chèn ngay sau anchor theo đúng thứ tự.
            // `insert_after` theo dõi node CUỐI đã chèn thành công (không
            // suy luận qua độ dài parent.child_nodes() — parent có thể
            // chứa các sibling khác không thuộc vòng lặp này, nên
            // "phần tử cuối của parent" KHÔNG đáng tin làm điểm neo).
            let mut insert_after: web_sys::Node = anchor.clone().into();
            for (index, item) in items.iter().enumerate() {
                let fragment = template.content().clone_node_with_deep(true).unwrap_or_else(|_| {
                    doc.create_document_fragment().into()
                });

                let frame = LoopFrame {
                    item_var: spec.item_var.clone(),
                    item_value: item.clone(),
                    index_var: spec.index_var.clone(),
                    index_value: Some(index as f64),
                };

                // Bind toàn bộ subtree vừa clone, truyền `frame` làm THAM
                // SỐ xuyên suốt (không push/pop global stack) — mỗi
                // closure con bên trong tự đóng gói frame này qua `move`,
                // nên đọc đúng dữ liệu của ĐÚNG ITEM này kể cả khi re-run
                // độc lập về sau (xem eval_expr_id_tracked để biết chi
                // tiết cách push/pop được thực hiện tại đúng thời điểm
                // eval, không phải tại thời điểm bind).
                if let Ok(frag_el) = fragment.clone().dyn_into::<web_sys::DocumentFragment>() {
                    bind_fragment_with_loop_frame(sh, &frag_el, &frame);
                }

                // Ghi nhớ node CUỐI CÙNG bên trong fragment TRƯỚC khi chèn
                // — sau khi insert_before/append_child, fragment rỗng đi
                // (nội dung "di chuyển" vào parent), nên phải lấy tham
                // chiếu last_child TRƯỚC lúc đó.
                let last_node_of_fragment = fragment.last_child();

                if let Some(new_sibling) = insert_after.next_sibling() {
                    let _ = parent.insert_before(&fragment, Some(&new_sibling));
                } else {
                    let _ = parent.append_child(&fragment);
                }

                // Cập nhật điểm neo cho lần chèn kế tiếp = node cuối vừa
                // chèn (nếu fragment rỗng bất thường, giữ nguyên neo cũ
                // thay vì panic/trỏ sai chỗ).
                if let Some(last) = last_node_of_fragment {
                    insert_after = last;
                }
            }
        }),
    );
}

/// Bind mọi binding (text/attr/style/class/if/event) bên trong 1
/// DocumentFragment vừa clone từ template, truyền `frame` xuống làm
/// tham số cho mọi hàm bind_* con — KHÔNG dùng global loop-scope stack.
///
/// ĐÃ SỬA (trước đây push/pop global stack quanh lúc bind, gây lỗi khi
/// closure con re-run độc lập về sau — xem lịch sử ở comment cũ đã xoá).
/// Cách sửa: mỗi closure con (bind_text/bind_if/...) tự đóng gói `frame`
/// qua `move` và tự push/pop NGAY TRONG LẦN GỌI eval_expr_id_tracked —
/// đúng bất kể closure chạy vào lúc nào.
// ════════════════════════════════════════════════════════════
// 4b. BIND SWITCH — truong_hop (data-vb-switch / data-vb-case / data-vb-default)
// ════════════════════════════════════════════════════════════
// Tương ứng gen_switch() ở codegen/control.rs. Tính subject 1 lần
// (tracked — re-run khi subject đổi), so khớp bằng strict_eq (===,
// giống hành vi "==" của biểu thức ViBao — xem expr_eval::eval_binary)
// với từng case con theo THỨ TỰ xuất hiện trong DOM, hiện đúng 1 nhánh
// đầu tiên khớp, ẩn mọi nhánh khác. Nếu không case nào khớp, hiện nhánh
// data-vb-default (nếu có).

fn bind_switch(shared: &SharedState, switch_el: &Element, loop_frame: Option<&LoopFrame>) {
    let raw = match switch_el.get_attribute(ATTR_SWITCH) {
        Some(r) => r,
        None => return,
    };
    let Some(subject_id) = parse_expr_id(&raw, ATTR_SWITCH) else {
        return;
    };

    // Thu thập sẵn danh sách (element, Some(case_expr_id) | None cho
    // default) TRƯỚC khi bind — chỉ cần duyệt DOM con 1 lần lúc bind,
    // không phải mỗi lần subject đổi.
    let children = switch_el.children();
    let mut branches: Vec<(Element, Option<usize>)> = Vec::new();
    for i in 0..children.length() {
        let Some(child) = children.item(i) else { continue };
        if child.has_attribute(ATTR_DEFAULT) {
            branches.push((child, None));
        } else if let Some(case_raw) = child.get_attribute(ATTR_CASE) {
            if let Some(case_id) = parse_expr_id(&case_raw, ATTR_CASE) {
                branches.push((child, Some(case_id)));
            }
        }
    }

    if branches.is_empty() {
        return;
    }

    let loop_frame = loop_frame.cloned();
    state::subscribe(
        shared,
        Box::new(move |sh: &SharedState| {
            let subject_val = eval_expr_id_tracked(sh, subject_id, loop_frame.as_ref());

            // Tìm nhánh case ĐẦU TIÊN khớp subject (bỏ qua default ở
            // bước này — default chỉ hiện khi KHÔNG case nào khớp).
            let mut matched_idx: Option<usize> = None;
            for (idx, (_, case_id)) in branches.iter().enumerate() {
                if let Some(cid) = case_id {
                    let case_val = eval_expr_id_tracked(sh, *cid, loop_frame.as_ref());
                    if subject_val.strict_eq(&case_val) {
                        matched_idx = Some(idx);
                        break;
                    }
                }
            }

            // Không case nào khớp -> tìm nhánh default (case_id == None).
            let show_idx = matched_idx.or_else(|| {
                branches.iter().position(|(_, case_id)| case_id.is_none())
            });

            for (idx, (el, _)) in branches.iter().enumerate() {
                if let Ok(html_el) = el.clone().dyn_into::<HtmlElement>() {
                    let display = if Some(idx) == show_idx { "" } else { "none" };
                    let _ = html_el.style().set_property("display", display);
                }
            }
        }),
    );
}

fn bind_fragment_with_loop_frame(shared: &SharedState, fragment: &web_sys::DocumentFragment, frame: &LoopFrame) {
    // DocumentFragment không kế thừa Element trong DOM spec (không thể
    // downcast thành Element) — luôn duyệt qua children trực tiếp.
    let children = fragment.children();
    for i in 0..children.length() {
        if let Some(el) = children.item(i) {
            bind_subtree(shared, &el, Some(frame));
        }
    }
}

/// Bind mọi loại binding tìm thấy trong `root` VÀ mọi hậu duệ của nó
/// (dùng chung bởi __vb.boot() khi bind toàn trang, và bind_loop khi
/// bind 1 subtree mới render). Tương đương __bindAll ở bản JS cũ.
///
/// `loop_frame`: nếu `root` nằm bên trong 1 item của vòng lặp (được gọi
/// từ `bind_fragment_with_loop_frame`), đây là snapshot của item đó —
/// truyền xuống MỌI hàm bind_* con để chúng tự đóng gói vào closure của
/// mình (xem `eval_expr_id_tracked`). Khi bind toàn trang (không trong
/// loop nào), giá trị này là `None`.
pub(crate) fn bind_subtree(shared: &SharedState, root: &Element, loop_frame: Option<&LoopFrame>) {
    for el in query_all(root, &format!("[{}]", ATTR_BIND_TEXT)) {
        bind_text(shared, &el, loop_frame);
    }
    for el in query_all(root, &format!("[{}]", ATTR_IF)) {
        bind_if(shared, &el, loop_frame);
    }
    for el in query_all(root, &format!("[{}]", ATTR_LOOP)) {
        bind_loop(shared, &el, loop_frame);
    }
    for el in query_all(root, &format!("[{}]", ATTR_SWITCH)) {
        bind_switch(shared, &el, loop_frame);
    }
    for el in query_all(root, &format!("[{}]", ATTR_BIND_CLASS)) {
        bind_class(shared, &el, loop_frame);
    }
    for el in query_all(root, "*") {
        bind_all_attrs(shared, &el, loop_frame);
        bind_all_styles(shared, &el, loop_frame);
        bind_events(shared, &el, loop_frame);
        bind_model(shared, &el, loop_frame);
        bind_animations(&el);
    }
}

// ════════════════════════════════════════════════════════════
// 6. BIND EVENT — data-vb-on-<ten-su-kien>="<actionId>"
// ════════════════════════════════════════════════════════════
// Tương đương __bindEvent ở bản JS cũ. Action handler (thân hàm khi bấm
// nút, vd goi_api/thong_bao/gan_bien...) KHÔNG nằm trong expr_eval — đó
// là side-effect, không phải expression thuần. Ở đây ta chỉ lo việc GẮN
// listener và gọi vào 1 dispatcher (action.rs — CHƯA VIẾT, xem TODO ở
// cuối file), truyền action_id để dispatcher tự tra và chạy đúng logic.
//
// Sau khi action chạy xong, PHẢI gọi state::flush() để mọi set_state()
// xảy ra bên trong action được gom lại và trigger re-render 1 lần —
// action::dispatch_one tự gọi flush() ở cuối, không phải tuỳ chọn.
//
// `loop_frame`: nếu action nằm bên trong 1 item của vong_lap (vd
// "xoa($item)" bên trong on_click của button nằm trong loop item), đây
// là snapshot LoopFrame của ĐÚNG item đó, đóng gói qua `move` vào
// closure — để expr_eval bên trong action::dispatch_all resolve đúng
// "$item" kể cả khi handler chạy độc lập về sau (ĐÃ SỬA — trước đây
// tham số này chưa dùng, "$item" bên trong action luôn resolve sai/rỗng
// khi dùng bên trong loop).

fn bind_events(shared: &SharedState, el: &Element, loop_frame: Option<&LoopFrame>) {
    let attr_names = list_attribute_names_with_prefix(el, ATTR_EVENT_PREFIX);
    for full_name in attr_names {
        let event_name = &full_name[ATTR_EVENT_PREFIX.len()..];
        let raw = match el.get_attribute(&full_name) {
            Some(r) => r,
            None => continue,
        };
        let action_id = match parse_expr_id(&raw, &full_name) {
            Some(id) => id,
            None => continue,
        };

        let shared_clone = shared.clone();
        let event_target: web_sys::EventTarget = el.clone().into();

        // Tra registry NGAY LÚC BIND (không phải lúc bấm) — actions
        // (Vec<Action>) không đổi trong suốt vòng đời trang cho 1
        // action_id cố định, nên tra 1 lần và đóng gói qua `move` là đủ,
        // tránh phải tra lại registry mỗi lần bấm (registry chỉ đọc,
        // không có lý do gì để trì hoãn việc tra cứu).
        let actions = match super::action_registry::get(action_id) {
            Some(a) => a,
            None => {
                super::log::warn(&format!(
                    "[ViBao] action id {} không tồn tại trong registry",
                    action_id
                ));
                continue;
            }
        };
        let actions = std::rc::Rc::new(actions);
        let loop_frame_owned = loop_frame.cloned();

        let closure = Closure::<dyn FnMut(Event)>::new(move |_evt: Event| {
            let shared_for_task = shared_clone.clone();
            let actions_for_task = actions.clone();
            let loop_frame_for_task = loop_frame_owned.clone();
            // `dispatch_all` là async (vì ApiCall cần .await fetch) —
            // event listener (closure sync) không thể tự await, nên
            // spawn Future này chạy độc lập trên microtask queue của
            // trình duyệt qua wasm-bindgen-futures::spawn_local. Đây LÀ
            // cách chuẩn để "fire and forget" 1 async task từ ngữ cảnh
            // đồng bộ trong wasm-bindgen, tương đương không await 1
            // promise ở JS (nhưng vẫn đảm bảo Future chạy tới khi xong).
            wasm_bindgen_futures::spawn_local(async move {
                super::action::dispatch_all(&shared_for_task, &actions_for_task, loop_frame_for_task.as_ref()).await;
            });
        });

        let _ = event_target
            .add_event_listener_with_callback(event_name, closure.as_ref().unchecked_ref());
        // `.forget()` giữ closure sống mãi (memory leak có chủ đích) —
        // cần thiết vì listener phải tồn tại suốt đời element, và Rust
        // không có "GC theo dõi DOM lifetime" để tự biết khi nào element
        // bị gỡ để drop closure đúng lúc. Đây LÀ cách làm chuẩn phổ biến
        // khi viết wasm-bindgen event listener dài hạn (không phải lỗi).
        closure.forget();
    }
}

// ════════════════════════════════════════════════════════════
// 7. BIND MODEL — 2-way binding cho input/textarea/select
// ════════════════════════════════════════════════════════════
// data-vb-model="<state_key>" — tương đương __bindModel ở bản JS cũ:
//   - state -> input.value (1 chiều, qua subscribe bình thường)
//   - input -> state (chiều ngược, qua event "input"/"change")

fn bind_model(shared: &SharedState, el: &Element, _loop_frame: Option<&LoopFrame>) {
    let key = match el.get_attribute(ATTR_MODEL) {
        Some(k) => k,
        None => return,
    };

    // Chiều 1: state -> input.value
    if let Ok(input) = el.clone().dyn_into::<web_sys::HtmlInputElement>() {
        let input_clone = input.clone();
        let key_clone = key.clone();
        state::subscribe(
            shared,
            Box::new(move |sh: &SharedState| {
                let v = state::get_tracked(sh, &key_clone);
                input_clone.set_value(&v.to_string());
            }),
        );

        // Chiều 2: input -> state, qua sự kiện "input" (gõ tới đâu cập
        // nhật tới đó, giống v-model.lazy=false của các framework khác).
        let shared_clone = shared.clone();
        let input_for_listener = input.clone();
        let key_for_listener = key.clone();
        let closure = Closure::<dyn FnMut(Event)>::new(move |_evt: Event| {
            let new_val = input_for_listener.value();
            shared_clone
                .borrow_mut()
                .set_state(&key_for_listener, VbValue::str(new_val));
            state::flush(&shared_clone);
        });
        let target: web_sys::EventTarget = input.into();
        let _ = target.add_event_listener_with_callback("input", closure.as_ref().unchecked_ref());
        closure.forget();
        return;
    }

    // Textarea: cùng logic, khác kiểu cụ thể (web-sys tách riêng type).
    if let Ok(textarea) = el.clone().dyn_into::<web_sys::HtmlTextAreaElement>() {
        let ta_clone = textarea.clone();
        let key_clone = key.clone();
        state::subscribe(
            shared,
            Box::new(move |sh: &SharedState| {
                let v = state::get_tracked(sh, &key_clone);
                ta_clone.set_value(&v.to_string());
            }),
        );

        let shared_clone = shared.clone();
        let ta_for_listener = textarea.clone();
        let key_for_listener = key.clone();
        let closure = Closure::<dyn FnMut(Event)>::new(move |_evt: Event| {
            let new_val = ta_for_listener.value();
            shared_clone
                .borrow_mut()
                .set_state(&key_for_listener, VbValue::str(new_val));
            state::flush(&shared_clone);
        });
        let target: web_sys::EventTarget = textarea.into();
        let _ = target.add_event_listener_with_callback("input", closure.as_ref().unchecked_ref());
        closure.forget();
    }
}

// ════════════════════════════════════════════════════════════
// 7a. BIND ANIMATION — data-vb-anim-hover / data-vb-anim-scroll
// ════════════════════════════════════════════════════════════
// Thay thế hoàn toàn kiến trúc JS cũ (compile_hover_animation/
// compile_scroll_animation ở codegen/action.rs, không còn dùng trong
// pipeline build thật — xem ghi chú ở codegen/element.rs::gen_anim_attrs).
// Animation KHÔNG cần state/reactive — chỉ là hiệu ứng CSS class toggle
// theo sự kiện DOM thuần (mouseenter/mouseleave, hoặc lọt vào viewport),
// nên bind 1 lần lúc mount, không qua state::subscribe.

/// Đọc data-vb-anim-hover="<ten>:<thoi_gian_ms>", toggle class
/// "vb-anim-<ten>" khi chuột vào/ra khỏi element.
fn bind_animations(el: &Element) {
    if let Some(raw) = el.get_attribute("data-vb-anim-hover") {
        bind_hover_animation(el, &raw);
    }
    if let Some(raw) = el.get_attribute("data-vb-anim-scroll") {
        bind_scroll_animation(el, &raw);
    }
}

fn bind_hover_animation(el: &Element, raw: &str) {
    let mut parts = raw.splitn(2, ':');
    let anim_name = match parts.next() {
        Some(n) if !n.is_empty() => n.to_string(),
        _ => return,
    };
    let class_name = format!("vb-anim-{}", anim_name);

    let el_enter = el.clone();
    let class_enter = class_name.clone();
    let enter_closure = Closure::<dyn FnMut(Event)>::new(move |_evt: Event| {
        let _ = el_enter.class_list().add_1(&class_enter);
    });
    let target_enter: web_sys::EventTarget = el.clone().into();
    let _ = target_enter
        .add_event_listener_with_callback("mouseenter", enter_closure.as_ref().unchecked_ref());
    enter_closure.forget();

    let el_leave = el.clone();
    let leave_closure = Closure::<dyn FnMut(Event)>::new(move |_evt: Event| {
        let _ = el_leave.class_list().remove_1(&class_name);
    });
    let target_leave: web_sys::EventTarget = el.clone().into();
    let _ = target_leave
        .add_event_listener_with_callback("mouseleave", leave_closure.as_ref().unchecked_ref());
    leave_closure.forget();
}

/// Đọc data-vb-anim-scroll="<ten>:<thoi_gian_ms>:<tre_ms>", dùng
/// IntersectionObserver để thêm class "vb-anim-<ten>" khi element lọt
/// vào viewport (chỉ kích hoạt 1 LẦN — tương đương { once: true } ở
/// bản JS cũ, tự unobserve ngay sau khi trigger để tránh chạy lại mỗi
/// lần cuộn qua cuộn lại).
///
/// ⚠️ CHƯA KIỂM CHỨNG BUILD: đây là API web-sys phức tạp nhất trong
/// toàn bộ dom.rs (IntersectionObserver constructor nhận callback kiểu
/// Function, entries trả về dạng js_sys::Array cần downcast từng phần
/// tử) — sandbox lúc viết không có cargo để tự compile-check. Nếu build
/// lỗi, đây là hàm đầu tiên cần xem — khả năng cao lỗi nằm ở chữ ký
/// đúng của `IntersectionObserver::new()` (có thể cần
/// `IntersectionObserver::new_with_options` hoặc kiểu callback khác).
fn bind_scroll_animation(el: &Element, raw: &str) {
    let anim_name = match raw.splitn(3, ':').next() {
        Some(n) if !n.is_empty() => n.to_string(),
        _ => return,
    };
    let class_name = format!("vb-anim-{}", anim_name);

    // web-sys IntersectionObserver API cần 1 JS callback (Closure) nhận
    // (Array<IntersectionObserverEntry>, IntersectionObserver) — dùng
    // đúng chữ ký này để new() chấp nhận.
    let el_clone = el.clone();
    let observer_cell: std::rc::Rc<std::cell::RefCell<Option<web_sys::IntersectionObserver>>> =
        std::rc::Rc::new(std::cell::RefCell::new(None));
    let observer_cell_for_closure = observer_cell.clone();

    let closure = Closure::<dyn FnMut(js_sys::Array)>::new(move |entries: js_sys::Array| {
        for i in 0..entries.length() {
            let entry = entries.get(i);
            let Ok(entry) = entry.dyn_into::<web_sys::IntersectionObserverEntry>() else {
                continue;
            };
            if entry.is_intersecting() {
                let _ = el_clone.class_list().add_1(&class_name);
                // Kích hoạt 1 lần rồi ngừng quan sát — tránh lặp lại hiệu
                // ứng mỗi lần người dùng cuộn qua lại quanh ngưỡng viewport.
                if let Some(obs) = observer_cell_for_closure.borrow().as_ref() {
                    obs.disconnect();
                }
            }
        }
    });

    let observer = match web_sys::IntersectionObserver::new(closure.as_ref().unchecked_ref()) {
        Ok(o) => o,
        Err(_) => {
            closure.forget();
            return;
        }
    };
    observer.observe(el);
    *observer_cell.borrow_mut() = Some(observer);
    closure.forget();
}

// ════════════════════════════════════════════════════════════
// 7b. UI HELPERS — toast / alert / navigate / modal / scroll / clipboard
// ════════════════════════════════════════════════════════════
// Port phần "side-effect UI" của 19-runtime-api.ts (RUNTIME_TOAST_SOURCE,
// RUNTIME_MODAL_SOURCE, RUNTIME_SCROLL_SOURCE, RUNTIME_CLIPBOARD_SOURCE,
// và 2 hàm điều hướng của RUNTIME_ROUTER_API_SOURCE) — dùng bởi
// action.rs khi thực thi FunctionCall (thong_bao/canh_bao/mo_modal/...).
//
// CHƯA PORT: __auth (token tự động, dang_xuat), __save/__load qua sessionStorage
// riêng biệt (auth.rs để dành đợt sau — ViBao chưa có cú pháp guard() trong
// ast.rs nên chưa có chỗ bám vào tính năng auth).

/// thong_bao(noi_dung, kieu:..., thoi_gian:...) — hiện 1 toast tạm thời,
/// tự biến mất sau `duration_ms`. Tương đương __toast() bản JS cũ.
pub fn toast(message: &str, kieu: &str, duration_ms: i32) {
    let doc = document();
    let container = match ensure_toast_container(&doc) {
        Some(c) => c,
        None => return,
    };

    let el = match doc.create_element("div") {
        Ok(e) => e,
        Err(_) => return,
    };
    let _ = el.set_class_name(&format!("vb-toast vb-toast-{}", kieu));
    el.set_text_content(Some(message));
    let _ = container.append_child(&el);

    // Tự gỡ toast sau `duration_ms` — dùng setTimeout qua web-sys.
    // Closure "one-shot": .forget() vẫn an toàn ở đây vì closure tự kết
    // thúc vòng đời sau khi setTimeout callback chạy 1 lần (không giữ
    // listener sống mãi như event listener thường trực).
    let el_for_timeout = el.clone();
    let closure = Closure::once(move || {
        let _ = el_for_timeout
            .parent_node()
            .map(|parent| parent.remove_child(&el_for_timeout));
    });
    if let Some(win) = web_sys::window() {
        let _ = win.set_timeout_with_callback_and_timeout_and_arguments_0(
            closure.as_ref().unchecked_ref(),
            duration_ms.max(0),
        );
    }
    closure.forget();
}

fn ensure_toast_container(doc: &Document) -> Option<Element> {
    if let Some(existing) = doc.query_selector(".vb-toast-container").ok().flatten() {
        return Some(existing);
    }
    let el = doc.create_element("div").ok()?;
    let _ = el.set_class_name("vb-toast-container");
    doc.body()?.append_child(&el).ok()?;
    Some(el)
}

/// canh_bao(noi_dung) — alert trình duyệt chuẩn.
pub fn alert(message: &str) {
    if let Some(win) = web_sys::window() {
        let _ = win.alert_with_message(message);
    }
}

/// dieu_huong(path) — điều hướng SPA THẬT qua router.rs (History API,
/// không reload trang). Trước đây (khi router.rs chưa tồn tại) hàm này
/// tạm dùng location.href — đã thay bằng router::navigate() thật.
pub fn navigate(shared: &SharedState, path: &str) {
    super::router::navigate(shared, path, false, false);
}

/// mo_tab_moi(path) — mở tab mới. Nếu path đã là URL đầy đủ, giữ
/// nguyên; ngược lại ghép với origin hiện tại.
pub fn open_tab(path: &str) {
    let win = match web_sys::window() {
        Some(w) => w,
        None => return,
    };
    let url = if path.starts_with("http://") || path.starts_with("https://") {
        path.to_string()
    } else {
        let origin = win.location().origin().unwrap_or_default();
        format!("{}{}", origin, path)
    };
    let _ = win.open_with_url_and_target_and_features(&url, "_blank", "noopener");
}

/// mo_modal(id) — hiện overlay có id "vb-modal-<id>" (do codegen sinh
/// sẵn trong HTML lúc build, xem codegen phần modal — nếu chưa có,
/// hàm này chỉ log cảnh báo an toàn, không panic).
pub fn open_modal(id: &str) {
    let doc = document();
    let el_id = format!("vb-modal-{}", id);
    let Some(overlay) = doc.get_element_by_id(&el_id) else {
        super::log::warn(&format!("[ViBao] Không tìm thấy modal \"{}\"", id));
        return;
    };
    if let Ok(html_el) = overlay.dyn_into::<HtmlElement>() {
        let _ = html_el.style().set_property("display", "flex");
        let _ = html_el.focus();
    }
    if let Some(body) = doc.body() {
        let _ = body.style().set_property("overflow", "hidden");
    }
}

/// dong_modal(id) — ẩn overlay tương ứng, khôi phục scroll body nếu
/// không còn modal nào khác đang mở.
pub fn close_modal(id: &str) {
    let doc = document();
    let el_id = format!("vb-modal-{}", id);
    let Some(overlay) = doc.get_element_by_id(&el_id) else {
        return;
    };
    if let Ok(html_el) = overlay.dyn_into::<HtmlElement>() {
        let _ = html_el.style().set_property("display", "none");
    }
    // Kiểm tra còn modal nào khác đang mở không (display != "none") —
    // đơn giản hoá so với bản JS (không track registry riêng), quét
    // trực tiếp DOM vì số lượng modal trên 1 trang thường rất nhỏ.
    let any_open = doc
        .query_selector_all("[id^='vb-modal-']")
        .ok()
        .map(|list| {
            (0..list.length()).any(|i| {
                list.item(i)
                    .and_then(|n| n.dyn_into::<HtmlElement>().ok())
                    .map(|el| el.style().get_property_value("display").unwrap_or_default() != "none")
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false);

    if !any_open {
        if let Some(body) = doc.body() {
            let _ = body.style().set_property("overflow", "");
        }
    }
}

/// cuon_den(target) — cuộn mượt tới phần tử có id = target (bỏ dấu "#"
/// nếu có).
pub fn scroll_to(target: &str) {
    let id = target.trim_start_matches('#');
    let doc = document();
    let Some(el) = doc.get_element_by_id(id) else {
        super::log::warn(&format!("[ViBao] cuon_den(): không tìm thấy phần tử #{}", id));
        return;
    };
    el.scroll_into_view();
}

/// cuon_len_dau() — cuộn mượt về đầu trang.
pub fn scroll_top() {
    if let Some(win) = web_sys::window() {
        win.scroll_to_with_x_and_y(0.0, 0.0);
    }
}

/// sao_chep(text) — copy vào clipboard.
///
/// CHƯA TRIỂN KHAI THẬT: Clipboard API trong web-sys là "unstable" —
/// cần feature `Clipboard`+`Navigator` (đã bật ở Cargo.toml) NHƯNG còn
/// cần thêm cờ build `--cfg=web_sys_unstable_apis` (RUSTFLAGS hoặc
/// .cargo/config.toml) mới thực sự gọi được `Navigator::clipboard()`.
/// Thiếu cờ đó, code gọi thẳng API này sẽ KHÔNG BIÊN DỊCH ĐƯỢC — rủi ro
/// làm hỏng toàn bộ build của crate chỉ vì 1 tính năng phụ. Để an toàn,
/// tạm thời chỉ log cảnh báo; bật lại khi đã cấu hình đúng cờ build
/// (xem ghi chú thêm ở Cargo.toml nếu/khi làm việc này).
pub fn copy_text(_text: &str) {
    super::log::warn(
        "[ViBao] sao_chep() chưa được hỗ trợ — Clipboard API cần cờ build \
         --cfg=web_sys_unstable_apis, xem ghi chú trong dom.rs::copy_text.",
    );
}


// ════════════════════════════════════════════════════════════
// 8. BOOT — điểm khởi động, gọi từ JS qua __vb.boot({...})
// ════════════════════════════════════════════════════════════
// Tương đương phần cuối 18-runtime-event.ts (nơi gắn tất cả bind* vào
// DOMContentLoaded). Ở đây, thay vì tự lắng nghe DOMContentLoaded (JS
// script tag đặt cuối <body> nên DOM luôn đã sẵn sàng khi script chạy
// tới, không cần đợi thêm), ta expose thẳng 1 hàm `boot` để dòng cuối
// cùng của JS do codegen sinh ra gọi trực tiếp:
//
//   __vb.boot({ baseURL: '...', exprRegistry: [...], actionRegistry: [...] });
//
// `#[wasm_bindgen]` tự sinh JS glue code cho phép gọi hàm Rust này từ
// JS như 1 hàm bình thường — đây là ranh giới FFI chính giữa JS (chỉ
// còn vài dòng bootstrap tối thiểu) và WASM (toàn bộ logic thật).

#[wasm_bindgen]
pub struct VbRuntime {
    shared: SharedState,
}

#[wasm_bindgen]
impl VbRuntime {
    /// Khởi động runtime: nạp expr + action registry, set base_url,
    /// bind toàn bộ binding có sẵn trong DOM hiện tại (`<body>`).
    /// `opts_json` là 1 chuỗi JSON dạng
    /// `{ "baseURL": "...", "exprRegistry": [...], "actionRegistry": [...] }`.
    #[wasm_bindgen(constructor)]
    pub fn new(opts_json: &str) -> VbRuntime {
        #[derive(serde::Deserialize)]
        struct BootOpts {
            #[serde(rename = "exprRegistry")]
            expr_registry: Vec<Expr>,
            #[serde(rename = "actionRegistry", default)]
            action_registry: Vec<Vec<vibao_ast::Action>>,
            #[serde(rename = "baseURL")]
            base_url: String,
        }

        let opts: BootOpts = match serde_json::from_str(opts_json) {
            Ok(o) => o,
            Err(err) => {
                super::log::error(&format!("[ViBao] boot() nhận JSON không hợp lệ: {}", err));
                BootOpts {
                    expr_registry: Vec::new(),
                    action_registry: Vec::new(),
                    base_url: String::new(),
                }
            }
        };

        expr_registry::load_from_json(
            &serde_json::to_string(&opts.expr_registry).unwrap_or_else(|_| "[]".to_string()),
        );
        super::action_registry::load_from_json(
            &serde_json::to_string(&opts.action_registry).unwrap_or_else(|_| "[]".to_string()),
        );

        let shared = state::new_shared_state();
        shared.borrow_mut().set_base_url(opts.base_url);

        // KHÔNG bind_subtree() cho toàn <body> ở đây — làm vậy sẽ bind
        // TRÙNG 2 lần cho trang đang active (1 lần ở đây cho toàn body,
        // 1 lần nữa bên trong router::boot_router khi nó activate đúng
        // trang). router::boot_router() là nơi DUY NHẤT gọi bind_subtree,
        // và nó tự biết cần bind subtree của ĐÚNG 1 trang (.vb-page)
        // khớp với URL hiện tại — các trang khác (chưa active) không cần
        // bind ngay, sẽ tự bind khi router điều hướng tới chúng lần đầu.
        super::router::boot_router(&shared);

        VbRuntime { shared }
    }

    /// Expose evalExpr(id) cho JS gọi trực tiếp nếu cần (vd debug console,
    /// hoặc 1 action handler muốn tính lại giá trị hiện tại của 1 expr đã
    /// đăng ký mà không qua binding). KHÔNG track dependency (dùng
    /// `eval`, không phải `eval_tracked`) vì lời gọi từ JS console/action
    /// không nằm trong ngữ cảnh 1 subscriber đang chạy.
    #[wasm_bindgen(js_name = evalExpr)]
    pub fn eval_expr(&self, id: usize) -> JsValue {
        let v = eval_expr_id(&self.shared, id);
        v.to_js_value()
    }
}
