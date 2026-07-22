// ============================================================
// VIBAO RUNTIME (Rust/WASM) — runtime/log.rs
// Helper log console dùng chung toàn runtime.
//
// Lý do cần file này: gọi thẳng `web_sys::console::warn_1(...)` chỉ hoạt
// động khi build cho target `wasm32-unknown-unknown` (có JS glue thật
// phía sau). Khi chạy `cargo test` bình thường (target native của máy
// build), `web_sys::console` vẫn biên dịch được (nó chỉ là FFI binding)
// nhưng gọi vào lúc runtime sẽ panic vì không có JS engine nào phía sau
// để nhận lời gọi đó. Bọc qua 2 hàm dưới đây, chọn implementation đúng
// bằng `#[cfg(target_arch = "wasm32")]`, để `cargo test` (native) chạy
// được bình thường trong lúc phát triển, còn bản build thật (wasm) vẫn
// in ra console trình duyệt như bản JS gốc.
// ============================================================

#[cfg(target_arch = "wasm32")]
pub fn warn(msg: &str) {
    web_sys::console::warn_1(&msg.into());
}

#[cfg(not(target_arch = "wasm32"))]
pub fn warn(msg: &str) {
    eprintln!("[warn] {}", msg);
}

#[cfg(target_arch = "wasm32")]
pub fn error(msg: &str) {
    web_sys::console::error_1(&msg.into());
}

#[cfg(not(target_arch = "wasm32"))]
pub fn error(msg: &str) {
    eprintln!("[error] {}", msg);
}
