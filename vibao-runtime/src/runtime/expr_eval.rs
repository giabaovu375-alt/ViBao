// ============================================================
// VIBAO RUNTIME (Rust/WASM) — runtime/expr_eval.rs
// Đánh giá trực tiếp `vibao_ast::Expr` thành `VbValue`, chạy hoàn
// toàn bằng Rust — đây là bộ phận thay thế `new Function()`/`eval()`
// phía JS ở bản cũ. Không có bước "biên dịch ra JS rồi chạy JS":
// cây Expr được duyệt (tree-walking interpreter) trực tiếp mỗi lần
// cần giá trị.
//
// Được registry bên codegen (`codegen/expr.rs::expr_to_js_registry`)
// nạp cho: mỗi Expr "động" (Variable, Binary, Call, ...) được gán 1 id,
// gửi qua JSON trong `__vb.boot({ exprRegistry: [...] })`. Khi JS gọi
// `__vb.evalExpr(id)` (xem runtime/dom.rs, viết sau), hàm ở đây tra
// đúng Expr theo id và evaluate.
//
// ── TRACKED vs UNTRACKED ────────────────────────────────────────────
// Có 2 hàm public: `eval_tracked` (dùng bên trong 1 subscriber đang
// chạy — mọi Variable đọc được sẽ tự ghi dependency, để binding tự
// re-run khi state đổi) và `eval` (không track — dùng khi tính 1 lần,
// vd giá trị khởi tạo, hoặc bên trong 1 lời gọi hàm mà bản thân đối số
// không cần track riêng vì toàn bộ lời gọi cha đã track rồi).
// Trong thực tế, mọi expression eval từ binding (if/loop/style) LUÔN
// nên dùng `eval_tracked` — `eval` chỉ tồn tại cho các use case hiếm
// (vd action handler chạy 1 lần khi bấm nút, không cần reactive).
// ============================================================

use vibao_ast::{BinaryOp, ColorFuncKind, Expr, LiteralValue, TemplatePart, UnaryOp};

use super::state::{SharedState, State};
use super::value::VbValue;

// ════════════════════════════════════════════════════════════
// ENTRY POINTS
// ════════════════════════════════════════════════════════════

/// Đánh giá 1 Expr, CÓ TRACK dependency — dùng bên trong 1 subscriber
/// (binding if/loop/style/text động). `shared` phải đang chạy trong
/// ngữ cảnh 1 subscriber (current_tracking đã được set bởi
/// `state::run_subscriber`) để track có tác dụng; nếu không, hành vi
/// vẫn đúng (không track gì, giống hệt `eval` bên dưới), chỉ là mất
/// tính năng tự re-render.
pub fn eval_tracked(shared: &SharedState, expr: &Expr) -> VbValue {
    let mut state = shared.borrow_mut();
    eval_inner(&mut state, expr, true)
}

/// Đánh giá 1 Expr, KHÔNG track — dùng khi tính 1 lần (action handler,
/// giá trị khởi tạo lúc boot).
pub fn eval(shared: &SharedState, expr: &Expr) -> VbValue {
    let mut state = shared.borrow_mut();
    eval_inner(&mut state, expr, false)
}

/// Đánh giá 1 Expr, KHÔNG track, VỚI 1 LoopFrame được đóng gói sẵn —
/// dùng bởi action.rs khi action nằm bên trong 1 item của vong_lap (vd
/// "xoa($item)" trong on_click của button lồng trong loop). Tự push
/// frame lên loop-scope stack NGAY TRƯỚC khi eval, pop NGAY SAU — đúng
/// nguyên tắc đã áp dụng cho binding ở dom.rs::eval_expr_id_tracked
/// (mỗi lần gọi tự mang theo frame của mình, không phụ thuộc ai đã push
/// từ trước).
///
/// ĐÃ SỬA: trước đây action bên trong loop luôn gọi `eval()` trần
/// (không có frame nào), khiến "$item" resolve sai/rỗng — đây chính là
/// lỗ hổng "loop-action" đã note từ trước, giờ vá bằng hàm này.
pub fn eval_with_loop_frame(shared: &SharedState, expr: &Expr, loop_frame: Option<&super::state::LoopFrame>) -> VbValue {
    if let Some(frame) = loop_frame {
        shared.borrow_mut().push_loop_scope(frame.clone());
    }

    let mut state = shared.borrow_mut();
    let result = eval_inner(&mut state, expr, false);
    drop(state);

    if loop_frame.is_some() {
        shared.borrow_mut().pop_loop_scope();
    }

    result
}

