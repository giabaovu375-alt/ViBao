// ============================================================
// VIBAO COMPILER (Rust) — codegen/expr.rs
// Biên dịch Expr (AST biểu thức) thành chuỗi mã JS thực thi được
// ở runtime trong trình duyệt. Tương đương exprToJS/templateToJS/
// resolveValue của 06-parser-expr.ts (bản TS cũ đặt cùng chỗ với
// expression parsing, nhưng ở đây ta tách riêng vì đây thực chất
// là một bước CODEGEN chứ không phải parsing).
//
// ── GHI CHÚ VỀ EXPR REGISTRY (Rust/WASM evaluator) ─────────────────
// `expr_to_js()` (giữ NGUYÊN không đổi, xem bên dưới) sinh JS thuần —
// cách làm gốc, nơi biểu thức được đánh giá bằng chính JS runtime.
// Bên cạnh đó, thêm `expr_to_js_registry()` — hàm MỚI, dùng khi bạn
// muốn biểu thức được ĐÁNH GIÁ BỞI RUST/WASM thay vì JS: thay vì sinh
// ra "__s.n + 1" (JS tính), nó ĐĂNG KÝ nguyên cây `Expr` vào 1 registry
// nội bộ và sinh ra "__vb.evalExpr(<id>)" — lúc chạy, JS gọi vào WASM,
// WASM tự deserialize lại Expr đã đăng ký và evaluator Rust (module
// runtime/expr_eval.rs bên crate vibao-runtime) tính ra kết quả.
//
// Registry dùng `thread_local!` (không phải tham số `&mut ctx` truyền
// xuyên suốt mọi hàm expr_to_js con) — vì codegen chạy đơn luồng, 1 lần
// mỗi lượt build, nên không cần đồng bộ đa luồng; đổi 74+ lệnh gọi
// expr_to_js_default() rải khắp 7 file codegen khác để truyền thêm
// context là chi phí lớn không cần thiết cho lợi ích tương đương.
// Sau khi build xong 1 trang, gọi `take_expr_registry()` để lấy toàn bộ
// bảng {id -> Expr} đã tích luỹ, serialize ra JSON, nhúng vào output
// (qua __vb.boot(...)) để WASM nạp lúc trang load.
// ============================================================

use std::cell::RefCell;

use vibao_ast::{BinaryOp, ColorFuncKind, Expr, LiteralValue, TemplatePart, UnaryOp};

thread_local! {
    /// Bảng tích luỹ toàn bộ Expr đã đăng ký trong lượt build hiện tại.
    /// Index trong Vec CHÍNH LÀ id dùng ở "__vb.evalExpr(id)" — đơn giản
    /// hơn HashMap vì id chỉ cần duy nhất trong 1 lần build, không cần
    /// ổn định giữa các lần build khác nhau.
    static EXPR_REGISTRY: RefCell<Vec<Expr>> = RefCell::new(Vec::new());
}

/// Đăng ký 1 Expr vào registry, trả về id (index) để nhúng vào JS như
/// "__vb.evalExpr(<id>)". Dùng bởi `expr_to_js_registry()` bên dưới.
pub fn register_expr(expr: Expr) -> usize {
    EXPR_REGISTRY.with(|reg| {
        let mut reg = reg.borrow_mut();
        reg.push(expr);
        reg.len() - 1
    })
}

/// Lấy toàn bộ registry đã tích luỹ VÀ xoá sạch nó (reset cho lượt build
/// tiếp theo — quan trọng nếu `main.rs` build nhiều trang trong 1 lần
/// chạy process, tránh registry của trang này lẫn sang trang khác).
/// Gọi hàm này SAU KHI đã sinh xong toàn bộ JS cho 1 trang, rồi
/// serialize kết quả trả về (Vec<Expr>) ra JSON để nhúng vào output.
pub fn take_expr_registry() -> Vec<Expr> {
    EXPR_REGISTRY.with(|reg| std::mem::take(&mut *reg.borrow_mut()))
}

