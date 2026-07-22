#!/usr/bin/env bash
# ============================================================
# VIBAO — build-release.sh
#
# Build bản RELEASE (không phải debug) của compiler vibaoc + runtime
# WASM, rồi đóng gói thành 1 thư mục/tarball sẵn sàng upload lên
# GitHub Releases. Dev cuối tải file này về qua install.sh, KHÔNG
# cần cài Rust/wasm-pack gì cả — chỉ cần giải nén và thêm vào PATH.
#
# Chạy trên máy ĐÃ CÀI: rustup (cargo), target wasm32-unknown-unknown,
# wasm-pack (script sẽ tự cài phần thiếu nếu chưa có).
#
# Cách dùng:
#   ./scripts/build-release.sh [version]
#   vd: ./scripts/build-release.sh 0.0.4
#
# Kết quả: ./release/vibao-<version>-<os>-<arch>.tar.gz
# ============================================================
set -euo pipefail

VERSION="${1:-dev}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
RELEASE_DIR="$ROOT_DIR/release"
STAGE_DIR="$RELEASE_DIR/stage"

echo "==> ViBao release build (version: $VERSION)"

# ─── 1. Xác định OS/ARCH để đặt tên file đúng chuẩn (giống rustup) ───
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"
case "$OS" in
  linux*)  OS_NAME="linux" ;;
  darwin*) OS_NAME="macos" ;;
  *)       echo "⚠️  Hệ điều hành '$OS' chưa được hỗ trợ đóng gói tự động."; OS_NAME="$OS" ;;
esac
case "$ARCH" in
  x86_64|amd64) ARCH_NAME="x64" ;;
  arm64|aarch64) ARCH_NAME="arm64" ;;
  *) ARCH_NAME="$ARCH" ;;
esac
TARGET_TRIPLE="${OS_NAME}-${ARCH_NAME}"
PKG_NAME="vibao-${VERSION}-${TARGET_TRIPLE}"

echo "==> Target: $TARGET_TRIPLE"

# ─── 2. Đảm bảo toolchain đủ (wasm target + wasm-pack) ───
if ! rustup target list --installed | grep -q wasm32-unknown-unknown; then
  echo "==> Thêm target wasm32-unknown-unknown..."
  rustup target add wasm32-unknown-unknown
fi

if ! command -v wasm-pack >/dev/null 2>&1; then
  echo "==> Cài wasm-pack..."
  curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh
fi

# ─── 3. Build vibaoc (native binary, release) ───
echo "==> Build vibaoc (release)..."
cd "$ROOT_DIR"
cargo build --release -p vibaoc

# ─── 4. Build vibao-runtime ra WASM (release) ───
echo "==> Build vibao-runtime (wasm-pack, release)..."
cd "$ROOT_DIR/vibao-runtime"
wasm-pack build --release --target web --out-dir pkg
cd "$ROOT_DIR"

# ─── 5. Đóng gói: binary + pkg/ cạnh nhau (đúng layout mà
#        default_pkg_dir() trong main.rs mong đợi — xem comment ở đó) ───
echo "==> Đóng gói vào $STAGE_DIR/$PKG_NAME ..."
rm -rf "$STAGE_DIR"
mkdir -p "$STAGE_DIR/$PKG_NAME/pkg"

BIN_NAME="vibaoc"
if [ "$OS_NAME" = "windows" ]; then
  BIN_NAME="vibaoc.exe"
fi
cp "target/release/${BIN_NAME}" "$STAGE_DIR/$PKG_NAME/"
cp "vibao-runtime/pkg/vibao_runtime.js" "$STAGE_DIR/$PKG_NAME/pkg/"
cp "vibao-runtime/pkg/vibao_runtime_bg.wasm" "$STAGE_DIR/$PKG_NAME/pkg/"

# README nhỏ trong gói, phòng khi ai đó giải nén thủ công không qua install.sh
cat > "$STAGE_DIR/$PKG_NAME/README.txt" <<EOF
ViBao Compiler ${VERSION} (${TARGET_TRIPLE})

Cài nhanh (khuyến nghị): dùng install.sh ở repo GitHub thay vì giải nén tay.

Nếu giải nén thủ công:
  1. Đặt cả thư mục này (vibaoc + pkg/) vào 1 chỗ cố định, vd ~/.vibao/bin/
  2. Thêm thư mục đó vào PATH.
  3. Chạy: vibaoc build app.vbao
  (pkg/ PHẢI nằm cạnh vibaoc — compiler tự tìm runtime WASM ở đó.)
EOF

# ─── 6. Nén thành tarball ───
mkdir -p "$RELEASE_DIR"
cd "$STAGE_DIR"
tar -czf "$RELEASE_DIR/${PKG_NAME}.tar.gz" "$PKG_NAME"
cd "$ROOT_DIR"

echo ""
echo "✅ Xong: $RELEASE_DIR/${PKG_NAME}.tar.gz"
echo ""
echo "Bước tiếp theo:"
echo "  1. Vào GitHub repo -> Releases -> Draft a new release"
echo "  2. Đặt tag, vd v${VERSION}"
echo "  3. Upload file: $RELEASE_DIR/${PKG_NAME}.tar.gz"
echo "  4. Lặp lại script này trên máy macOS/Windows nếu muốn hỗ trợ đa nền tảng"
echo "     (mỗi OS/ARCH cần build + upload file .tar.gz riêng)."
