// ============================================================
// VIBAO RUNTIME (Rust/WASM) — runtime/state.rs
// Port trực tiếp của 17-runtime-state.ts sang Rust thuần.
//
// Khác biệt cốt lõi so với bản JS gốc:
//   - Bản JS: 1 "subscriber" là 1 closure JS bất kỳ (run: () => {...}),
//     tự do đọc __state.x/__state.y bên trong; deps được suy ra nhờ
//     __getState() ghi lại field nào bị đọc trong lúc closure chạy, nhờ
//     1 biến toàn cục __currentTracking mà mọi hàm trong cùng module
//     scope đều thấy được.
//   - Bản Rust: KHÔNG có "module scope" ngầm định như JS, nên state phải
//     được chia sẻ tường minh qua `SharedState = Rc<RefCell<State>>`.
//     1 subscriber giữ `Box<dyn Fn(&SharedState)>` — hàm Rust thuần được
//     biên dịch sẵn (không phải eval string) — nhận `&SharedState` (chứ
//     không phải `&State`/`&mut State` trực tiếp) để có thể tự
//     `.borrow_mut()` bên trong khi cần gọi `get_tracked()` (đọc + track
//     dependency). Việc chạy lại subscriber (`run_subscriber`), gom batch
//     (`flush`), và đăng ký/huỷ đăng ký (`subscribe`/`unsubscribe`) đều
//     là HÀM TỰ DO (không phải method trên `State`) nhận `&SharedState`
//     làm tham số — lý do chi tiết xem doc-comment ngay phía trên nhóm
//     hàm đó, phần "SUBSCRIBER LIFECYCLE".
//
// WASM chạy single-threaded trong 1 tab, nên dùng RefCell/Rc thay vì
// Mutex/Arc — không cần đồng bộ hoá đa luồng, chỉ cần interior mutability
// để nhiều closure (event handler DOM) cùng truy cập 1 State dùng chung.
// ============================================================

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use crate::runtime::value::VbValue;

/// ID định danh 1 subscriber (binding đã đăng ký), tăng dần đơn điệu —
/// tương đương identity của object `sub` trong Set() ở bản JS (ở JS,
/// object reference đóng vai trò định danh; ở Rust ta cần 1 khoá tường minh).
pub type SubId = u64;

/// 1 binding đã đăng ký, tương đương `{ run, deps }` ở bản JS.
///
/// `run` nhận `&SharedState` (Rc<RefCell<State>>) — không phải `&State`/
/// `&mut State` trực tiếp — vì bên trong 1 binding thường cần gọi lại
/// các hàm track-aware (`get_tracked`) mà bản thân việc track cần mượn
/// `&mut State` tại đúng thời điểm đọc. Nhận `&SharedState` cho phép
/// closure tự `.borrow()`/`.borrow_mut()` khi cần, không phải lo circular
/// ownership vì `SharedState` được truyền vào từ NGOÀI (bởi các hàm tự do
/// `subscribe()`/`run_subscriber()`/`flush()` bên dưới), không lưu bên
/// trong chính `State`.
struct Subscriber {
    run: Box<dyn Fn(&SharedState)>,
    deps: HashSet<String>,
}

/// Store toàn bộ trạng thái phản ứng của 1 trang — tương đương gộp
/// __state + __vars + __subscribers + __keyIndex + __currentTracking
/// của bản JS cũ vào 1 struct duy nhất.
///
/// Bọc trong `Rc<RefCell<..>>` ở nơi dùng (xem `SharedState` bên dưới)
/// vì nhiều closure DOM callback (onclick, oninput...) cần cùng truy cập
/// và mutate được State — Rust không cho phép nhiều `&mut` cùng lúc nên
/// cần RefCell để dời việc kiểm tra borrow xuống runtime, giống hệt lý do
/// JS không cần lo chuyện này (JS luôn single-threaded, mutable-by-default).
pub struct State {
    state: HashMap<String, VbValue>,
    vars: HashMap<String, VbValue>,

