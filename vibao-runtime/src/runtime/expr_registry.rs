// ============================================================
// VIBAO RUNTIME (Rust/WASM) — runtime/expr_registry.rs
// Phía NHẬN của expr registry — đối xứng với phía GỬI đã có ở
// `vibaoc/src/codegen/expr.rs::take_expr_registry()`.
//
// Lúc build, codegen serialize toàn bộ Expr "động" (mọi nơi gọi
// expr_to_js_registry) thành JSON, nhúng vào `__vb.boot({ exprRegistry:
// [...] })`. Lúc trang load, JS gọi `__vb.boot(optsJson)` → hàm ở đây
// deserialize JSON đó thành Vec<Expr> thật, lưu vào 1 thread_local để
// `evalExpr(id)` (gọi từ JS mỗi khi 1 binding cần tính giá trị) tra
// đúng Expr theo index.
//
// Dùng RefCell<Vec<Expr>> (không phải Mutex) vì WASM chạy đơn luồng
// trong 1 tab trình duyệt — không cần đồng bộ đa luồng.
// ============================================================

use std::cell::RefCell;

use vibao_ast::Expr;

thread_local! {
    static REGISTRY: RefCell<Vec<Expr>> = RefCell::new(Vec::new());
}

/// Nạp registry từ JSON — gọi 1 lần lúc `__vb.boot(...)`. Nếu JSON lỗi
/// (không nên xảy ra trừ khi codegen/runtime lệch phiên bản với nhau),
/// log lỗi ra console và để registry rỗng thay vì panic — 1 lỗi ở expr
/// registry không nên làm sập toàn bộ app, chỉ khiến các binding "động"
/// trả về Null (an toàn hơn crash trắng trang).
pub fn load_from_json(json: &str) {
    match serde_json::from_str::<Vec<Expr>>(json) {
        Ok(exprs) => {
            REGISTRY.with(|reg| {
                *reg.borrow_mut() = exprs;
            });
        }
        Err(err) => {
            crate::runtime::log::error(&format!(
                "[ViBao] Không parse được exprRegistry JSON: {}",
                err
            ));
        }
    }
}

/// Tra 1 Expr theo id. Trả về `None` nếu id không hợp lệ (out of range)
/// — caller (evalExpr trong dom.rs) nên coi đây là VbValue::Null, không
/// panic, vì 1 id sai (do bug codegen hoặc registry chưa load) không nên
/// làm crash cả trang.
pub fn get(id: usize) -> Option<Expr> {
    REGISTRY.with(|reg| reg.borrow().get(id).cloned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vibao_ast::Pos;

    #[test]
    fn test_load_and_get_roundtrip() {
        let e = Expr::literal_num(42.0, Pos { line: 1, column: 1 });
        let json = serde_json::to_string(&vec![e]).unwrap();
        load_from_json(&json);
        let fetched = get(0).expect("id 0 phải tồn tại sau khi load");
        match fetched {
            Expr::Literal(vibao_ast::LiteralValue::Num(n, _), _) => assert_eq!(n, 42.0),
            _ => panic!("Sai loại Expr sau roundtrip"),
        }
    }

    #[test]
    fn test_get_out_of_range_returns_none() {
        load_from_json("[]");
        assert!(get(999).is_none());
    }

    #[test]
    fn test_load_invalid_json_does_not_panic() {
        // Không được panic — chỉ log lỗi và giữ registry như cũ (rỗng).
        load_from_json("{ invalid json");
        assert!(get(0).is_none());
    }
}
