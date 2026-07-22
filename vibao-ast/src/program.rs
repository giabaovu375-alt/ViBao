// ============================================================
// VIBAO COMPILER (Rust) — ast/program.rs
// CHƯƠNG TRÌNH GỐC & TRANG: Program, App, Page, PageEvent
// ============================================================

use super::child::Child;
use super::decl::{ComponentDef, StateDecl, Theme, VarDecl};
use super::event::Action;
use super::style::ColorValue;
use super::Pos;

// ════════════════════════════════════════════════════════════
// 2. CHƯƠNG TRÌNH GỐC
// ════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct Program {
    pub app: App,
}

#[derive(Debug, Clone)]
pub struct App {
    pub name: String,
    pub variables: Vec<VarDecl>,
    pub themes: Vec<Theme>,
    pub components: Vec<ComponentDef>, // @the
    pub pages: Vec<Page>,
    pub pos: Pos,
}

// ════════════════════════════════════════════════════════════
// 3. TRANG
// ════════════════════════════════════════════════════════════

#[derive(Debug, Clone)]
pub struct Page {
    pub route: String,
    pub name: Option<String>,
    pub mau_nen: Option<ColorValue>,
    pub states: Vec<StateDecl>,
    pub events: Vec<PageEvent>,
    pub children: Vec<Child>,
    pub pos: Pos,
}

#[derive(Debug, Clone)]
pub struct PageEvent {
    pub name: PageEventName,
    pub body: Vec<Action>,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PageEventName {
    OnTai,
    OnHuy,
}