// ════════════════════════════════════════════════════════════
// CORE TREE-WALKING EVALUATOR
// ════════════════════════════════════════════════════════════

/// `tracked` quyết định Variable/MemberAccess đọc qua đường có ghi
/// dependency (`scope_resolve_tracked`) hay không (`scope_resolve`).
/// Nhận `&mut State` (không phải `&SharedState`) vì đệ quy nội bộ
/// không cần re-borrow `RefCell` liên tục — tránh chi phí + tránh nguy
/// cơ "already borrowed" nếu lỡ gọi lồng `shared.borrow_mut()` 2 lần.
fn eval_inner(state: &mut State, expr: &Expr, tracked: bool) -> VbValue {
    match expr {
        Expr::Literal(lit, _) => eval_literal(lit),

        Expr::Variable(name, _) => {
            if tracked {
                state.scope_resolve_tracked(name)
            } else {
                state.scope_resolve(name)
            }
        }

        Expr::MemberAccess { object, property, .. } => {
            let obj = eval_inner(state, object, tracked);
            obj.get_field(property)
        }

        Expr::Binary { op, left, right, .. } => {
            let l = eval_inner(state, left, tracked);
            let r = eval_inner(state, right, tracked);
            eval_binary(*op, &l, &r)
        }

        Expr::Unary { op, operand, .. } => {
            let o = eval_inner(state, operand, tracked);
            match op {
                UnaryOp::Not => VbValue::Bool(!o.is_truthy()),
                UnaryOp::Neg => VbValue::Num(-o.to_num_or_zero()),
            }
        }

        Expr::Call { callee, args, .. } => {
            let arg_values: Vec<VbValue> =
                args.iter().map(|a| eval_inner(state, a, tracked)).collect();
            eval_call(callee, &arg_values)
        }

        Expr::ColorFunc { func, color, amount, .. } => {
            let color_val = eval_inner(state, color, tracked);
            eval_color_func(*func, &color_val, *amount)
        }

        Expr::Array(items, _) => {
            let values: Vec<VbValue> =
                items.iter().map(|e| eval_inner(state, e, tracked)).collect();
            VbValue::Array(values)
        }

        Expr::Object(fields, _) => {
            let entries: Vec<(String, VbValue)> = fields
                .iter()
                .map(|(k, v)| (k.clone(), eval_inner(state, v, tracked)))
                .collect();
            VbValue::object(entries)
        }

        Expr::TemplateString(parts, _) => eval_template(state, parts, tracked),
    }
}

// ════════════════════════════════════════════════════════════
// LITERAL
// ════════════════════════════════════════════════════════════

fn eval_literal(lit: &LiteralValue) -> VbValue {
    match lit {
        // Đơn vị CSS (nếu có) không có ý nghĩa trong 1 biểu thức JS/logic
        // thuần (vd $n - 1) — chỉ phần số được dùng. Đây là quyết định
        // NHẤT QUÁN với `codegen/expr.rs::literal_to_js`, nơi cũng bỏ đơn
        // vị khi sinh JS cho ngữ cảnh biểu thức số học thuần.
        LiteralValue::Num(n, _unit) => VbValue::Num(*n),
        LiteralValue::Str(s) => VbValue::Str(s.clone()),
        LiteralValue::Bool(b) => VbValue::Bool(*b),
        LiteralValue::Color(c) => VbValue::Str(c.clone()),
    }
}

// ════════════════════════════════════════════════════════════
// BINARY OP
// ════════════════════════════════════════════════════════════