/// Biên dịch 1 Expr thành lời gọi WASM evaluator thay vì JS thuần.
/// Luôn đăng ký toàn bộ `expr` (không tách nhỏ literal ra như
/// `resolve_value()`), vì mục đích của hàm này là "biểu thức này cần
/// Rust tính", bất kể cây con bên trong có phần literal hay không —
/// việc tối ưu tách literal ra khỏi cây trước khi đăng ký (nếu cần) nên
/// làm ở tầng gọi (caller), không phải ở đây.
pub fn expr_to_js_registry(expr: &Expr) -> String {
    let id = register_expr(expr.clone());
    format!("__vb.evalExpr({})", id)
}

/// Prefix mặc định cho biến state khi sinh JS — "__s.ten_bien". Được
/// tham số hoá (thay vì hard-code) để tương lai có thể sinh code cho
/// scope khác (vd props của component custom) mà không sửa hàm này.
pub const DEFAULT_STATE_PREFIX: &str = "__s";

/// Biên dịch 1 Expr thành chuỗi JS. `state_prefix` quyết định biến ViBao
/// ($ten) sẽ trỏ vào object nào ở runtime (mặc định "__s" = state hiện
/// tại của trang).
pub fn expr_to_js(expr: &Expr, state_prefix: &str) -> String {
    match expr {
        Expr::Literal(lit, _) => literal_to_js(lit),

        Expr::Variable(name, _) => format!("{}.{}", state_prefix, name),

        Expr::MemberAccess { object, property, .. } => {
            let obj = expr_to_js(object, state_prefix);
            // Vài "field" tiếng Việt đặc biệt trên mảng/chuỗi được ánh xạ
            // sang thuộc tính JS thật tương ứng — xem propMap ở bản TS cũ.
            match property.as_str() {
                "rong" => format!("({}.length === 0)", obj),
                "do_dai" => format!("{}.length", obj),
                other => format!("{}.{}", obj, other),
            }
        }

        Expr::Binary { op, left, right, .. } => {
            let l = expr_to_js(left, state_prefix);
            let r = expr_to_js(right, state_prefix);
            format!("({} {} {})", l, binary_op_js(*op), r)
        }

        Expr::Unary { op, operand, .. } => {
            let o = expr_to_js(operand, state_prefix);
            match op {
                UnaryOp::Not => format!("!{}", o),
                UnaryOp::Neg => format!("-{}", o),
            }
        }

        Expr::Call { callee, args, .. } => {
            let args_js = args
                .iter()
                .map(|a| expr_to_js(a, state_prefix))
                .collect::<Vec<_>>()
                .join(", ");
            let js_fn = map_call_fn(callee);
            format!("{}({})", js_fn, args_js)
        }

        Expr::ColorFunc { func, color, amount, .. } => {
            let color_js = expr_to_js(color, state_prefix);
            format!("{}({}, {})", map_color_fn(*func), color_js, amount)
        }

        Expr::Array(items, _) => {
            let items_js = items
                .iter()
                .map(|e| expr_to_js(e, state_prefix))
                .collect::<Vec<_>>()
                .join(", ");
            format!("[{}]", items_js)
        }

        Expr::Object(fields, _) => {
            let fields_js = fields
                .iter()
                .map(|(k, v)| format!("{}: {}", json_string(k), expr_to_js(v, state_prefix)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{ {} }}", fields_js)
        }

        Expr::TemplateString(parts, _) => template_to_js(parts, state_prefix),
    }
}

/// Tiện ích gọi expr_to_js với prefix mặc định "__s" — dùng ở hầu hết
/// mọi nơi trong codegen không cần scope đặc biệt.
pub fn expr_to_js_default(expr: &Expr) -> String {
    expr_to_js(expr, DEFAULT_STATE_PREFIX)
}

// ════════════════════════════════════════════════════════════
// LITERAL → JS
// ════════════════════════════════════════════════════════════

fn literal_to_js(lit: &LiteralValue) -> String {
    match lit {
        LiteralValue::Str(s) => json_string(s),
        LiteralValue::Color(c) => json_string(c),
        // Trong biểu thức JS thuần (vd $n - 1, gia_tien($x)), đơn vị CSS
        // không có ý nghĩa — chỉ phần số được dùng. Đơn vị chỉ quan trọng
        // ở resolve_value() bên dưới, nơi giá trị được dùng làm CSS.
        LiteralValue::Num(n, _unit) => format_number(*n),
        LiteralValue::Bool(b) => b.to_string(),
    }
}

/// Định dạng số sao cho giống output của JS `String(number)` — số nguyên
/// không có phần thập phân dư (vd 5.0 → "5", không phải "5.0").
fn format_number(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        n.to_string()
    }
}

