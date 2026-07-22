// ============================================================
// VIBAO COMPILER (Rust) — ast/child.rs
// CHILD NODE (mọi thứ bên trong trang/box/vv.) & ELEMENT
// ============================================================

use super::control_flow::{IfNode, LoopNode, SwitchNode};
use super::decl::{StateDecl, VarDecl};
use super::event::EventNode;
use super::expr::Expr;
use super::program::PageEvent;
use super::style::{AnimationProps, ResponsiveNode};
use super::Pos;

// ════════════════════════════════════════════════════════════
// 6. CHILD NODE — mọi thứ bên trong trang/box/vv.
// ════════════════════════════════════════════════════════════

/// Tương đương "ChildNode" union ở bản TS. Rust enum bọc từng biến thể
/// trong Box<T> vì Child xuất hiện đệ quy bên trong chính các struct nó
/// chứa (Element có Vec<Child>, nếu không Box thì kích thước Child sẽ
/// phụ thuộc đệ quy vào chính nó — compiler Rust không cho phép kiểu vô
/// hạn kích thước như vậy, đây là khác biệt căn bản với TS (nơi mọi thứ
/// đều là reference ngầm định, không cần khai báo Box tường minh).
#[derive(Debug, Clone)]
pub enum Child {
    Element(Element),
    ComponentCall(ComponentCall),
    If(Box<IfNode>),
    Switch(Box<SwitchNode>),
    Loop(Box<LoopNode>),
    StateDecl(StateDecl),
    VarDecl(VarDecl),
    PageEvent(PageEvent),
}

// ════════════════════════════════════════════════════════════
// 7. ELEMENT — component cụ thể (text, box, flex, button, ...)
// ════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct Element {
    pub tag: String,
    pub props: PropsMap,
    pub children: Vec<Child>,
    pub events: Vec<EventNode>,
    pub responsive: Vec<ResponsiveNode>,
    pub animation: AnimationProps,
    pub pos: Pos,
}

#[derive(Debug, Clone)]
pub struct ComponentCall {
    pub name: String,
    pub props: PropsMap,
    pub children: Vec<Child>,
    pub pos: Pos,
}

/// Props map: key -> Expr. Dùng Vec<(String, Expr)> thay vì HashMap để
/// giữ đúng THỨ TỰ khai báo props như trong source — quan trọng cho
/// codegen khi cần tái tạo lại thứ tự CSS property hoặc debug output dễ
/// đọc hơn. HashMap trong Rust không đảm bảo thứ tự lặp qua.
pub type PropsMap = Vec<(String, Expr)>;

/// Tiện ích tra cứu 1 prop theo tên trong PropsMap (thay thế cho việc
/// HashMap tự có, phải viết tay vì đổi sang Vec để giữ thứ tự).
pub fn get_prop<'a>(props: &'a PropsMap, key: &str) -> Option<&'a Expr> {
    props.iter().find(|(k, _)| k == key).map(|(_, v)| v)
}