fn eval_binary(op: BinaryOp, l: &VbValue, r: &VbValue) -> VbValue {
    match op {
        BinaryOp::Add => eval_add(l, r),
        BinaryOp::Sub => VbValue::Num(l.to_num_or_zero() - r.to_num_or_zero()),
        BinaryOp::Mul => VbValue::Num(l.to_num_or_zero() * r.to_num_or_zero()),
        BinaryOp::Div => VbValue::Num(l.to_num_or_zero() / r.to_num_or_zero()),
        BinaryOp::Mod => VbValue::Num(l.to_num_or_zero() % r.to_num_or_zero()),
        // Khớp hành vi codegen JS (luôn sinh "===" / "!==") — so sánh
        // NGHIÊM NGẶT theo kiểu + giá trị, không coerce kiểu ngầm định.
        BinaryOp::Eq => VbValue::Bool(l.strict_eq(r)),
        BinaryOp::Neq => VbValue::Bool(!l.strict_eq(r)),
        BinaryOp::Gt => VbValue::Bool(l.partial_cmp_loose(r) == std::cmp::Ordering::Greater),
        BinaryOp::Gte => VbValue::Bool(l.partial_cmp_loose(r) != std::cmp::Ordering::Less),
        BinaryOp::Lt => VbValue::Bool(l.partial_cmp_loose(r) == std::cmp::Ordering::Less),
        BinaryOp::Lte => VbValue::Bool(l.partial_cmp_loose(r) != std::cmp::Ordering::Greater),
        BinaryOp::And => VbValue::Bool(l.is_truthy() && r.is_truthy()),
        BinaryOp::Or => VbValue::Bool(l.is_truthy() || r.is_truthy()),
    }
}

/// "+" có 2 nghĩa tuỳ kiểu toán hạng, giống hệt JS: nếu 1 trong 2 vế là
/// chuỗi, "+" nối chuỗi (String(l) + String(r)); ngược lại cộng số học.
/// Đây LÀ hành vi JS thật (không phải lựa chọn tuỳ ý của ViBao) — giữ
/// nguyên để khớp kết quả với bản codegen JS cũ (nơi "+" dịch thẳng
/// sang toán tử "+" của JS, thừa hưởng semantics này).
fn eval_add(l: &VbValue, r: &VbValue) -> VbValue {
    match (l, r) {
        (VbValue::Num(a), VbValue::Num(b)) => VbValue::Num(a + b),
        _ => VbValue::Str(format!("{}{}", l, r)),
    }
}

// ════════════════════════════════════════════════════════════
// FUNCTION CALLS (__fmt.*, lam_tron, và fallback cho hàm không rõ)
// ════════════════════════════════════════════════════════════

/// Ánh xạ tên hàm tiện ích ViBao sang hành vi Rust tương ứng — tương
/// đương `__fmt.*` phía JS cũ VÀ `map_call_fn` phía codegen (nhưng ở
/// đây THỰC THI kết quả thay vì chỉ đổi tên để sinh JS).
///
/// Hàm không nằm trong bảng bên dưới (custom action, gọi component...)
/// trả về `VbValue::Null` — evaluator KHÔNG PHẢI nơi thực thi side-effect
/// (mở modal, gọi API...), những việc đó thuộc `runtime/dom.rs`/`api.rs`
/// (action, không phải expression). Nếu 1 Expr::Call đi lạc vào đây với
/// callee thuộc nhóm side-effect, đó là lỗi codegen đã đăng ký nhầm expr
/// hành động vào registry biểu thức — không phải điều evaluator tự sửa
/// được, nên trả Null thay vì đoán mò là lựa chọn an toàn nhất.
fn eval_call(callee: &str, args: &[VbValue]) -> VbValue {
    match callee {
        "lam_tron" => VbValue::Num(args.first().map(|v| v.to_num_or_zero()).unwrap_or(0.0).round()),
        "phan_tram" => {
            let n = args.first().map(|v| v.to_num_or_zero()).unwrap_or(0.0);
            VbValue::Str(format!("{}%", format_number_trim(n)))
        }
        "hoa_chu" => {
            let s = args.first().and_then(|v| v.as_str()).unwrap_or("").to_string();
            VbValue::Str(s.to_uppercase())
        }
        "rut_gon" => {
            let s = args.first().and_then(|v| v.as_str()).unwrap_or("");
            let max_len = args.get(1).map(|v| v.to_num_or_zero() as usize).unwrap_or(20);
            if s.chars().count() > max_len {
                let truncated: String = s.chars().take(max_len).collect();
                VbValue::Str(format!("{}...", truncated))
            } else {
                VbValue::Str(s.to_string())
            }
        }
        "gia_tien" => {
            // Định dạng tiền VNĐ đơn giản: nhóm hàng nghìn bằng dấu chấm,
            // hậu tố "đ" — tương đương __fmt.giaTien ở bản JS cũ.
            let n = args.first().map(|v| v.to_num_or_zero()).unwrap_or(0.0);
            VbValue::Str(format!("{}đ", group_thousands(n as i64)))
        }
        "ngay" => {
            // Định dạng ngày: bản tối giản, chỉ format chuỗi ISO có sẵn
            // thành dd/mm/yyyy nếu input đã là chuỗi ISO "yyyy-mm-dd...".
            // KHÔNG có Date/timezone logic đầy đủ ở đây — evaluator chạy
            // trong WASM không có quyền truy cập Date của JS trực tiếp
            // (cần qua js-sys nếu muốn "bây giờ"); với 1 chuỗi ngày có
            // sẵn thì parse thủ công là đủ cho phần lớn use case hiển thị.
            let s = args.first().and_then(|v| v.as_str()).unwrap_or("");
            format_iso_date(s)
        }
        _ => VbValue::Null,
    }
}