/// Escape 1 chuỗi Rust thành JS string literal hợp lệ (giống
/// `JSON.stringify` trong bản TS cũ) — bọc trong dấu " và escape các ký
/// tự đặc biệt (", \, xuống dòng).
pub fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

// ════════════════════════════════════════════════════════════
// BINARY OP → JS OPERATOR
// ════════════════════════════════════════════════════════════

fn binary_op_js(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Mod => "%",
        // "==" / "!=" của ViBao luôn biên dịch ra "===" / "!==" — tránh
        // toàn bộ lớp bug type coercion ngầm định của so sánh lỏng JS.
        BinaryOp::Eq => "===",
        BinaryOp::Neq => "!==",
        BinaryOp::Gt => ">",
        BinaryOp::Gte => ">=",
        BinaryOp::Lt => "<",
        BinaryOp::Lte => "<=",
        BinaryOp::And => "&&",
        BinaryOp::Or => "||",
    }
}

// ════════════════════════════════════════════════════════════
// FUNCTION NAME MAPPING (utility functions & color functions)
// ════════════════════════════════════════════════════════════

/// Ánh xạ tên hàm tiện ích ViBao (viết bằng tiếng Việt) sang tên hàm
/// thực thi tương ứng trong runtime JS (__fmt.*). Hàm không nằm trong
/// bảng (custom action, ...) được giữ nguyên tên gốc.
fn map_call_fn(callee: &str) -> String {
    match callee {
        "gia_tien" => "__fmt.giaTien".to_string(),
        "ngay" => "__fmt.ngay".to_string(),
        "rut_gon" => "__fmt.rutGon".to_string(),
        "hoa_chu" => "__fmt.hoaChu".to_string(),
        "phan_tram" => "__fmt.phanTram".to_string(),
        "lam_tron" => "Math.round".to_string(),
        other => other.to_string(),
    }
}

fn map_color_fn(func: ColorFuncKind) -> &'static str {
    match func {
        ColorFuncKind::TrongSuot => "__color.trongSuot",
        ColorFuncKind::LamSang => "__color.lamSang",
        ColorFuncKind::LamToi => "__color.lamToi",
    }
}

// ════════════════════════════════════════════════════════════
// TEMPLATE STRING → JS TEMPLATE LITERAL
// ════════════════════════════════════════════════════════════

/// Biên dịch các phần đã tách của 1 template string ("Xin chào $ten")
/// thành 1 JS template literal hoàn chỉnh, bọc trong dấu backtick.
pub fn template_to_js(parts: &[TemplatePart], state_prefix: &str) -> String {
    let mut inner = String::new();
    for part in parts {
        match part {
            TemplatePart::Text(text) => {
                // Escape backtick và ${ để không phá vỡ cú pháp template
                // literal JS khi text người dùng chứa các ký tự này.
                inner.push_str(&escape_template_text(text));
            }
            TemplatePart::Variable(name) => {
                inner.push_str(&format!(
                    "${{{}.{} ?? __vars.{} ?? ''}}",
                    state_prefix, name, name
                ));
            }
            TemplatePart::Member(path) => {
                inner.push_str(&format!("${{__get('{}') ?? ''}}", path.join(".")));
            }
        }
    }
    format!("`{}`", inner)
}

