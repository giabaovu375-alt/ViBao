// ============================================================
// VIBAO RUNTIME (Rust/WASM) — runtime/router.rs
// Port của 20-runtime-router.ts sang Rust thuần. Đây là router SPA
// THẬT — mọi trang ViBao được build chung vào 1 index.html (xem
// vibaoc/src/main.rs::cmd_build), mỗi trang là 1
// `<div class="vb-page" data-route="...">`. Router chỉ ẨN/HIỆN đúng
// div tương ứng với URL hiện tại, KHÔNG reload trang, KHÔNG fetch HTML
// qua mạng — toàn bộ trang đã có sẵn trong DOM từ lúc load ban đầu.
//
// KHÁC BIỆT so với bản JS gốc:
//   - Không có __guards/registerGuard — ViBao hiện chưa có cú pháp khai
//     báo guard trong ngôn ngữ (không có "guard(...)" ở ast.rs), nên
//     phần auth-gate để dành đợt sau khi ngôn ngữ có cú pháp tương ứng.
//   - Route pattern (":id") dùng cách so khớp segment thủ công thay vì
//     biên dịch ra Regex thật — tránh thêm dependency `regex` chỉ cho
//     1 việc đơn giản (so khớp theo dấu "/"), đủ cho phần lớn use case.
// ============================================================

use std::cell::RefCell;
use std::collections::BTreeMap;

use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::Event;

use super::dom;
use super::state::SharedState;
use super::value::VbValue;

// ════════════════════════════════════════════════════════════
// 1. ROUTE REGISTRY & MATCHING
// ════════════════════════════════════════════════════════════

/// 1 route đã đăng ký — tương đương phần tử của mảng `__routes` ở bản JS.
#[derive(Clone)]
pub struct RouteEntry {
    /// Pattern gốc, vd "/san-pham/:id" — dùng làm khoá tra
    /// `[data-route="..."]` trong DOM.
    pub pattern: String,
    /// Tên các param theo đúng thứ tự xuất hiện, vd ["id"] cho pattern
    /// "/san-pham/:id".
    pub param_names: Vec<String>,
}

/// Kết quả so khớp 1 route: chính route đó + giá trị param thực tế lấy
/// từ URL, vd {"id": "42"}.
pub struct RouteMatch {
    pub route: RouteEntry,
    pub params: BTreeMap<String, String>,
}

thread_local! {
    static ROUTES: RefCell<Vec<RouteEntry>> = RefCell::new(Vec::new());
    static CURRENT_ROUTE: RefCell<Option<String>> = RefCell::new(None);
}

/// Đăng ký 1 route vào registry — gọi bởi `boot_router()` cho mỗi trang
/// tìm thấy trong DOM (`[data-route]`) lúc khởi động.
fn register_route(pattern: &str) {
    let param_names = pattern
        .split('/')
        .filter(|seg| seg.starts_with(':'))
        .map(|seg| seg[1..].to_string())
        .collect();
    ROUTES.with(|r| {
        r.borrow_mut().push(RouteEntry {
            pattern: pattern.to_string(),
            param_names,
        });
    });
}

/// Chuẩn hoá 1 path: bỏ query string, hash, và trailing slash (trừ "/"
/// gốc) — tương đương phần đầu của `__matchRoute` bản JS.
fn normalize_path(path: &str) -> String {
    let without_query = path.split('?').next().unwrap_or(path);
    let without_hash = without_query.split('#').next().unwrap_or(without_query);
    let trimmed = without_hash.trim_end_matches('/');
    if trimmed.is_empty() {
        "/".to_string()
    } else {
        trimmed.to_string()
    }
}

