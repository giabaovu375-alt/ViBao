# ViBao — Đặc tả ngôn ngữ (tài liệu tham chiếu cho AI)

> Tài liệu này được trích xuất TRỰC TIẾP từ mã nguồn compiler (`vibaoc`)
> và runtime (`vibao-runtime`) tại thời điểm viết — không phải mô tả lý
> tưởng hay dự định. Mọi API/cú pháp liệt kê ở đây ĐÃ CHẠY ĐƯỢC THẬT,
> đã qua build + test xác nhận (198/198 test pass tại thời điểm cập
> nhật này). Phần nào CHƯA hoạt động được ghi rõ trong mục "Giới hạn
> hiện tại" — không suy đoán/bổ sung tính năng ngoài danh sách này khi
> sinh code ViBao.

## 1. Tổng quan

ViBao là ngôn ngữ lập trình biên dịch sang web, cú pháp tiếng Việt
không dấu (snake_case), tương tự Svelte/Vue compiler nhưng viết bằng
Rust cho cả compiler lẫn runtime.

```
app.vbao → vibaoc build → HTML + CSS + JS + WASM → trình duyệt
```

- File nguồn: đuôi `.vbao` (không dùng `.v` — trùng V-lang/Verilog).
- Build ra 1 SPA (Single-Page App) THẬT — mọi trang gộp chung 1
  `index.html`, điều hướng qua JS/WASM router (History API), không
  reload trang. Mọi `id` HTML là DUY NHẤT TOÀN CỤC xuyên suốt cả app
  (không riêng từng trang) — đã sửa bug ID trùng lặp giữa các trang.
- Runtime (state, biểu thức động, DOM binding, action, animation,
  switch, page lifecycle) chạy bằng WebAssembly biên dịch từ Rust —
  KHÔNG dùng `eval()`/`new Function()` phía JS ở bất kỳ đâu.

## 2. Cấu trúc chương trình

```vbao
ung_dung("Tên ứng dụng") {
    trang("/") {
        // nội dung trang chủ
    }

    trang("/gioi-thieu", "Tên trang", mau_nen: xam_nhat) {
        // nội dung trang khác, có tên hiển thị + màu nền riêng
    }
}
```

- `ung_dung("Tên")` — khối gốc bắt buộc, bao mọi thứ khác.
- `trang("/duong-dan", "Tên trang", mau_nen: <mau>)` — khai báo 1
  trang theo route. Tham số thứ 2 (tên) và named option `mau_nen`
  (màu nền cấp trang) đều TUỲ CHỌN và có thể xuất hiện theo BẤT KỲ THỨ
  TỰ nào sau route. `mau_nen` nhận tên màu (mục 4), mã hex, hoặc biến
  `$ten`.
- Route hỗ trợ tham số động dạng `:ten` (vd `"/san-pham/:id"`) — được
  `router.rs` (runtime) so khớp lúc điều hướng, giá trị bơm vào state
  dưới đúng tên (`$id`).

## 3. Component/Tag hợp lệ

**Chỉ các tag dưới đây thực sự sinh ra HTML đúng nghĩa.** Tag khác nằm
ngoài danh sách này sẽ compile được (lexer/parser chấp nhận rộng hơn)
nhưng codegen **fallback về `<div>` rỗng** — xem mục 9.

| Tag ViBao | HTML sinh ra | Ghi chú |
|---|---|---|
| `text` | `<p>` | |
| `h1`, `h2`, `h3` | `<h1>`/`<h2>`/`<h3>` | |
| `p` | `<p>` | |
| `nhan` | `<span>` | "nhãn" |
| `button` | `<button>` | |
| `link`, `lien_ket` | `<a>` | Prop `den: "/path"` tự sinh `href` + điều hướng SPA khi click (xem mục 4) |
| `image` | `<img>` | |
| `video` | `<video>` | |
| `icon` | `<span>` | |
| `input` | `<input>` | |
| `spacer` | `<div>` | |
| `divider` | `<hr>` | |
| `vong_quay` | `<div>` | placeholder, chưa có logic spinner thật |
| `thanh_tien_trinh` | `<div>` | placeholder |
| `scroll` | `<div>` | (cũng là layout tag, xem mục 4) |
| `container` | `<div>` | (cũng là layout tag) |

### Layout tag (hỗ trợ CSS layout đầy đủ hơn — flexbox/grid)