fn format_number_trim(n: f64) -> String {
    if n.fract() == 0.0 && n.is_finite() {
        format!("{}", n as i64)
    } else {
        n.to_string()
    }
}

/// Nhóm chữ số hàng nghìn bằng dấu chấm — vd 1234567 -> "1.234.567".
fn group_thousands(n: i64) -> String {
    let neg = n < 0;
    let s = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            out.push('.');
        }
        out.push(ch);
    }
    let grouped: String = out.chars().rev().collect();
    if neg {
        format!("-{}", grouped)
    } else {
        grouped
    }
}

/// Parse chuỗi ISO "yyyy-mm-dd" (hoặc có thêm "Thh:mm:ss...") thành
/// "dd/mm/yyyy". Trả về nguyên chuỗi gốc nếu không khớp định dạng.
fn format_iso_date(s: &str) -> VbValue {
    let date_part = s.split('T').next().unwrap_or(s);
    let parts: Vec<&str> = date_part.split('-').collect();
    if parts.len() == 3 {
        VbValue::Str(format!("{}/{}/{}", parts[2], parts[1], parts[0]))
    } else {
        VbValue::Str(s.to_string())
    }
}

// ════════════════════════════════════════════════════════════
// COLOR FUNCTIONS (trong_suot, lam_sang, lam_toi)
// ════════════════════════════════════════════════════════════

/// Áp dụng hàm màu lên 1 mã hex "#RRGGBB", trả về mã hex/rgba mới.
/// `amount` trong khoảng 0-100. Nếu `color` không phải chuỗi hex hợp lệ,
/// trả về nguyên giá trị gốc (an toàn, không panic).
fn eval_color_func(func: ColorFuncKind, color: &VbValue, amount: f64) -> VbValue {
    let hex = match color.as_str() {
        Some(s) => s,
        None => return color.clone(),
    };
    let (r, g, b) = match parse_hex_rgb(hex) {
        Some(rgb) => rgb,
        None => return color.clone(),
    };
    let amount = amount.clamp(0.0, 100.0);

    match func {
        ColorFuncKind::TrongSuot => {
            // Độ trong suốt: trả về rgba() với alpha = 1 - amount/100.
            let alpha = 1.0 - amount / 100.0;
            VbValue::Str(format!("rgba({}, {}, {}, {})", r, g, b, alpha))
        }
        ColorFuncKind::LamSang => {
            let f = amount / 100.0;
            let nr = lerp_to(r, 255, f);
            let ng = lerp_to(g, 255, f);
            let nb = lerp_to(b, 255, f);
            VbValue::Str(format_hex(nr, ng, nb))
        }
        ColorFuncKind::LamToi => {
            let f = amount / 100.0;
            let nr = lerp_to(r, 0, f);
            let ng = lerp_to(g, 0, f);
            let nb = lerp_to(b, 0, f);
            VbValue::Str(format_hex(nr, ng, nb))
        }
    }
}