/// So khớp `path` với 1 `pattern` (có thể chứa segment ":ten") theo
/// từng đoạn phân cách bởi "/". Trả về param map nếu khớp.
fn match_pattern(pattern: &str, path: &str) -> Option<BTreeMap<String, String>> {
    // Giữ String đã normalize trong 1 biến riêng TRƯỚC khi split — nếu
    // gọi .split() thẳng trên kết quả normalize_path(...) (giá trị tạm),
    // Vec<&str> thu được sẽ vay mượn từ 1 String bị huỷ ngay cuối
    // statement đó, tạo ra dangling reference (lỗi E0716). Tách biến rõ
    // ràng để String sống đủ lâu cho tới khi pattern_segs/path_segs
    // không còn dùng nữa.
    let normalized_pattern = normalize_path(pattern);
    let normalized_path = normalize_path(path);
    let pattern_segs: Vec<&str> = normalized_pattern.split('/').collect();
    let path_segs: Vec<&str> = normalized_path.split('/').collect();

    if pattern_segs.len() != path_segs.len() {
        return None;
    }

    let mut params = BTreeMap::new();
    for (p_seg, path_seg) in pattern_segs.iter().zip(path_segs.iter()) {
        if let Some(name) = p_seg.strip_prefix(':') {
            // Segment param — nhận mọi giá trị không rỗng, decode
            // percent-encoding cơ bản qua js_sys nếu cần (bỏ qua để đơn
            // giản, hầu hết route param không chứa ký tự đặc biệt).
            if path_seg.is_empty() {
                return None;
            }
            params.insert(name.to_string(), path_seg.to_string());
        } else if p_seg != path_seg {
            return None;
        }
    }

    Some(params)
}

/// Tìm route khớp với `path`, ưu tiên theo thứ tự đăng ký (route đăng
/// ký trước có ưu tiên cao hơn nếu 2 pattern cùng khớp 1 path — hiếm
/// khi xảy ra với thiết kế route thông thường).
fn match_route(path: &str) -> Option<RouteMatch> {
    ROUTES.with(|routes| {
        for route in routes.borrow().iter() {
            if let Some(params) = match_pattern(&route.pattern, path) {
                return Some(RouteMatch {
                    route: route.clone(),
                    params,
                });
            }
        }
        None
    })
}

// ════════════════════════════════════════════════════════════
// 2. PAGE ACTIVATION / TEARDOWN
// ════════════════════════════════════════════════════════════

/// Ẩn mọi `.vb-page`, hiện đúng 1 trang khớp `route_match`, bơm route
/// params vào state (để `$id` trong "trang(\"/san-pham/:id\")" resolve
/// đúng), rồi re-bind subtree đó.
///
/// LƯU Ý: gọi `bind_subtree` LẠI mỗi lần activate (kể cả khi quay lại 1
/// trang đã activate trước đó) — điều này ĐĂNG KÝ SUBSCRIBER MỚI mỗi
/// lần, tương tự bản JS gốc (comment "an toàn vì __subscribe tự tạo
/// binding mới"). Đây là 1 rò rỉ bộ nhớ NHẸ nếu người dùng qua lại 1
/// route nhiều lần — chấp nhận được cho use case web app thông thường
/// (không phải app chạy hàng giờ không refresh), nhưng ghi chú rõ ở đây
/// để biết nếu cần tối ưu (unsubscribe binding cũ trước khi bind lại)
/// về sau.
fn activate_page(shared: &SharedState, route_match: &RouteMatch) {
    let doc = match web_sys::window().and_then(|w| w.document()) {
        Some(d) => d,
        None => return,
    };

    // Trước khi ẩn trang CŨ (nếu có), đọc data-vb-on-huy của nó và
    // dispatch — đây chính là lúc đúng để chạy on_huy (đang RỜI khỏi
    // trang đó), trước khi DOM của nó bị display:none.
    if let Some(prev_pattern) = current_route() {
        if prev_pattern != route_match.route.pattern {
            dispatch_lifecycle_action(shared, &doc, &prev_pattern, "data-vb-on-huy");
        }
    }

    // Ẩn mọi trang.
    if let Ok(list) = doc.query_selector_all(".vb-page") {
        for i in 0..list.length() {
            if let Some(node) = list.item(i) {
                if let Ok(el) = node.dyn_into::<web_sys::HtmlElement>() {
                    let _ = el.style().set_property("display", "none");
                }
            }
        }
    }

    // Tìm đúng div của route này qua CSS attribute selector. Escape dấu
    // '"' trong pattern (phòng trường hợp route chứa ký tự đặc biệt) để
    // không phá cú pháp selector.
    let escaped = route_match.route.pattern.replace('\\', "\\\\").replace('"', "\\\"");
    let selector = format!(".vb-page[data-route=\"{}\"]", escaped);

    let target = match doc.query_selector(&selector).ok().flatten() {
        Some(t) => t,
        None => {
            super::log::error(&format!(
                "[ViBao Router] Không tìm thấy DOM cho route \"{}\"",
                route_match.route.pattern
            ));
            return;
        }
    };

    if let Ok(html_el) = target.clone().dyn_into::<web_sys::HtmlElement>() {
        let _ = html_el.style().set_property("display", "");
    }

    // Bơm route params vào state — Variable("id") trong Expr sẽ resolve
    // qua scope_resolve -> global state, khớp đúng giá trị URL hiện tại.
    {
        let mut state = shared.borrow_mut();
        for (key, value) in &route_match.params {
            state.set_state(key, VbValue::str(value.clone()));
        }
    }

    // Re-bind toàn bộ subtree của trang vừa active.
    dom::bind_subtree(shared, &target, None);

    CURRENT_ROUTE.with(|cur| {
        *cur.borrow_mut() = Some(route_match.route.pattern.clone());
    });

    // Sau khi trang mới đã hiện + bind xong, dispatch on_tai của nó.
    dispatch_lifecycle_action(shared, &doc, &route_match.route.pattern, "data-vb-on-tai");
}

