// ============================================================
// VIBAO RUNTIME (Rust/WASM) — lib.rs
// Crate root. Biên dịch sang wasm32-unknown-unknown, thay thế
// 17/18/19/20/21-runtime-*.ts bản cũ bằng engine Rust thuần
// (ngoại trừ các lệnh gọi DOM API bắt buộc phải qua web-sys/wasm-bindgen
// vì WASM không có quyền truy cập DOM trực tiếp).
// ============================================================

pub mod runtime;

pub use runtime::dom::VbRuntime;
pub use runtime::expr_eval::{eval, eval_tracked};
pub use runtime::state::{LoopFrame, SharedState, State, SubId};
pub use runtime::value::VbValue;
