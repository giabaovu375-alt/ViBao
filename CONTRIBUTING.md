# Đóng góp cho ViBao

Guide này dành cho ai muốn sửa/xây thêm chính compiler (`vibaoc`)
hoặc runtime (`vibao-runtime`). Nếu chỉ muốn *dùng* ViBao để viết
app, xem [`README.md`](README.md) — không cần đọc file này.

## Yêu cầu môi trường

- [Rust](https://rustup.rs/) (bản ổn định, `cargo` đi kèm)
- Target WASM: `rustup target add wasm32-unknown-unknown`
- [`wasm-pack`](https://rustwasm.github.io/wasm-pack/installer/) —
  để build `vibao-runtime` ra WASM

## Build & test

```bash
git clone https://github.com/giabaovu375-alt/ViBao.git
cd ViBao

# Chạy toàn bộ test (vibao-ast + vibaoc + vibao-runtime)
cargo test

# Build compiler (native binary)
cargo build --release -p vibaoc

# Build runtime ra WASM
cd vibao-runtime
wasm-pack build --release --target web --out-dir pkg
cd ..
```

Sau khi build cả 2, để `vibaoc build` tìm thấy runtime WASM, đặt
biến môi trường trỏ tới nó (hoặc copy `pkg/` cạnh binary — xem thêm
ở `vibaoc/src/main.rs::copy_runtime_pkg`):

```bash
export VIBAO_PKG_DIR=$(pwd)/vibao-runtime/pkg
./target/release/vibaoc build app.vbao --out dist
```

## Cấu trúc code

```
vibao-ast/src/       Định nghĩa AST (Expr, Element, App, Page...)
vibaoc/src/lexer/    Tokenizer — nguồn .vbao → Vec<Token>
vibaoc/src/parser/   Recursive-descent parser — Vec<Token> → AST
vibaoc/src/codegen/  AST → HTML/CSS/JS (+ đăng ký action/expr cho WASM)
vibao-runtime/src/   Chạy trong trình duyệt: state, DOM binding, router,
                     action, biểu thức động — biên dịch ra WASM
```

Mỗi module con trong `parser/` và `codegen/` đều có `#[cfg(test)]
mod tests` ở cuối file — khi sửa logic, thêm test cùng chỗ thay vì
tạo file test riêng.

## Quy ước code

- Comment giải thích **lý do** (why), không chỉ diễn giải lại code.
  Khi sửa 1 bug, ghi rõ `// BUG ĐÃ SỬA: ...` giải thích bug cũ là gì
  — giúp người sau không vô tình revert lại.
- Không dùng `{:?}` (Debug) trong message lỗi hiển thị cho người
  dùng ViBao — dùng `{}` (Display), xem `TokenKind::fmt` trong
  `vibaoc/src/lexer/token.rs` làm ví dụ. `{:?}` chỉ chấp nhận được
  trong `panic!`/test nội bộ.
- Ưu tiên không phụ thuộc thêm crate ngoài trừ khi thực sự cần —
  dự án cố tình tối giản dependency.

## Phát hành bản mới (release)

Xem [`scripts/RELEASE.md`](scripts/RELEASE.md) — quy trình build
binary release và publish lên GitHub Releases để dev cuối cài qua
`scripts/install.sh`.

## Báo lỗi / đề xuất

Mở issue trên GitHub, kèm:
- Đoạn `.vbao` tối thiểu để tái hiện lỗi
- Output đầy đủ của `vibaoc check <file> --ast` hoặc `vibaoc build`
- Kết quả mong đợi vs. thực tế