    subscribers: HashMap<SubId, Subscriber>,
    next_sub_id: SubId,
    key_index: HashMap<String, HashSet<SubId>>,

    /// Tương đương __currentTracking — subscriber đang chạy dở, để get()
    /// biết cần ghi nhận field vừa đọc vào deps của ai.
    current_tracking: Option<SubId>,

    /// Tương đương __pendingKeys — gom batch trong 1 "tick" trước khi
    /// notify. Ở JS dùng queueMicrotask; ở đây caller (runtime/dom.rs)
    /// chủ động gọi `flush()` sau mỗi lần xử lý xong 1 event/callback,
    /// vì WASM không có microtask queue tiện dụng như JS — nhưng hiệu
    /// ứng cuối cùng giống nhau: nhiều set_state() liên tiếp trong cùng
    /// 1 event handler chỉ re-render 1 lần khi flush() được gọi.
    pending_keys: HashSet<String>,

    // ── Scope stacks (component props / vong_lap) ─────────────────────
    loop_scope_stack: Vec<LoopFrame>,
    component_scope_stack: Vec<String>,
    /// id -> (tên prop -> getter). Getter là closure Rust, không phải JS
    /// function, nên props vẫn "sống" (đọc lại state cha mỗi lần gọi)
    /// giống hệt lý do bản JS dùng `() => giá_trị` thay vì giá trị tĩnh.
    component_props: HashMap<String, HashMap<String, Box<dyn Fn(&State) -> VbValue>>>,

    /// Base URL cho goi_api() — tương đương __api.baseURL ở bản JS cũ,
    /// nạp từ optsJson lúc __vb.boot() (xem dom.rs::VbRuntime::new).
    base_url: String,
}

/// 1 "khung" scope của vòng lặp — tương đương `{ [itemVar]: item, ... }`
/// ở bản JS. Giữ tường minh item_var/index_var thay vì object động vì
/// Rust cần biết trước shape, không thể "tuỳ ý thêm field" như JS.
#[derive(Clone)]
pub struct LoopFrame {
    pub item_var: String,
    pub item_value: VbValue,
    pub index_var: Option<String>,
    pub index_value: Option<f64>,
}

impl State {
    pub fn new() -> Self {
        State {
            state: HashMap::new(),
            vars: HashMap::new(),
            subscribers: HashMap::new(),
            next_sub_id: 0,
            key_index: HashMap::new(),
            current_tracking: None,
            pending_keys: HashSet::new(),
            loop_scope_stack: Vec::new(),
            component_scope_stack: Vec::new(),
            component_props: HashMap::new(),
            base_url: String::new(),
        }
    }

    // ════════════════════════════════════════════════════════════
    // STATE STORE — tương đương section 1 của bản JS
    // ════════════════════════════════════════════════════════════

    /// Tương đương __setState. So sánh bằng giá trị (VbValue: PartialEq)
    /// thay vì reference identity JS (`old === value`) — với dữ liệu
    /// scalar/mảng/object nhỏ điều này cho kết quả tương đương thực tế;
    /// khác biệt duy nhất là 2 object *khác reference nhưng cùng giá trị*
    /// ở JS sẽ trigger re-render còn ở đây thì không — đây là cải thiện,
    /// không phải regression (JS side vốn coi đó là "bug tiềm ẩn" cần
    /// tránh bằng cách luôn tạo object mới khi đổi, xem comment gốc).
    pub fn set_state(&mut self, key: &str, value: VbValue) {
        if let Some(old) = self.state.get(key) {
            if old == &value {
                return;
            }
        }
        self.state.insert(key.to_string(), value);
        self.pending_keys.insert(key.to_string());
    }

    /// Tương đương __getState — đọc giá trị VÀ ghi nhận dependency nếu
    /// đang có 1 subscriber chạy dở. Method này vẫn nằm trên `State`
    /// (nhận `&mut self`) vì bản thân việc track chỉ cần mutate nội bộ
    /// struct này (current_tracking + subscribers[id].deps), không cần
    /// gọi lại subscriber nào khác — khác với `subscribe`/`flush` (bên
    /// dưới, ngoài impl block) vốn cần gọi `(sub.run)(&SharedState)`.
    pub fn get_state(&mut self, key: &str) -> VbValue {
        if let Some(sub_id) = self.current_tracking {
            if let Some(sub) = self.subscribers.get_mut(&sub_id) {
                sub.deps.insert(key.to_string());
            }
        }
        self.state.get(key).cloned().unwrap_or(VbValue::Null)
    }