/// Tìm `.vb-page[data-route="<pattern>"]`, đọc attribute `attr_name`
/// (là "data-vb-on-tai" hoặc "data-vb-on-huy"), nếu có action id hợp
/// lệ thì tra action registry và dispatch (async, fire-and-forget qua
/// spawn_local — activate_page/navigate là hàm đồng bộ, không thể
/// .await trực tiếp).
fn dispatch_lifecycle_action(shared: &SharedState, doc: &web_sys::Document, pattern: &str, attr_name: &str) {
    let escaped = pattern.replace('\\', "\\\\").replace('"', "\\\"");
    let selector = format!(".vb-page[data-route=\"{}\"]", escaped);

    let Some(el) = doc.query_selector(&selector).ok().flatten() else {
        return;
    };
    let Some(raw_id) = el.get_attribute(attr_name) else {
        return;
    };
    let Ok(action_id) = raw_id.trim().parse::<usize>() else {
        super::log::warn(&format!(
            "[ViBao Router] \"{}\" không phải action id hợp lệ: \"{}\"",
            attr_name, raw_id
        ));
        return;
    };
    let Some(actions) = super::action_registry::get(action_id) else {
        super::log::warn(&format!(
            "[ViBao Router] action id {} ({}) không tồn tại trong registry",
            action_id, attr_name
        ));
        return;
    };

    let shared_clone = shared.clone();
    wasm_bindgen_futures::spawn_local(async move {
        super::action::dispatch_all(&shared_clone, &actions, None).await;
    });
}

// ════════════════════════════════════════════════════════════
// 3. NAVIGATE
// ════════════════════════════════════════════════════════════

