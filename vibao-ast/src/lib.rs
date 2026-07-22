// ============================================================
// VIBAO — crate `vibao-ast`
// Định nghĩa AST node types, DÙNG CHUNG giữa 2 crate trong workspace:
//   - `vibaoc`         (compiler: lexer → parser → codegen)
//   - `vibao-runtime`  (WASM: chạy trong trình duyệt, evaluate Expr
//                       trực tiếp bằng Rust thay vì eval chuỗi JS)
//
// Trước đây (`ast.rs`/`ast/mod.rs`) file này chỉ thuộc về `vibaoc`. Giờ
// tách thành crate riêng để `vibao-runtime` có thể `use vibao_ast::Expr`
// mà không phải định nghĩa lại (tránh 2 bản Expr lệch nhau theo thời
// gian). Mọi type ở đây derive Serialize/Deserialize (serde) vì đây
// chính là "hợp đồng dữ liệu" giữa 2 crate: codegen serialize 1 Expr
// thành JSON lúc build, runtime deserialize lại đúng JSON đó lúc chạy.
//
// File này được chia nhỏ từ 1 file ast.rs duy nhất thành nhiều
// module con theo nhóm khái niệm, để dễ đọc / dễ maintain hơn:
//   program.rs      — Program, App, Page, PageEvent
//   decl.rs         — VarDecl, StateDecl, Theme, ComponentDef, ParamDef, DataType
//   child.rs        — Child, Element, ComponentCall, PropsMap, get_prop
//   control_flow.rs — IfNode, SwitchNode, CaseNode, LoopNode, LoopKind
//   event.rs        — EventNode, EventName, Action
//   expr.rs         — Expr, LiteralValue, TemplatePart, toán tử, helper constructors
//   style.rs        — ColorValue, AnimationProps, LapValue, Breakpoint, ResponsiveNode
//   tests.rs        — unit tests (chỉ build khi cfg(test))
//
// Mọi type ở đây được re-export thẳng từ `vibao_ast::`, nên các chỗ gọi
// `ast::Page`, `ast::Expr`, `ast::get_prop(...)` ... ở parser.rs /
// codegen.rs của `vibaoc` chỉ cần đổi `use crate::ast::` thành
// `use vibao_ast::` — không cần sửa cách dùng gì khác.
// ============================================================

use serde::{Deserialize, Serialize};

pub mod child;
pub mod control_flow;
pub mod decl;
pub mod event;
pub mod expr;
pub mod program;
pub mod style;

#[cfg(test)]
mod tests;

pub use child::{get_prop, Child, ComponentCall, Element, PropsMap};
pub use control_flow::{CaseNode, IfNode, LoopKind, LoopNode, SwitchNode};
pub use decl::{ComponentDef, DataType, ParamDef, StateDecl, Theme, VarDecl};
pub use event::{Action, EventName, EventNode};
pub use expr::{BinaryOp, ColorFuncKind, Expr, LiteralValue, TemplatePart, UnaryOp};
pub use program::{App, Page, PageEvent, PageEventName, Program};
pub use style::{AnimationProps, Breakpoint, ColorValue, LapValue, ResponsiveNode};

// ════════════════════════════════════════════════════════════
// 1. VỊ TRÍ TRONG SOURCE (dùng để báo lỗi)
// ════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Pos {
    pub line: usize,
    pub column: usize,
}