    /// Đọc thẳng không track — dùng nội bộ nơi không cần reactive
    /// (vd hiển thị debug, hoặc đọc bên ngoài 1 subscriber run).
    pub fn peek_state(&self, key: &str) -> VbValue {
        self.state.get(key).cloned().unwrap_or(VbValue::Null)
    }

    pub fn get_var(&self, key: &str) -> VbValue {
        self.vars.get(key).cloned().unwrap_or(VbValue::Null)
    }

    pub fn set_var(&mut self, key: &str, value: VbValue) {
        self.vars.insert(key.to_string(), value);
    }

    // ── Batch notify ───────────────────────────────────────────────
    // Bản JS dùng queueMicrotask để tự động gom nhiều set_state() liên
    // tiếp. Rust/WASM không có cách "tự trigger" tương đương an toàn mà
    // không thêm dependency ngoài (setTimeout(0) qua web-sys là lựa chọn
    // nhưng thêm độ trễ 1 tick thật). Thay vào đó: mọi entrypoint gọi vào
    // Rust từ JS (event handler, __setState do action gọi...) PHẢI kết
    // thúc bằng flush(&shared_state) — hàm tự do bên dưới impl block này
    // (xem lý do tách ra khỏi impl trong doc-comment của nó).

    fn index_sub(&mut self, id: SubId, deps: &HashSet<String>) {
        for key in deps {
            self.key_index.entry(key.clone()).or_default().insert(id);
        }
    }

    fn unindex_sub(&mut self, id: SubId, deps: &HashSet<String>) {
        for key in deps {
            if let Some(set) = self.key_index.get_mut(key) {
                set.remove(&id);
            }
        }
    }

    // ════════════════════════════════════════════════════════════
    // MUTATION HELPERS — tương đương section 2 của bản JS
    // ════════════════════════════════════════════════════════════

    /// $ds.them(item)
    pub fn state_push(&mut self, key: &str, item: VbValue) {
        let arr = match self.state.get(key) {
            Some(VbValue::Array(a)) => a.clone(),
            _ => {
                crate::runtime::log::warn(&format!(
                    "[ViBao] \"${}.them()\" gọi trên giá trị không phải mảng",
                    key
                ));
                return;
            }
        };
        let mut next = arr;
        next.push(item);
        self.set_state(key, VbValue::Array(next));
    }

    /// $ds.xoa(index) — chỉ hỗ trợ xoá theo index số, tương đương nhánh
    /// `typeof indexOrItem === 'number'` ở bản JS. Xoá-theo-giá-trị (nhánh
    /// còn lại ở bản JS) tách thành `state_remove_matching` bên dưới vì
    /// Rust cần kiểu tường minh cho tham số thay vì "number hoặc object".
    pub fn state_remove_by_index(&mut self, key: &str, index: usize) {
        let arr = match self.state.get(key) {
            Some(VbValue::Array(a)) => a.clone(),
            _ => return,
        };
        let next: Vec<VbValue> = arr
            .into_iter()
            .enumerate()
            .filter(|(i, _)| *i != index)
            .map(|(_, v)| v)
            .collect();
        self.set_state(key, VbValue::Array(next));
    }

    /// $ds.xoa(item) — xoá theo so khớp: nếu item có field "id" thì so
    /// theo id, ngược lại so bằng giá trị toàn phần (strict_eq).
    pub fn state_remove_matching(&mut self, key: &str, target: &VbValue) {
        let arr = match self.state.get(key) {
            Some(VbValue::Array(a)) => a.clone(),
            _ => return,
        };
        let target_id = target.as_object().and_then(|o| o.get("id"));
        let next: Vec<VbValue> = arr
            .into_iter()
            .filter(|it| {
                if let (Some(tid), Some(it_obj)) = (target_id, it.as_object()) {
                    if let Some(iid) = it_obj.get("id") {
                        return iid != tid;
                    }
                }
                it != target
            })
            .collect();
        self.set_state(key, VbValue::Array(next));
    }