/// Điều hướng SPA tới `path` — cập nhật URL qua History API (không
/// reload), activate đúng trang. Tương đương `__vbRouterNavigate`.
///
/// `replace`: dùng `history.replaceState` thay vì `pushState` — dùng
/// khi fallback về route mặc định lúc boot (không nên tạo thêm 1 entry
/// lịch sử cho việc "tự sửa URL sai" của chính app).
/// `from_popstate`: nếu true, KHÔNG cập nhật URL nữa (URL đã đúng rồi,
/// đây là điều hướng do người dùng bấm Back/Forward).
pub fn navigate(shared: &SharedState, path: &str, replace: bool, from_popstate: bool) {
    let matched = match match_route(path) {
        Some(m) => m,
        None => {
            super::log::warn(&format!("[ViBao Router] Không tìm thấy route cho \"{}\"", path));
            return;
        }
    };

    if !from_popstate {
        if let Some(win) = web_sys::window() {
            if let Ok(history) = win.history() {
                let state = wasm_bindgen::JsValue::NULL;
                let result = if replace {
                    history.replace_state_with_url(&state, "", Some(path))
                } else {
                    history.push_state_with_url(&state, "", Some(path))
                };
                let _ = result;
            }
        }
    }

    activate_page(shared, &matched);

    if let Some(win) = web_sys::window() {
        win.scroll_to_with_x_and_y(0.0, 0.0);
    }
}

/// Trả về route pattern hiện tại đang active, nếu có.
pub fn current_route() -> Option<String> {
    CURRENT_ROUTE.with(|cur| cur.borrow().clone())
}

// ════════════════════════════════════════════════════════════
// 4. LINK INTERCEPTION & POPSTATE
// ════════════════════════════════════════════════════════════

/// Chặn click vào `<a data-vb-link="/path">` để điều hướng qua router
/// thay vì để trình duyệt reload cả trang. Dùng event delegation ở
/// document (1 listener duy nhất) — tự động hoạt động cả với link được
/// vòng lặp sinh ra động sau này, không cần bind lại mỗi lần render.
fn setup_link_interception(shared: &SharedState) {
    let doc = match web_sys::window().and_then(|w| w.document()) {
        Some(d) => d,
        None => return,
    };

    let shared_clone = shared.clone();
    let closure = Closure::<dyn FnMut(Event)>::new(move |evt: Event| {
        // Cho phép mở tab mới bằng Ctrl/Cmd/giữa-chuột hoạt động bình
        // thường — chỉ can thiệp khi là click chuột trái đơn giản.
        if let Ok(mouse_evt) = evt.clone().dyn_into::<web_sys::MouseEvent>() {
            if mouse_evt.ctrl_key() || mouse_evt.meta_key() || mouse_evt.shift_key() || mouse_evt.button() == 1 {
                return;
            }
        }

        let Some(target) = evt.target() else { return };
        let Ok(mut el) = target.dyn_into::<web_sys::Element>() else { return };

        // Tìm thẻ <a data-vb-link> gần nhất từ target trở lên (bubbling
        // thủ công qua parent chain) — cho phép click vào icon/span bên
        // trong <a> vẫn hoạt động đúng.
        loop {
            if el.tag_name().eq_ignore_ascii_case("a") {
                if let Some(path) = el.get_attribute("data-vb-link") {
                    evt.prevent_default();
                    navigate(&shared_clone, &path, false, false);
                    return;
                }
            }
            match el.parent_element() {
                Some(p) => el = p,
                None => break,
            }
        }
    });

    let target: web_sys::EventTarget = doc.into();
    let _ = target.add_event_listener_with_callback("click", closure.as_ref().unchecked_ref());
    closure.forget();
}

/// Xử lý nút Back/Forward của trình duyệt.
fn setup_popstate_handler(shared: &SharedState) {
    let Some(win) = web_sys::window() else { return };
    let shared_clone = shared.clone();

    let closure = Closure::<dyn FnMut(Event)>::new(move |_evt: Event| {
        let path = web_sys::window()
            .and_then(|w| w.location().pathname().ok())
            .unwrap_or_else(|| "/".to_string());
        navigate(&shared_clone, &path, false, true);
    });

    let target: web_sys::EventTarget = win.into();
    let _ = target.add_event_listener_with_callback("popstate", closure.as_ref().unchecked_ref());
    closure.forget();
}

// ════════════════════════════════════════════════════════════
// 5. BOOT
// ════════════════════════════════════════════════════════════