| Tag | CSS display |
|---|---|
| `flex` | `display: flex` |
| `grid` | `display: grid` |
| `box` | block thường, có padding/màu/border |
| `stack` | grid ép `1fr` (chồng lớp) |
| `scroll` | box có `overflow` |
| `container` | box căn giữa, giới hạn `max_rong` |
| `layer` | `position: relative` (làm gốc cho absolute con) |
| `dinh_dau` | `position: sticky; top: 0` |
| `dinh_man_hinh` | `position: fixed` |

## 4. Cú pháp Element

```vbao
tag(prop1: value1, prop2: value2, ...) {
    // children (element con, event block, animation/responsive block)
}
```

- Props (`key: value`) **PHẢI nằm trong `(...)`** ngay sau tên tag —
  KHÔNG được viết `key: value` bên trong `{...}`.
- `{...}` **chỉ chứa**: element con, khối sự kiện (`on_click { ... }`).
- Với `text`/`button`/`link`/... — tham số string đầu tiên (không có
  tên) tự động gán vào prop `noi_dung`:
  ```vbao
  text("Xin chào", co: 16)   // noi_dung = "Xin chào", co = 16
  ```

### Bảng prop hỗ trợ (simple element — text/button/input/...)

| Prop | CSS/HTML | Ví dụ |
|---|---|---|
| `mau`, `mau_chu` | `color` | `mau: xanh` |
| `mau_vien` | `border-color` | |
| `width` | `width` | `width: 200` (px) hoặc `width: 50%` |
| `height` | `height` | |
| `max_rong` | `max-width` | |
| `radius` | `border-radius` | |
| `dem` | `padding` | **KHÔNG phải `padding`** |
| `le` | `margin` | |
| `vien` | `border-width` | |
| `kieu_vien` | `border-style` | |
| `bong` | `box-shadow` | |
| `overflow` | `overflow` | |
| `tang_z` | `z-index` | |
| `co` | `font-size` | **KHÔNG phải `co_chu`** |
| `dam` | `font-weight: bold` (bool) | `dam: true` |
| `nghieng` | `font-style: italic` (bool) | |
| `gach_chan` | `text-decoration: underline` (bool) | |
| `can` | `text-align` (`trai`/`phai`/`giua`/`deu`) | |
| `hang` | `line-height` | |
| `khoang_chu` | `letter-spacing` | |
| `bien_doi` | `transform` | |
| `font` | `font-family` | |
| `huong` | flex-direction (element `flex`) | |
| `gap` | `gap` | |
| `doc`, `boc` | flex-wrap liên quan | |
| `fit` | `object-fit` (ảnh/video) | |
| `alt` | `alt` (ảnh) | |
| `lazy` | `loading="lazy"` (bool) | |
| `loai` | `type` (input) | |
| `chu_tro` | `placeholder` (input) | |
| `bat_buoc` | `required` (bool, input) | |
| `vo_hieu` | `disabled` (bool) | |
| `gia_tri` | `value` (input) | |
| `noi_dung` | text content | tự gán từ tham số string đầu |
| `den` | `href` + điều hướng SPA | **CHỈ dùng cho `link`/`lien_ket`** — giá trị PHẢI TĨNH (string literal), route ĐỘNG (phụ thuộc state) chưa hỗ trợ, dùng `button` + `dieu_huong()` thay thế |

### Prop riêng cho layout tag (`box`/`flex`/`grid`/...)

| Prop | Áp dụng | Ghi chú |
|---|---|---|
| `color` | mọi layout tag | **background-color, KHÔNG phải màu chữ** |
| `dem` | mọi layout tag | padding |
| `le` | box | margin |
| `radius` | box | border-radius |
| `vien` | box | border-width |
| `bong` | box | box-shadow |
| `tran_x`, `tran_y` | box | overflow-x/y |
| `huong` | flex | row/column |
| `gap`, `gap_doc`, `gap_ngang` | flex/grid | |
| `can` | flex | `justify-content` — nhận `start`/`end`/`center`/`giua` |
| `doc` | flex | `align-items` — nhận `start`/`end`/`center`/`giua`/`stretch`/`deu` (đều map về `stretch`) |
| `cot`, `hang_luoi` | grid | grid-template-columns/rows |
| `min_rong`, `max_rong`, `min_cao`, `max_cao` | container | |
| `vi_tri` | dinh_dau/dinh_man_hinh | `tren`/`duoi`/`trai`/`phai` |

