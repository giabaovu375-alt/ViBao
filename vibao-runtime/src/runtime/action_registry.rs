// ============================================================
// VIBAO RUNTIME (Rust/WASM) — runtime/action_registry.rs
// Đối xứng expr_registry.rs, nhưng cho `Action` thay vì `Expr`.
//
// Codegen serialize toàn bộ Vec<Action> (thân mỗi event handler, mỗi
// on_click/on_change/...) thành JSON, nhúng vào __vb.boot(). Lúc chạy,
// dom.rs (bind_events) chỉ nhúng 1 "action id" vào attribute
// data-vb-on-click="<actionId>" — khi bấm nút, tra registry theo id,
// lấy ra Vec<Action> thật, đưa cho action::dispatch() thực thi.
// ============================================================

use std::cell::RefCell;

use vibao_ast::Action;

thread_local! {
    static REGISTRY: RefCell<Vec<Vec<Action>>> = RefCell::new(Vec::new());
}

pub fn load_from_json(json: &str) {
    match serde_json::from_str::<Vec<Vec<Action>>>(json) {
        Ok(actions) => {
            REGISTRY.with(|reg| {
                *reg.borrow_mut() = actions;
            });
        }
        Err(err) => {
            crate::runtime::log::error(&format!(
                "[ViBao] Không parse được actionRegistry JSON: {}",
                err
            ));
        }
    }
}

/// Tra 1 chuỗi Action (thân của 1 event handler) theo id.
pub fn get(id: usize) -> Option<Vec<Action>> {
    REGISTRY.with(|reg| reg.borrow().get(id).cloned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use vibao_ast::Pos;

    fn p() -> Pos {
        Pos { line: 1, column: 1 }
    }

    #[test]
    fn test_load_and_get_roundtrip() {
        let actions = vec![Action::Assign {
            target: "dem".to_string(),
            value: vibao_ast::Expr::literal_num(1.0, p()),
            pos: p(),
        }];
        let json = serde_json::to_string(&vec![actions]).unwrap();
        load_from_json(&json);
        let fetched = get(0).expect("id 0 phải tồn tại");
        assert_eq!(fetched.len(), 1);
    }

    #[test]
    fn test_get_out_of_range_returns_none() {
        load_from_json("[]");
        assert!(get(999).is_none());
    }
}
