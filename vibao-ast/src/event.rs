// ============================================================
// VIBAO COMPILER (Rust) — ast/event.rs
// EVENTS & ACTIONS (bên trong sự kiện)
//
// Action derive Serialize/Deserialize giống Expr — LÝ DO: theo đúng
// kiến trúc "Rust thuần, không eval JS" đã chọn cho biểu thức, hành
// động (thong_bao/goi_api/gan_bien...) khi bấm nút cũng cần được THỰC
// THI bởi Rust/WASM, không phải sinh JS rồi eval. Codegen serialize
// Vec<Action> ra JSON qua 1 "action registry" (đối xứng expr registry),
// runtime deserialize lại và tự dispatch (xem vibao-runtime::action).
// ============================================================

use serde::{Deserialize, Serialize};

use super::child::PropsMap;
use super::expr::Expr;
use super::Pos;

// ════════════════════════════════════════════════════════════
// 9. EVENTS
// ════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventNode {
    pub name: EventName,
    pub body: Vec<Action>,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EventName {
    OnClick,
    OnHover,
    OnBlur,
    OnFocus,
    OnChange,
    OnSubmit,
    OnScroll,
}

// ════════════════════════════════════════════════════════════
// 10. ACTIONS (bên trong sự kiện)
// ════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Action {
    FunctionCall {
        name: String,
        args: Vec<Expr>,
        opts: PropsMap, // named options: thong_bao(msg, kieu:thanh_cong)
        assign_to: Option<String>,
        pos: Pos,
    },
    Assign {
        target: String, // tên biến, không có $
        value: Expr,
        pos: Pos,
    },
    ApiCall {
        method: String,
        endpoint: Expr,
        data: Option<Expr>,
        assign_to: Option<String>,
        on_success: Option<Vec<Action>>,
        on_failure: Option<Vec<Action>>,
        pos: Pos,
    },
    IfAction {
        condition: Expr,
        consequent: Vec<Action>,
        alternate: Option<Vec<Action>>,
        pos: Pos,
    },
}