### Giá trị màu hợp lệ (tên tiếng Việt → hex)

```
trang=#FFFFFF   den=#000000    do=#E53E3E    xanh=#3182CE
xanh_la=#38A169 vang=#F59E0B   hong=#D53F8C  tim=#805AD5
cam=#DD6B20     xam=#718096    xam_nhat=#F7FAFC
xam_dam=#2D3748 luc=#25855A    nau=#7B341E
```

Chỉ đúng các tên này được resolve ra hex. Tên khác (vd `xanh_nhat` —
KHÔNG tồn tại) sẽ được coi là chuỗi thường, in thẳng ra CSS (sai, CSS
không hiểu tên đó) — LUÔN kiểm tra tên màu nằm trong danh sách này
trước khi dùng.

### Animation

```vbao
box() {
    hieu_ung_hover: "phong_to"     // đặt trong (...) cùng props khác — TODO xác nhận cú pháp chính xác qua parser
}
```

Hover/scroll animation sinh ra HTML attribute (`data-vb-anim-hover`,
`data-vb-anim-scroll`), runtime tự bind bằng `web-sys`
(mouseenter/mouseleave, IntersectionObserver) — Rust thuần, không JS.

## 5. Biểu thức (Expr)

```vbao
$ten_bien              // đọc biến (không cần dấu $ khi khai báo, CÓ dấu $ khi dùng trong biểu thức)
$a + $b                // + là cộng số HOẶC nối chuỗi (tùy kiểu, giống JS)
$a - $b, $a * $b, $a / $b, $a % $b
$a == $b, $a != $b     // so sánh nghiêm ngặt (===), không ép kiểu
$a > $b, $a >= $b, $a < $b, $a <= $b
$a && $b, $a || $b, !$a
"Xin chào $ten"        // template string, nội suy biến
lam_tron($n)            // hàm tiện ích, xem mục 7
```

## 6. Điều khiển luồng

```vbao
neu $dieu_kien {
    // ...
} khong_thi {
    // ...
}
```

```vbao
vong_lap $item trong $danh_sach {
    text($item.ten)
    button("Xoá") {
        on_click { xoa($item) }   // $item resolve ĐÚNG bên trong action (đã sửa lỗ hổng loop-action)
    }
}
```

```vbao
truong_hop $trang_thai {
    "dang_tai" {
        text("Đang tải...")
    }
    "loi" {
        text("Có lỗi xảy ra")
    }
    mac_dinh {
        text("Sẵn sàng")
    }
}
```

`truong_hop`/switch — CÚ PHÁP MỚI HOẠT ĐỘNG (trước đây chưa có parser
dù AST/codegen đã sẵn sàng). So khớp bằng `==` nghiêm ngặt (giống mọi
so sánh khác trong ViBao). `mac_dinh` là tuỳ chọn, tối đa 1 khối.

## 7. Sự kiện & Hành động (Action)

```vbao
button("Bấm tui") {
    on_click {
        thong_bao("Xin chào!", kieu: thanh_cong)
    }
}
```

Sự kiện hỗ trợ: `on_click`, `on_hover`, `on_blur`, `on_focus`,
`on_change`, `on_submit`, `on_scroll`.

### Sự kiện vòng đời trang

```vbao
trang("/") {
    on_tai {
        // chạy khi trang được điều hướng TỚI (kể cả lần đầu boot)
    }
    on_huy {
        // chạy khi router điều hướng RỜI khỏi trang này
    }
    // ... nội dung trang
}
```

`on_tai`/`on_huy` giờ chạy qua action registry (Rust thuần), nối vào
`router.rs::activate_page` — `on_huy` của trang cũ chạy TRƯỚC khi ẩn
nó, `on_tai` của trang mới chạy SAU khi hiện + bind xong.

### Hàm hành động đã hoạt động thật (chạy bằng Rust/WASM, KHÔNG phải JS)