fn parse_hex_rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let h = hex.strip_prefix('#').unwrap_or(hex);
    if h.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&h[0..2], 16).ok()?;
    let g = u8::from_str_radix(&h[2..4], 16).ok()?;
    let b = u8::from_str_radix(&h[4..6], 16).ok()?;
    Some((r, g, b))
}

fn lerp_to(from: u8, to: u8, f: f64) -> u8 {
    let from = from as f64;
    let to = to as f64;
    (from + (to - from) * f).round().clamp(0.0, 255.0) as u8
}

fn format_hex(r: u8, g: u8, b: u8) -> String {
    format!("#{:02X}{:02X}{:02X}", r, g, b)
}

// ════════════════════════════════════════════════════════════
// TEMPLATE STRING
// ════════════════════════════════════════════════════════════

fn eval_template(state: &mut State, parts: &[TemplatePart], tracked: bool) -> VbValue {
    let mut out = String::new();
    for part in parts {
        match part {
            TemplatePart::Text(text) => out.push_str(text),
            TemplatePart::Variable(name) => {
                let v = if tracked {
                    state.scope_resolve_tracked(name)
                } else {
                    state.scope_resolve(name)
                };
                out.push_str(&v.to_string());
            }
            TemplatePart::Member(path) => {
                let full_path = path.join(".");
                let v = if tracked {
                    // get_path không có biến thể tracked riêng — dig từng
                    // phần thủ công ở đây để track đúng phần "root".
                    let mut parts_iter = path.iter();
                    let root_name = match parts_iter.next() {
                        Some(p) => p,
                        // path rỗng (không nên xảy ra — parser luôn tạo
                        // Member với ít nhất 1 phần tử) — bỏ qua an toàn,
                        // không có gì để nối vào output cho phần tử này.
                        None => continue,
                    };
                    let mut cur = state.scope_resolve_tracked(root_name);
                    for p in parts_iter {
                        if cur.is_null() {
                            break;
                        }
                        cur = cur.get_field(p);
                    }
                    cur
                } else {
                    state.get_path(&full_path)
                };
                out.push_str(&v.to_string());
            }
        }
    }
    VbValue::Str(out)
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

    fn setup() -> SharedState {
        super::super::state::new_shared_state()
    }

    #[test]
    fn test_eval_literal_number() {
        let shared = setup();
        let e = Expr::literal_num(42.0, p());
        assert_eq!(eval(&shared, &e).as_num(), Some(42.0));
    }

    #[test]
    fn test_eval_variable_reads_global_state() {
        let shared = setup();
        shared.borrow_mut().set_state("dem", VbValue::num(5.0));
        let e = Expr::Variable("dem".to_string(), p());
        assert_eq!(eval(&shared, &e).as_num(), Some(5.0));
    }

    #[test]
    fn test_eval_binary_add_numbers() {
        let shared = setup();
        let e = Expr::Binary {
            op: BinaryOp::Add,
            left: Box::new(Expr::literal_num(1.0, p())),
            right: Box::new(Expr::literal_num(2.0, p())),
            pos: p(),
        };
        assert_eq!(eval(&shared, &e).as_num(), Some(3.0));
    }

    #[test]
    fn test_eval_binary_add_string_concat() {
        let shared = setup();
        let e = Expr::Binary {
            op: BinaryOp::Add,
            left: Box::new(Expr::literal_str("a", p())),
            right: Box::new(Expr::literal_str("b", p())),
            pos: p(),
        };
        assert_eq!(eval(&shared, &e).as_str(), Some("ab"));
    }

    #[test]
    fn test_eval_binary_eq_strict() {
        let shared = setup();
        let e = Expr::Binary {
            op: BinaryOp::Eq,
            left: Box::new(Expr::literal_num(1.0, p())),
            right: Box::new(Expr::literal_num(1.0, p())),
            pos: p(),
        };
        assert_eq!(eval(&shared, &e), VbValue::Bool(true));
    }

    #[test]
    fn test_eval_unary_not() {
        let shared = setup();
        let e = Expr::Unary {
            op: UnaryOp::Not,
            operand: Box::new(Expr::literal_bool(false, p())),
            pos: p(),
        };
        assert_eq!(eval(&shared, &e), VbValue::Bool(true));
    }

    #[test]
    fn test_eval_member_access_rong_do_dai() {
        let shared = setup();
        shared.borrow_mut().set_state(
            "ds",
            VbValue::Array(vec![VbValue::num(1.0), VbValue::num(2.0)]),
        );
        let base = Expr::Variable("ds".to_string(), p());
        let e = Expr::MemberAccess {
            object: Box::new(base),
            property: "do_dai".to_string(),
            pos: p(),
        };
        assert_eq!(eval(&shared, &e).as_num(), Some(2.0));
    }

    #[test]
    fn test_eval_call_lam_tron() {
        let shared = setup();
        let e = Expr::Call {
            callee: "lam_tron".to_string(),
            args: vec![Expr::literal_num(3.7, p())],
            pos: p(),
        };
        assert_eq!(eval(&shared, &e).as_num(), Some(4.0));
    }

    #[test]
    fn test_eval_call_gia_tien_groups_thousands() {
        let shared = setup();
        let e = Expr::Call {
            callee: "gia_tien".to_string(),
            args: vec![Expr::literal_num(1234567.0, p())],
            pos: p(),
        };
        assert_eq!(eval(&shared, &e).as_str(), Some("1.234.567đ"));
    }

    #[test]
    fn test_eval_color_func_lam_sang() {
        let shared = setup();
        let e = Expr::ColorFunc {
            func: ColorFuncKind::LamSang,
            color: Box::new(Expr::Literal(LiteralValue::Color("#000000".to_string()), p())),
            amount: 100.0,
            pos: p(),
        };
        // Sáng 100% từ đen -> trắng hoàn toàn.
        assert_eq!(eval(&shared, &e).as_str(), Some("#FFFFFF"));
    }

    #[test]
    fn test_eval_color_func_trong_suot() {
        let shared = setup();
        let e = Expr::ColorFunc {
            func: ColorFuncKind::TrongSuot,
            color: Box::new(Expr::Literal(LiteralValue::Color("#FF0000".to_string()), p())),
            amount: 50.0,
            pos: p(),
        };
        assert_eq!(eval(&shared, &e).as_str(), Some("rgba(255, 0, 0, 0.5)"));
    }

    #[test]
    fn test_eval_template_string() {
        let shared = setup();
        shared.borrow_mut().set_state("ten", VbValue::str("An"));
        let parts = vec![
            TemplatePart::Text("Xin chào ".to_string()),
            TemplatePart::Variable("ten".to_string()),
        ];
        let e = Expr::TemplateString(parts, p());
        assert_eq!(eval(&shared, &e).as_str(), Some("Xin chào An"));
    }

    #[test]
    fn test_eval_tracked_registers_dependency() {
        use super::super::state::{flush, subscribe};

        let shared = setup();
        shared.borrow_mut().set_state("n", VbValue::num(1.0));

        let seen = std::rc::Rc::new(std::cell::RefCell::new(0.0));
        let seen_clone = seen.clone();
        let e = Expr::Variable("n".to_string(), p());

        subscribe(
            &shared,
            Box::new(move |sh: &SharedState| {
                let v = eval_tracked(sh, &e);
                *seen_clone.borrow_mut() = v.as_num().unwrap_or(0.0);
            }),
        );
        assert_eq!(*seen.borrow(), 1.0);

        shared.borrow_mut().set_state("n", VbValue::num(2.0));
        flush(&shared);
        assert_eq!(*seen.borrow(), 2.0); // tự re-run vì evalTracked đã track "n"
    }

    #[test]
    fn test_eval_loop_scope_priority_over_global() {
        use super::super::state::LoopFrame;

        let shared = setup();
        shared.borrow_mut().set_state("item", VbValue::str("global"));
        shared.borrow_mut().push_loop_scope(LoopFrame {
            item_var: "item".to_string(),
            item_value: VbValue::str("local"),
            index_var: None,
            index_value: None,
        });

        let e = Expr::Variable("item".to_string(), p());
        assert_eq!(eval_tracked(&shared, &e).as_str(), Some("local"));
    }
}