fn escape_template_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('`', "\\`")
        .replace("${", "\\${")
}

// ════════════════════════════════════════════════════════════
// RESOLVED VALUE — dùng ở props.rs để quyết định style tĩnh vs động
// ════════════════════════════════════════════════════════════

/// Kết quả phân giải 1 Expr trong ngữ cảnh 1 giá trị prop — có thể là
/// hằng số tĩnh (biết ngay lúc biên dịch) hoặc biểu thức động (chỉ biết
/// lúc chạy, cần binding qua data-vb-bind-*).
#[derive(Debug, Clone)]
pub enum ResolvedValue {
    /// Giá trị chuỗi tĩnh dùng trực tiếp (không phải số/màu, vd "row").
    Static(String),
    /// Kích thước CSS đã có đơn vị (vd "16px", "50%").
    Size(String),
    /// Màu đã resolve ra mã CSS hợp lệ (hex hoặc var(--...)).
    Color(String),
    /// Biểu thức động — `js` là mã JS cần binding lúc runtime.
    Dynamic(String),
}

impl ResolvedValue {
    /// Trả về chuỗi CSS tương ứng cho các biến thể tĩnh (Static/Size/
    /// Color) — dùng khi caller đã biết chắc giá trị không phải Dynamic.
    /// Cho Dynamic, trả về chuỗi rỗng (caller phải tự xử lý binding).
    pub fn as_css(&self) -> String {
        match self {
            ResolvedValue::Static(s) => s.clone(),
            ResolvedValue::Size(s) => s.clone(),
            ResolvedValue::Color(s) => s.clone(),
            ResolvedValue::Dynamic(_) => String::new(),
        }
    }

    pub fn is_dynamic(&self) -> bool {
        matches!(self, ResolvedValue::Dynamic(_))
    }
}

/// Phân giải 1 Expr thành ResolvedValue — tương đương resolveValue() ở
/// bản TS cũ. Literal số/màu được xử lý đặc biệt (thêm đơn vị px, resolve
/// tên màu ra hex); mọi biểu thức khác (biến, phép toán, gọi hàm...) đều
/// là Dynamic vì giá trị chỉ xác định được lúc runtime.
pub fn resolve_value(expr: &Expr) -> ResolvedValue {
    match expr {
        Expr::Literal(LiteralValue::Color(hex), _) => ResolvedValue::Color(hex.clone()),
        // Số có đơn vị CSS tường minh (vd "50%", "10vw") giữ nguyên đơn vị
        // đó; số trần (không đơn vị, vd "16") mặc định coi là px — khớp
        // hành vi resolveValue() ở bản TS cũ (lit.value luôn được nối "px"
        // trừ khi chuỗi gốc đã chứa %/vw/vh).
        Expr::Literal(LiteralValue::Num(n, Some(unit)), _) => {
            ResolvedValue::Size(format!("{}{}", format_number(*n), unit))
        }
        Expr::Literal(LiteralValue::Num(n, None), _) => {
            ResolvedValue::Size(format!("{}px", format_number(*n)))
        }
        Expr::Literal(LiteralValue::Str(s), _) => ResolvedValue::Static(s.clone()),
        Expr::Literal(LiteralValue::Bool(b), _) => ResolvedValue::Static(b.to_string()),
        Expr::TemplateString(parts, _) => {
            ResolvedValue::Dynamic(template_to_js(parts, DEFAULT_STATE_PREFIX))
        }
        // Variable/MemberAccess/Binary/Unary/Call/ColorFunc/Array/Object
        // đều không thể biết được lúc biên dịch — luôn Dynamic.
        other => ResolvedValue::Dynamic(expr_to_js_default(other)),
    }
}