| Hàm | Tham số | Mô tả |
|---|---|---|
| `thong_bao(text, kieu: ..., thoi_gian: ...)` | | Toast tạm thời |
| `canh_bao(text)` | | `window.alert()` |
| `dieu_huong(path)` | | Điều hướng SPA thật (History API, không reload) |
| `mo_tab_moi(path)` | | Mở tab mới |
| `mo_modal(id)` / `dong_modal(id)` | | Cần HTML có sẵn `id="vb-modal-<id>"` |
| `cuon_den(target)` | | Cuộn mượt tới phần tử |
| `cuon_len_dau()` | | Cuộn về đầu trang |
| `luu_du_lieu(endpoint, data)` | | POST qua fetch thật |
| `tai_du_lieu(endpoint)` | | GET qua fetch thật |
| `sao_chep(text)` | | **TẮT** — xem mục 9 |

### Assignment và if-action

```vbao
$dem = $dem + 1
neu $dem > 10 {
    thong_bao("Đủ rồi!")
}
```

## 8. Component tự định nghĩa (`@the`)

```vbao
@the ThanhVien(ten: chuoi, tuoi: so) {
    box(dem: 16, color: xam_nhat) {
        text($ten, co: 18, dam: true)
        text("Tuổi: $tuoi", co: 14)
    }
}

// dùng lại:
ThanhVien(ten: "An", tuoi: 20)
ThanhVien(ten: "Bình", tuoi: 25)
```

`@the` GIỜ HOẠT ĐỘNG ĐÚNG (bug thật đã sửa — trước đây điều kiện nhận
diện `@the` dùng so sánh sai kiểu, khiến BẤT KỲ `@xxx` nào cũng bị
hiểu nhầm thành `@the`, có thể gây parse sai âm thầm). Tham số khai
báo kiểu qua `ten: kieu` (`chuoi`/`so`/`mau`/`bool`/`mang`/`doi_tuong`/
`hanh_dong`/`any`).

## 9. Giới hạn hiện tại (KHÔNG suy đoán ngoài danh sách này)

- **Component phức tạp** (`modal`, `tabs`, `accordion`, `carousel`,
  `bang`, `bieu_do`, `ban_do`, `form`, `nhom_input`, `chon_mot`,
  `hop_kiem`, `lua_chon`, `thanh_nav`, `trinh_soan_thao`,
  `xuong_trang`) — lexer nhận diện tên, nhưng codegen **fallback về
  `<div>` rỗng** (chưa có logic sinh HTML thật). Dùng `@the` (mục 8)
  để tự xây component tương đương nếu cần.
- **`sao_chep` (clipboard)** — vô hiệu hoá tạm thời (cần cờ build đặc
  biệt `--cfg=web_sys_unstable_apis`, chưa cấu hình vì rủi ro làm
  hỏng build nếu thiếu cờ).
- **`den` (link) với route ĐỘNG** — chỉ hỗ trợ giá trị TĨNH (string
  literal); nếu đích điều hướng phụ thuộc state, dùng `button` +
  `on_click { dieu_huong($bien) }`.
- **Auth/guard route** — chưa có cú pháp khai báo trong ngôn ngữ.
- **`form`/`input` 2-way binding (`bind_model`)** — có code runtime
  nhưng CHƯA test thực tế với input thật trong trình duyệt.
- **`goi_api`** — có code runtime (`api.rs`, dùng `fetch` thật qua
  web-sys) nhưng CHƯA test với 1 API endpoint thật, chỉ test hàm
  `resolve_url` (ghép URL) đơn lẻ.
- **Component phức tạp lồng bên trong `@the`** — `@the` mới test ở
  mức parse/codegen cơ bản, CHƯA test trường hợp lồng nhiều tầng
  (component gọi component khác) hay dùng chung với `vong_lap`/
  `truong_hop`.

## 10. Build & chạy

```bash
vibaoc build app.vbao --out dist    # ghi ra dist/, gồm HTML/CSS/JS + pkg/ (WASM)
vibaoc check app.vbao --ast          # debug: in AST, không ghi file
```

`dist/` là artifact build — KHÔNG commit vào git (`.gitignore`), giống
`node_modules/`/`target/`. Cần `VIBAO_PKG_DIR` (biến môi trường) trỏ
tới thư mục chứa `vibao_runtime.js` + `vibao_runtime_bg.wasm` (sinh ra
bởi `wasm-bindgen-cli --target web`) để `vibaoc build` tự copy vào
`dist/pkg/` — thiếu bước này, trang build ra thiếu hoàn toàn phần
tương tác động (state/action/router).
