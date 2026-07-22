# ViBao

ViBao là ngôn ngữ lập trình biên dịch sang web, cú pháp tiếng Việt
không dấu (snake_case) — tương tự Svelte/Vue compiler, nhưng compiler
lẫn runtime đều viết bằng Rust.

```
app.vbao  →  vibaoc build  →  HTML + CSS + JS + WASM  →  trình duyệt
```

- Build ra 1 SPA thật — mọi trang gộp chung 1 `index.html`, điều
  hướng qua router chạy WASM (History API), không reload trang.
- Runtime (state, biểu thức động, DOM binding, action, animation,
  vòng lặp, switch, page lifecycle...) chạy bằng WebAssembly biên
  dịch từ Rust — không dùng `eval()`/`new Function()` phía JS.

Xem đặc tả đầy đủ tại [`docs/VIBAO_SPEC.md`](docs/VIBAO_SPEC.md).

## Cài đặt (dev dùng ViBao — không cần biết Rust)

```bash
curl -fsSL https://raw.githubusercontent.com/giabaovu375-alt/ViBao/main/scripts/install.sh | sh
```

Script tự tải bản build sẵn phù hợp với hệ điều hành, cài vào
`~/.vibao/bin`, và thêm vào `PATH`. Mở terminal mới rồi thử:

```bash
vibaoc --help
```

## Ví dụ nhanh

Tạo file `app.vbao`:

```vbao
ung_dung("App đầu tiên") {
    trang("/") {
        box(dem: 32) {
            text("Xin chào ViBao!", co: 24, dam: true)
            button("Bấm tui") {
                on_click {
                    thong_bao("Đã bấm!", kieu: thanh_cong)
                }
            }
        }
    }
}
```

Build:

```bash
vibaoc build app.vbao --out dist
```

Mở `dist/index.html` trong trình duyệt là xong. Muốn debug nhanh
(xem lỗi/AST, không ghi file):

```bash
vibaoc check app.vbao --ast
```

## Trạng thái dự án

Đây là compiler đang phát triển tích cực, không phải bản 1.0 ổn
định. Trước khi dùng, đọc mục **"Giới hạn hiện tại"** trong
[`docs/VIBAO_SPEC.md`](docs/VIBAO_SPEC.md) — liệt kê rõ phần nào đã
chạy thật (có test xác nhận) và phần nào chưa (component phức tạp
còn fallback `<div>` rỗng, `goi_api` chưa test với endpoint thật,
v.v.), để không suy đoán tính năng ngoài danh sách đó.

## Muốn đóng góp code compiler?

Xem [`CONTRIBUTING.md`](CONTRIBUTING.md) — hướng dẫn build từ
source, chạy test, và quy trình phát hành bản mới.

## Cấu trúc repo

```
vibao-ast/       AST — định nghĩa cấu trúc dữ liệu chương trình ViBao
vibaoc/          Compiler: lexer → parser → codegen (sinh HTML/CSS/JS)
vibao-runtime/   Runtime chạy trong trình duyệt (biên dịch ra WASM)
scripts/         Script build-release + install cho dev cuối
docs/            Đặc tả ngôn ngữ
```

## License

Chưa xác định — thêm license trước khi công khai chia sẻ rộng rãi.