    /// $ds.xoa_het()
    pub fn state_clear(&mut self, key: &str) {
        self.set_state(key, VbValue::Array(Vec::new()));
    }

    /// $ds.cap_nhat(index, newValue)
    pub fn state_update(&mut self, key: &str, index: usize, new_value: VbValue) {
        let arr = match self.state.get(key) {
            Some(VbValue::Array(a)) => a.clone(),
            _ => return,
        };
        let next: Vec<VbValue> = arr
            .into_iter()
            .enumerate()
            .map(|(i, v)| if i == index { new_value.clone() } else { v })
            .collect();
        self.set_state(key, VbValue::Array(next));
    }

    /// $obj.field = value — shallow copy giữ immutability, tương đương
    /// __stateSetField. Ràng buộc thiết kế giống hệt bản JS: mutate sâu
    /// bên trong object lấy ra từ get_state() sẽ KHÔNG kích hoạt re-render,
    /// mọi thay đổi phải đi qua các hàm state_* này.
    pub fn state_set_field(&mut self, key: &str, field: &str, value: VbValue) {
        let obj = match self.state.get(key) {
            Some(VbValue::Object(o)) => o.clone(),
            _ => {
                crate::runtime::log::warn(&format!(
                    "[ViBao] \"${}.{} = ...\" gọi trên giá trị không phải object",
                    key, field
                ));
                return;
            }
        };
        let mut next = obj;
        next.insert(field.to_string(), value);
        self.set_state(key, VbValue::Object(next));
    }

    // ════════════════════════════════════════════════════════════
    // SCOPE RESOLUTION — tương đương section 3, 4, 5 của bản JS
    // (member access resolver, component props, loop scope)
    // ════════════════════════════════════════════════════════════

    pub fn push_loop_scope(&mut self, frame: LoopFrame) {
        self.loop_scope_stack.push(frame);
    }

    pub fn pop_loop_scope(&mut self) {
        self.loop_scope_stack.pop();
    }

    pub fn push_component_scope(&mut self, id: &str) {
        self.component_scope_stack.push(id.to_string());
    }

    /// Pop có guard, tương đương __popComponentScope — log lỗi thay vì
    /// pop nhầm nếu top không khớp id mong đợi.
    pub fn pop_component_scope(&mut self, expected_id: &str) {
        match self.component_scope_stack.last() {
            Some(top) if top == expected_id => {
                self.component_scope_stack.pop();
            }
            Some(top) => {
                crate::runtime::log::error(&format!(
                    "[ViBao] Component scope stack mismatch: expected \"{}\" nhưng top là \"{}\" — bỏ qua pop để tránh corrupt stack.",
                    expected_id, top
                ));
            }
            None => {}
        }
    }

    pub fn register_props(&mut self, id: &str, getters: HashMap<String, Box<dyn Fn(&State) -> VbValue>>) {
        self.component_props.insert(id.to_string(), getters);
    }

    pub fn unregister_props(&mut self, id: &str) {
        self.component_props.remove(id);
    }

    /// Tương đương __propScope: tìm trong component scope stack (từ trong
    /// ra ngoài), fallback về global state/vars.
    fn prop_scope(&self, name: &str) -> VbValue {
        for scope_id in self.component_scope_stack.iter().rev() {
            if let Some(getters) = self.component_props.get(scope_id) {
                if let Some(getter) = getters.get(name) {
                    return getter(self);
                }
            }
        }
        self.state
            .get(name)
            .or_else(|| self.vars.get(name))
            .cloned()
            .unwrap_or(VbValue::Null)
    }

