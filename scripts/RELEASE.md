# Quy trình phát hành ViBao (cho maintainer)

Mục tiêu: dev khác cài `vibaoc` bằng 1 dòng lệnh, không cần cài Rust.

## Lần đầu setup repo

1. Đẩy toàn bộ source (kể cả thư mục `scripts/`) lên GitHub:
   ```
   git add scripts/
   git commit -m "Add release scripts"
   git push
   ```
   `install.sh` sẽ được tải qua link raw:
   `https://raw.githubusercontent.com/giabaovu375-alt/ViBao/main/scripts/install.sh`
   — chỉ cần file này nằm trên nhánh `main` là dev tải được ngay,
   KHÔNG cần đợi release nào cả.

## Mỗi lần ra bản mới

1. Trên máy có cargo + wasm-pack (Kaggle notebook vẫn dùng được):
   ```bash
   chmod +x scripts/build-release.sh
   ./scripts/build-release.sh 0.0.5
   ```
   Ra file: `release/vibao-0.0.5-linux-x64.tar.gz`

2. Vào GitHub repo → **Releases** → **Draft a new release**
   - Tag: `v0.0.5` (khớp version vừa build)
   - Đính kèm file `.tar.gz` vừa tạo
   - Publish

3. Xong — dev chạy:
   ```bash
   curl -fsSL https://raw.githubusercontent.com/giabaovu375-alt/ViBao/main/scripts/install.sh | sh
   ```
   sẽ tự tải đúng bản mới nhất.

## Hỗ trợ thêm macOS / nhiều nền tảng (khi cần)

`build-release.sh` tự nhận diện OS/ARCH của máy đang chạy. Muốn phát
hành cho macOS (Intel hoặc Apple Silicon) thì chạy lại chính script đó
**trên máy Mac thật** (hoặc CI macOS runner) rồi upload thêm file
`.tar.gz` tương ứng vào CÙNG 1 release GitHub. `install.sh` ở máy dev
sẽ tự chọn đúng file theo OS/ARCH của họ.

Hiện tại CHƯA hỗ trợ Windows tự động — nếu cần, thêm nhánh xử lý
`.exe` + `.zip` (thay vì `.tar.gz`) vào cả 2 script.

## Kiểm tra nhanh trước khi publish

```bash
# Trên máy vừa build xong:
cd release/stage/vibao-0.0.5-linux-x64
./vibaoc --help
./vibaoc build ../../../app.vbao --out /tmp/test-dist
ls /tmp/test-dist   # phải thấy index.html, style.css, app.js, pkg/
```