/// Trích giá trị TĨNH của 1 Expr dưới dạng chuỗi thô, dùng ở những nơi
/// chỉ chấp nhận giá trị biết trước lúc biên dịch (vd resolveLayoutCSS).
/// Trả về "__dynamic__" cho biểu thức động — sentinel value giống hệt
/// bản TS cũ (getStaticValue), để các hàm gọi sau có thể `if v ==
/// "__dynamic__"` mà bỏ qua thay vì crash.
pub fn get_static_value(expr: &Expr) -> String {
    match expr {
        Expr::Literal(LiteralValue::Str(s), _) => s.clone(),
        // Giữ nguyên đơn vị trong chuỗi trả về (vd "50%", không chỉ "50")
        // vì layout.rs (px/size/spacing) tự kiểm tra bằng regex xem chuỗi
        // đã có đơn vị chưa trước khi quyết định có nối thêm "px" không —
        // y hệt cách bản TS cũ dùng String(lit.value) giữ nguyên hậu tố.
        Expr::Literal(LiteralValue::Num(n, Some(unit)), _) => format!("{}{}", format_number(*n), unit),
        Expr::Literal(LiteralValue::Num(n, None), _) => format_number(*n),
        Expr::Literal(LiteralValue::Bool(b), _) => b.to_string(),
        Expr::Literal(LiteralValue::Color(c), _) => c.clone(),
        _ => "__dynamic__".to_string(),
    }
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
    fn test_variable_to_js() {
        let e = Expr::Variable("dem".to_string(), p());
        assert_eq!(expr_to_js_default(&e), "__s.dem");
    }

    #[test]
    fn test_binary_eq_becomes_strict_eq() {
        let e = Expr::Binary {
            op: BinaryOp::Eq,
            left: Box::new(Expr::literal_num(1.0, p())),
            right: Box::new(Expr::literal_num(2.0, p())),
            pos: p(),
        };
        assert_eq!(expr_to_js_default(&e), "(1 === 2)");
    }

    #[test]
    fn test_member_access_rong_do_dai() {
        let base = Expr::Variable("ds".to_string(), p());
        let e1 = Expr::MemberAccess {
            object: Box::new(base.clone()),
            property: "rong".to_string(),
            pos: p(),
        };
        let e2 = Expr::MemberAccess {
            object: Box::new(base),
            property: "do_dai".to_string(),
            pos: p(),
        };
        assert_eq!(expr_to_js_default(&e1), "(__s.ds.length === 0)");
        assert_eq!(expr_to_js_default(&e2), "__s.ds.length");
    }

    #[test]
    fn test_call_fn_mapping() {
        let e = Expr::Call {
            callee: "gia_tien".to_string(),
            args: vec![Expr::Variable("gia".to_string(), p())],
            pos: p(),
        };
        assert_eq!(expr_to_js_default(&e), "__fmt.giaTien(__s.gia)");
    }

    #[test]
    fn test_call_fn_unmapped_kept_as_is() {
        let e = Expr::Call {
            callee: "ham_tuy_chinh".to_string(),
            args: vec![],
            pos: p(),
        };
        assert_eq!(expr_to_js_default(&e), "ham_tuy_chinh()");
    }

    #[test]
    fn test_color_func_mapping() {
        let e = Expr::ColorFunc {
            func: ColorFuncKind::TrongSuot,
            color: Box::new(Expr::Literal(LiteralValue::Color("#000000".to_string()), p())),
            amount: 50.0,
            pos: p(),
        };
        assert_eq!(expr_to_js_default(&e), "__color.trongSuot(\"#000000\", 50)");
    }

    #[test]
    fn test_template_string_to_js() {
        let parts = vec![
            TemplatePart::Text("Xin chào ".to_string()),
            TemplatePart::Variable("ten".to_string()),
        ];
        assert_eq!(
            template_to_js(&parts, "__s"),
            "`Xin chào ${__s.ten ?? __vars.ten ?? ''}`"
        );
    }

    #[test]
    fn test_template_member_path() {
        let parts = vec![TemplatePart::Member(vec!["obj".to_string(), "field".to_string()])];
        assert_eq!(template_to_js(&parts, "__s"), "`${__get('obj.field') ?? ''}`");
    }

    #[test]
    fn test_number_formatting_no_trailing_zero() {
        assert_eq!(format_number(5.0), "5");
        assert_eq!(format_number(5.5), "5.5");
    }

    #[test]
    fn test_json_string_escapes_quotes() {
        assert_eq!(json_string("a\"b"), "\"a\\\"b\"");
    }

    #[test]
    fn test_resolve_value_color_literal() {
        let e = Expr::Literal(LiteralValue::Color("#FFFFFF".to_string()), p());
        match resolve_value(&e) {
            ResolvedValue::Color(c) => assert_eq!(c, "#FFFFFF"),
            _ => panic!("Phải là Color"),
        }
    }

    #[test]
    fn test_resolve_value_number_gets_px() {
        let e = Expr::literal_num(16.0, p());
        match resolve_value(&e) {
            ResolvedValue::Size(s) => assert_eq!(s, "16px"),
            _ => panic!("Phải là Size"),
        }
    }

    #[test]
    fn test_resolve_value_variable_is_dynamic() {
        let e = Expr::Variable("n".to_string(), p());
        assert!(resolve_value(&e).is_dynamic());
    }

    #[test]
    fn test_get_static_value_dynamic_sentinel() {
        let e = Expr::Variable("n".to_string(), p());
        assert_eq!(get_static_value(&e), "__dynamic__");
    }

    // ── Expr registry (Rust/WASM evaluator) ─────────────────────────

    #[test]
    fn test_registry_assigns_sequential_ids() {
        // Mỗi test chạy trên 1 thread riêng (Rust test framework), và
        // registry là thread_local, nên không lo test khác "làm bẩn"
        // id — nhưng để chắc chắn ta không giả định id bắt đầu từ 0,
        // chỉ kiểm tra id thứ 2 = id thứ 1 + 1 (tính đơn điệu tăng).
        let e1 = Expr::literal_num(1.0, p());
        let e2 = Expr::literal_num(2.0, p());
        let id1 = register_expr(e1);
        let id2 = register_expr(e2);
        assert_eq!(id2, id1 + 1);
    }

    #[test]
    fn test_expr_to_js_registry_emits_eval_call() {
        let e = Expr::Variable("n".to_string(), p());
        let js = expr_to_js_registry(&e);
        assert!(js.starts_with("__vb.evalExpr("));
        assert!(js.ends_with(")"));
    }

    #[test]
    fn test_take_expr_registry_drains_and_resets() {
        // Dọn sạch registry của thread này trước (phòng trường hợp test
        // khác trong cùng thread đã đăng ký gì đó trước — cargo test có
        // thể tái sử dụng thread cho nhiều test tuần tự).
        take_expr_registry();

        register_expr(Expr::literal_num(1.0, p()));
        register_expr(Expr::literal_num(2.0, p()));

        let drained = take_expr_registry();
        assert_eq!(drained.len(), 2);

        // Sau khi take, registry phải rỗng lại.
        let empty = take_expr_registry();
        assert_eq!(empty.len(), 0);
    }

    #[test]
    fn test_expr_to_js_unchanged_for_existing_behavior() {
        // Đảm bảo hàm expr_to_js gốc (JS thuần) không bị ảnh hưởng gì bởi
        // việc thêm registry — vẫn sinh JS y hệt như trước khi có registry.
        let e = Expr::Variable("dem".to_string(), p());
        assert_eq!(expr_to_js_default(&e), "__s.dem");
    }
}