/// Khởi động router — quét mọi `.vb-page[data-route]` có sẵn trong DOM
/// để đăng ký route, gắn link interception + popstate, rồi activate
/// đúng trang theo URL hiện tại. Gọi 1 lần từ `dom.rs::VbRuntime::new()`
/// SAU KHI bind_subtree() cho toàn `<body>` đã chạy — vì boot_router()
/// dựa vào các `.vb-page` đã tồn tại sẵn trong DOM.
pub fn boot_router(shared: &SharedState) {
    let doc = match web_sys::window().and_then(|w| w.document()) {
        Some(d) => d,
        None => return,
    };

    if let Ok(list) = doc.query_selector_all(".vb-page[data-route]") {
        for i in 0..list.length() {
            if let Some(node) = list.item(i) {
                if let Ok(el) = node.dyn_into::<web_sys::Element>() {
                    if let Some(route) = el.get_attribute("data-route") {
                        register_route(&route);
                    }
                }
            }
        }
    }

    setup_link_interception(shared);
    setup_popstate_handler(shared);

    let path = web_sys::window()
        .and_then(|w| w.location().pathname().ok())
        .unwrap_or_else(|| "/".to_string());

    match match_route(&path) {
        Some(matched) => activate_page(shared, &matched),
        None => {
            // Không khớp route nào — fallback về route đầu tiên đã đăng
            // ký (thường là "/") thay vì để trắng trang hoàn toàn.
            let fallback = ROUTES.with(|r| r.borrow().first().cloned());
            if let Some(route) = fallback {
                super::log::warn(&format!(
                    "[ViBao Router] Không khớp route cho \"{}\", fallback về \"{}\".",
                    path, route.pattern
                ));
                navigate(shared, &route.pattern, true, false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_path_strips_query_and_hash() {
        assert_eq!(normalize_path("/gioi-thieu?x=1#section"), "/gioi-thieu");
    }

    #[test]
    fn test_normalize_path_strips_trailing_slash() {
        assert_eq!(normalize_path("/gioi-thieu/"), "/gioi-thieu");
        assert_eq!(normalize_path("/"), "/");
    }

    #[test]
    fn test_match_pattern_static_route() {
        let params = match_pattern("/gioi-thieu", "/gioi-thieu");
        assert!(params.is_some());
        assert!(params.unwrap().is_empty());
    }

    #[test]
    fn test_match_pattern_with_param() {
        let params = match_pattern("/san-pham/:id", "/san-pham/42").unwrap();
        assert_eq!(params.get("id"), Some(&"42".to_string()));
    }

    #[test]
    fn test_match_pattern_multi_param() {
        let params = match_pattern("/danh-muc/:cat/san-pham/:id", "/danh-muc/dien-tu/san-pham/7").unwrap();
        assert_eq!(params.get("cat"), Some(&"dien-tu".to_string()));
        assert_eq!(params.get("id"), Some(&"7".to_string()));
    }

    #[test]
    fn test_match_pattern_rejects_wrong_segment_count() {
        assert!(match_pattern("/san-pham/:id", "/san-pham/42/extra").is_none());
    }

    #[test]
    fn test_match_pattern_rejects_non_matching_static_segment() {
        assert!(match_pattern("/gioi-thieu", "/lien-he").is_none());
    }

    #[test]
    fn test_register_and_match_route() {
        ROUTES.with(|r| r.borrow_mut().clear()); // dọn sạch registry thread này
        register_route("/san-pham/:id");
        let matched = match_route("/san-pham/99").expect("phải khớp route đã đăng ký");
        assert_eq!(matched.route.pattern, "/san-pham/:id");
        assert_eq!(matched.params.get("id"), Some(&"99".to_string()));
    }

    #[test]
    fn test_match_route_no_match_returns_none() {
        ROUTES.with(|r| r.borrow_mut().clear());
        register_route("/gioi-thieu");
        assert!(match_route("/khong-ton-tai").is_none());
    }
}