    /// Tương đương __resolveRoot: thứ tự ưu tiên loop > @the props > global.
    /// Đây là root resolver cho __get()-style path access.
    pub fn resolve_root(&self, name: &str) -> VbValue {
        for frame in self.loop_scope_stack.iter().rev() {
            if frame.item_var == name {
                return frame.item_value.clone();
            }
            if let Some(idx_var) = &frame.index_var {
                if idx_var == name {
                    return frame.index_value.map(VbValue::Num).unwrap_or(VbValue::Null);
                }
            }
        }
        for scope_id in self.component_scope_stack.iter().rev() {
            if let Some(getters) = self.component_props.get(scope_id) {
                if let Some(getter) = getters.get(name) {
                    return getter(self);
                }
            }
        }
        self.state
            .get(name)
            .or_else(|| self.vars.get(name))
            .cloned()
            .unwrap_or(VbValue::Null)
    }

    /// Tương đương __scopeResolve: dùng bởi expr evaluator khi resolve
    /// 1 Variable("ten") — ưu tiên loop scope gần nhất (kể cả path lồng
    /// "item.ten"), rồi mới prop_scope.
    pub fn scope_resolve(&self, name: &str) -> VbValue {
        for frame in self.loop_scope_stack.iter().rev() {
            if frame.item_var == name {
                return frame.item_value.clone();
            }
            if let Some(idx_var) = &frame.index_var {
                if idx_var == name {
                    return frame.index_value.map(VbValue::Num).unwrap_or(VbValue::Null);
                }
            }
            let prefix = format!("{}.", frame.item_var);
            if let Some(sub_path) = name.strip_prefix(&prefix) {
                return frame.item_value.dig_path(sub_path);
            }
        }
        self.prop_scope(name)
    }

    /// Biến thể CÓ TRACK của `scope_resolve` — dùng bởi expr evaluator khi
    /// đang chạy bên trong 1 subscriber (binding if/loop/style động cần
    /// tự re-render lúc state đổi). Khác `scope_resolve` (&self, không
    /// track) ở đúng 1 điểm: khi tên biến không khớp loop scope nào và
    /// không khớp component prop nào, nó rơi về `get_state()` (CÓ ghi
    /// dependency) thay vì đọc thẳng `self.state.get()` (không ghi gì).
    ///
    /// Biến trong loop scope / component props KHÔNG cần track qua đường
    /// này — vì bản thân loop re-render toàn bộ thân vòng lặp mỗi khi
    /// danh sách đổi (xem control.rs codegen), và component props là
    /// getter đóng gói sẵn logic track riêng của nó (nếu getter đó có gọi
    /// get_tracked bên trong).
    pub fn scope_resolve_tracked(&mut self, name: &str) -> VbValue {
        for frame in self.loop_scope_stack.clone().iter().rev() {
            if frame.item_var == name {
                return frame.item_value.clone();
            }
            if let Some(idx_var) = &frame.index_var {
                if idx_var == name {
                    return frame.index_value.map(VbValue::Num).unwrap_or(VbValue::Null);
                }
            }
            let prefix = format!("{}.", frame.item_var);
            if let Some(sub_path) = name.strip_prefix(&prefix) {
                return frame.item_value.dig_path(sub_path);
            }
        }
        // Component props: getter tự quyết định có track hay không (nó
        // nhận &State, không phải &mut, nên không thể tự gọi get_state()
        // — nếu prop cần reactive theo state cha, getter nên được viết để
        // đọc qua 1 cơ chế khác; đây là giới hạn đã có sẵn từ thiết kế
        // register_props(), không phải điều evaluator có thể sửa được).
        for scope_id in self.component_scope_stack.clone().iter().rev() {
            if let Some(getters) = self.component_props.get(scope_id) {
                if let Some(getter) = getters.get(name) {
                    return getter(self);
                }
            }
        }
        // Global: đây là nhánh DUY NHẤT khác `scope_resolve` — dùng
        // get_state() để ghi dependency vào subscriber đang chạy dở.
        self.get_state(name)
    }

