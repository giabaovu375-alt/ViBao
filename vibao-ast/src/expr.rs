// ============================================================
// VIBAO COMPILER (Rust) — ast/expr.rs
// EXPRESSIONS: Expr, LiteralValue, toán tử, và các hàm tiện ích
// tạo node nhanh (dùng ở parser)
//
// Mọi type ở đây đều derive Serialize/Deserialize (serde) — LÝ DO:
// crate này giờ được DÙNG CHUNG giữa `vibaoc` (compiler, sinh code) và
// `vibao-runtime` (WASM, chạy trong trình duyệt). Codegen serialize 1
// Expr thành JSON lúc build, nhúng vào JS output; lúc trang load, WASM
// deserialize lại đúng JSON đó thành Expr thật để evaluator Rust chạy
// trực tiếp — không cần eval chuỗi JS. Đây là cầu nối duy nhất giữa 2
// crate (chúng không gọi hàm lẫn nhau, chỉ trao đổi qua dữ liệu JSON).
// ============================================================

use serde::{Deserialize, Serialize};

use super::Pos;

// ════════════════════════════════════════════════════════════
// 11. EXPRESSIONS
// ════════════════════════════════════════════════════════════

/// Expr bọc Box<Expr> ở các biến thể đệ quy (Binary/Unary/MemberAccess)
/// vì cùng lý do với Child ở trên — kích thước enum phải cố định tại
/// compile-time, đệ quy trực tiếp (không qua con trỏ) là vô hạn kích
/// thước nên Rust bắt buộc phải Box hoá.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expr {
    Literal(LiteralValue, Pos),
    Variable(String, Pos), // không có $
    MemberAccess {
        object: Box<Expr>,
        property: String,
        pos: Pos,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
        pos: Pos,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
        pos: Pos,
    },
    Call {
        callee: String,
        args: Vec<Expr>,
        pos: Pos,
    },
    ColorFunc {
        func: ColorFuncKind,
        color: Box<Expr>,
        amount: f64, // 0-100
        pos: Pos,
    },
    Array(Vec<Expr>, Pos),
    Object(Vec<(String, Expr)>, Pos),
    /// Chuỗi có nội suy biến: "Xin chào $ten" — tách thành các phần.
    TemplateString(Vec<TemplatePart>, Pos),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LiteralValue {
    Str(String),
    /// (giá trị số, đơn vị CSS gốc nếu có — vd "50%" → Num(50.0, Some("%".into())),
    /// "16" → Num(16.0, None)). Giữ lại đơn vị từ lexer (NumberLit đã có raw
    /// string) để codegen phân biệt đúng "50%" khác "50" (mặc định px) —
    /// bug này từng có ở bản Rust ban đầu khi parser bỏ raw đi (_raw).
    Num(f64, Option<String>),
    Bool(bool),
    Color(String), // đã resolve ra hex hoặc giữ nguyên tên biến CSS
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TemplatePart {
    Text(String),
    Variable(String),
    Member(Vec<String>), // đường dẫn: $obj.field.sub → ["obj","field","sub"]
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Neq,
    Gt,
    Gte,
    Lt,
    Lte,
    And,
    Or,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum UnaryOp {
    Not,
    Neg,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ColorFuncKind {
    TrongSuot,
    LamSang,
    LamToi,
}

// ════════════════════════════════════════════════════════════
// 15. HELPER CONSTRUCTORS — tiện ích tạo node nhanh (dùng ở parser)
// ════════════════════════════════════════════════════════════

impl Expr {
    pub fn literal_str(value: impl Into<String>, pos: Pos) -> Self {
        Expr::Literal(LiteralValue::Str(value.into()), pos)
    }

    /// Tạo literal số không có đơn vị CSS (vd trong biểu thức số học
    /// thuần: $n - 1). Dùng literal_num_with_unit() khi cần giữ đơn vị.
    pub fn literal_num(value: f64, pos: Pos) -> Self {
        Expr::Literal(LiteralValue::Num(value, None), pos)
    }

    /// Tạo literal số kèm đơn vị CSS gốc (vd "50%", "10px") — dùng bởi
    /// parser khi token NumberLit có phần raw chứa hậu tố đơn vị.
    pub fn literal_num_with_unit(value: f64, unit: Option<String>, pos: Pos) -> Self {
        Expr::Literal(LiteralValue::Num(value, unit), pos)
    }

    pub fn literal_bool(value: bool, pos: Pos) -> Self {
        Expr::Literal(LiteralValue::Bool(value), pos)
    }

    pub fn pos(&self) -> Pos {
        match self {
            Expr::Literal(_, p) => *p,
            Expr::Variable(_, p) => *p,
            Expr::MemberAccess { pos, .. } => *pos,
            Expr::Binary { pos, .. } => *pos,
            Expr::Unary { pos, .. } => *pos,
            Expr::Call { pos, .. } => *pos,
            Expr::ColorFunc { pos, .. } => *pos,
            Expr::Array(_, p) => *p,
            Expr::Object(_, p) => *p,
            Expr::TemplateString(_, p) => *p,
        }
    }
}