    /// __get(path) — resolve path đầy đủ bắt đầu từ root đúng thứ tự
    /// ưu tiên scope, rồi dig sâu các phần còn lại.
    pub fn get_path(&self, path: &str) -> VbValue {
        let mut parts = path.split('.');
        let root_name = match parts.next() {
            Some(p) => p,
            None => return VbValue::Null,
        };
        let mut cur = self.resolve_root(root_name);
        for part in parts {
            if cur.is_null() {
                return VbValue::Null;
            }
            cur = cur.get_field(part);
        }
        cur
    }

    // ════════════════════════════════════════════════════════════
    // INIT / RESET — tương đương section 7 của bản JS
    // ════════════════════════════════════════════════════════════

    /// Tương đương __initPageState: reset toàn bộ subscriber + state khi
    /// chuyển trang trong SPA.
    pub fn init_page_state(&mut self, initial: HashMap<String, VbValue>) {
        self.subscribers.clear();
        self.key_index.clear();
        self.pending_keys.clear();
        self.state.clear();
        self.state.extend(initial);
    }

    /// Tương đương __initGlobalVars: chạy 1 lần lúc app boot, không reset
    /// khi đổi trang.
    pub fn init_global_vars(&mut self, initial: HashMap<String, VbValue>) {
        self.vars.extend(initial);
    }

    // ── Devtools ─────────────────────────────────────────────────────

    pub fn inspect_state(&self) -> VbValue {
        VbValue::Object(self.state.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
    }

    pub fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }

    // ── Base URL (dùng bởi goi_api) ─────────────────────────────────

    pub fn set_base_url(&mut self, url: String) {
        self.base_url = url;
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

/// State dùng chung xuyên suốt runtime — mọi closure DOM callback (event
/// handler, binding) đều clone `Rc` này để cùng truy cập 1 State. Tương
/// đương việc mọi hàm ở bản JS đều đóng trong cùng 1 IIFE scope và chia
/// sẻ ngầm __state/__subscribers qua closure — ở Rust phải tường minh
/// hoá bằng Rc<RefCell<..>> vì không có "module scope" ngầm định như JS.
pub type SharedState = Rc<RefCell<State>>;

pub fn new_shared_state() -> SharedState {
    Rc::new(RefCell::new(State::new()))
}

// ════════════════════════════════════════════════════════════
// SUBSCRIBER LIFECYCLE — hàm TỰ DO (không phải method trên State)
// ════════════════════════════════════════════════════════════
//
// Lý do tách khỏi `impl State`: `Subscriber::run` cần được gọi với
// `&SharedState` để bản thân binding có thể `.borrow_mut()` và gọi lại
// `get_state()` (track dependency) ngay trong lúc chạy. Nếu các hàm này
// là method `&mut self` trên `State`, việc gọi `(sub.run)(&shared)` đòi
// hỏi ta phải tạo ra 1 `&SharedState` trỏ ngược lại chính `self` — tức
// `State` phải tự biết `Rc<RefCell<Self>>` bọc quanh nó, một vòng tham
// chiếu không cần thiết. Thay vào đó, các hàm dưới đây nhận `&SharedState`
// từ BÊN NGOÀI (do người gọi — runtime/dom.rs — cung cấp, vì họ vốn đã
// giữ sẵn 1 bản `Rc` này), nên không có vòng tham chiếu nào cả.

/// Tương đương __subscribe. Trả về SubId để gọi `unsubscribe()` sau
/// (thay cho closure hủy đăng ký ở bản JS — Rust cần 1 khoá tường minh
/// vì không có identity ẩn kiểu object reference).
pub fn subscribe(shared: &SharedState, run: Box<dyn Fn(&SharedState)>) -> SubId {
    let id = {
        let mut state = shared.borrow_mut();
        let id = state.next_sub_id;
        state.next_sub_id += 1;
        state.subscribers.insert(
            id,
            Subscriber {
                run,
                deps: HashSet::new(),
            },
        );
        id
    };
    run_subscriber(shared, id); // chạy lần đầu ngay, giống bản JS
    id
}

pub fn unsubscribe(shared: &SharedState, id: SubId) {
    let mut state = shared.borrow_mut();
    if let Some(sub) = state.subscribers.remove(&id) {
        state.unindex_sub(id, &sub.deps);
    }
}

fn run_subscriber(shared: &SharedState, id: SubId) {
    // Lấy Subscriber ra khỏi map TRƯỚC khi gọi run — nếu không, khi
    // closure bên trong `run` gọi lại `shared.borrow_mut()` (vd để
    // get_state() track dependency), nó sẽ đụng phải borrow đang giữ
    // ở đây và panic "already borrowed" lúc chạy. Tách ra ngoài map rồi
    // mới gọi giúp borrow của bước "lấy sub ra" đã kết thúc từ trước.
    let mut sub = {
        let mut state = shared.borrow_mut();
        match state.subscribers.remove(&id) {
            Some(s) => {
                state.unindex_sub(id, &s.deps);
                s
            }
            None => return, // đã bị unsubscribe trong lúc pending
        }
    };

    let prev_tracking = {
        let mut state = shared.borrow_mut();
        let prev = state.current_tracking;
        state.current_tracking = Some(id);
        prev
    };

    // Deps được rebuild từ đầu mỗi lần chạy lại — CHỦ ĐÍCH (xem comment
    // gốc ở bản JS): nhánh phụ thuộc có thể đổi theo điều kiện runtime.
    sub.deps.clear();

    (sub.run)(shared);

    let mut state = shared.borrow_mut();
    state.current_tracking = prev_tracking;
    state.index_sub(id, &sub.deps);
    state.subscribers.insert(id, sub);
}

/// Tương đương thân của __scheduleNotify's queueMicrotask callback,
/// nhưng gọi tường minh thay vì tự động (xem lý do ở doc-comment của
/// field `pending_keys`). Idempotent nếu không có gì pending.
pub fn flush(shared: &SharedState) {
    let keys: Vec<String> = {
        let mut state = shared.borrow_mut();
        if state.pending_keys.is_empty() {
            return;
        }
        state.pending_keys.drain().collect()
    };

    let to_run: HashSet<SubId> = {
        let state = shared.borrow();
        let mut set = HashSet::new();
        for key in &keys {
            if let Some(subs) = state.key_index.get(key) {
                set.extend(subs.iter().copied());
            }
        }
        for (id, sub) in &state.subscribers {
            if sub.deps.is_empty() {
                set.insert(*id);
            }
        }
        set
    };

    for id in to_run {
        run_subscriber(shared, id);
    }
}

/// Đọc state VÀ track dependency, dùng bởi expr evaluator (module tiếp
/// theo) khi đang chạy bên trong 1 subscriber. Tương đương gọi trực tiếp
/// `__getState()` ở bản JS. Nhận `&SharedState` để khớp chữ ký mà
/// binding closures nhận được.
pub fn get_tracked(shared: &SharedState, key: &str) -> VbValue {
    shared.borrow_mut().get_state(key)
}

/// Đọc base_url hiện tại — dùng bởi action.rs khi thực thi goi_api().
pub fn get_base_url(shared: &SharedState) -> String {
    shared.borrow().base_url().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_get_state_roundtrip() {
        let mut state = State::new();
        state.set_state("dem", VbValue::num(1.0));
        assert_eq!(state.peek_state("dem").as_num(), Some(1.0));
    }

    #[test]
    fn test_set_state_same_value_does_not_mark_pending() {
        let mut state = State::new();
        state.set_state("dem", VbValue::num(1.0));
        state.pending_keys.clear(); // giả lập đã flush xong
        state.set_state("dem", VbValue::num(1.0)); // giá trị không đổi
        assert!(state.pending_keys.is_empty());
    }

    #[test]
    fn test_subscriber_reruns_on_dependency_change() {
        let shared = new_shared_state();
        let run_count = Rc::new(RefCell::new(0));

        shared.borrow_mut().set_state("n", VbValue::num(1.0));

        let run_count_clone = run_count.clone();
        let sub_id = subscribe(
            &shared,
            Box::new(move |sh: &SharedState| {
                *run_count_clone.borrow_mut() += 1;
                let _ = get_tracked(sh, "n"); // đọc + track "n" như 1 binding thật
            }),
        );
        // Lần chạy đầu tiên (subscribe tự chạy 1 lần)
        assert_eq!(*run_count.borrow(), 1);

        // Đổi "n" rồi flush -> subscriber phải chạy lại vì đã track "n"
        shared.borrow_mut().set_state("n", VbValue::num(2.0));
        flush(&shared);
        assert_eq!(*run_count.borrow(), 2);

        unsubscribe(&shared, sub_id);

        // Sau unsubscribe, đổi state không còn kích hoạt run nữa
        shared.borrow_mut().set_state("n", VbValue::num(3.0));
        flush(&shared);
        assert_eq!(*run_count.borrow(), 2);
    }

    #[test]
    fn test_auto_tracking_via_get_state() {
        let shared = new_shared_state();
        shared.borrow_mut().set_state("n", VbValue::num(1.0));

        let seen = Rc::new(RefCell::new(0.0));
        let seen_clone = seen.clone();

        subscribe(
            &shared,
            Box::new(move |sh: &SharedState| {
                let v = get_tracked(sh, "n"); // đọc + track "n"
                *seen_clone.borrow_mut() = v.as_num().unwrap_or(0.0);
            }),
        );

        assert_eq!(*seen.borrow(), 1.0);

        shared.borrow_mut().set_state("n", VbValue::num(2.0));
        flush(&shared);
        assert_eq!(*seen.borrow(), 2.0); // rerun tự động vì đã track "n"
    }

    #[test]
    fn test_state_push_and_remove() {
        let mut state = State::new();
        state.set_state("ds", VbValue::Array(vec![VbValue::num(1.0)]));
        state.state_push("ds", VbValue::num(2.0));
        assert_eq!(
            state.peek_state("ds").as_array().map(|a| a.len()),
            Some(2)
        );

        state.state_remove_by_index("ds", 0);
        let arr = state.peek_state("ds");
        assert_eq!(arr.as_array().unwrap()[0].as_num(), Some(2.0));
    }

    #[test]
    fn test_state_set_field_shallow_copy() {
        let mut state = State::new();
        let mut obj = std::collections::BTreeMap::new();
        obj.insert("ten".to_string(), VbValue::str("An"));
        state.set_state("nguoi_dung", VbValue::Object(obj));

        state.state_set_field("nguoi_dung", "ten", VbValue::str("Binh"));
        let updated = state.peek_state("nguoi_dung");
        assert_eq!(
            updated.as_object().unwrap().get("ten").unwrap().as_str(),
            Some("Binh")
        );
    }

    #[test]
    fn test_loop_scope_resolve_priority_over_global() {
        let mut state = State::new();
        state.set_state("item", VbValue::str("global"));
        state.push_loop_scope(LoopFrame {
            item_var: "item".to_string(),
            item_value: VbValue::str("local"),
            index_var: None,
            index_value: None,
        });

        assert_eq!(state.scope_resolve("item").as_str(), Some("local"));
        state.pop_loop_scope();
        assert_eq!(state.scope_resolve("item").as_str(), Some("global"));
    }

    #[test]
    fn test_get_path_nested_with_special_field() {
        let mut state = State::new();
        state.set_state(
            "ds",
            VbValue::Array(vec![VbValue::num(1.0), VbValue::num(2.0)]),
        );
        assert_eq!(state.get_path("ds.do_dai").as_num(), Some(2.0));
    }

    #[test]
    fn test_component_scope_mismatch_does_not_corrupt_stack() {
        let mut state = State::new();
        state.push_component_scope("a");
        state.push_component_scope("b");
        // Pop sai id — phải bị từ chối, stack giữ nguyên [a, b]
        state.pop_component_scope("a");
        assert_eq!(state.component_scope_stack.last().map(|s| s.as_str()), Some("b"));
        state.pop_component_scope("b");
        assert_eq!(state.component_scope_stack.last().map(|s| s.as_str()), Some("a"));
    }
}
