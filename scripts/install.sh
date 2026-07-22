#!/usr/bin/env bash
# ============================================================
# VIBAO — install.sh
#
# Cài đặt lệnh `vibaoc` (ViBao compiler) — giống cách cài tsc/deno/bun:
# tải sẵn binary đã build, KHÔNG cần cài Rust/cargo/wasm-pack.
#
# Dùng:
#   curl -fsSL https://raw.githubusercontent.com/giabaovu375-alt/ViBao/main/scripts/install.sh | sh
#
# Có thể chỉ định version cụ thể:
#   curl -fsSL .../install.sh | sh -s -- v0.0.4
# ============================================================
set -euo pipefail

REPO="giabaovu375-alt/ViBao"
INSTALL_DIR="${VIBAO_INSTALL_DIR:-$HOME/.vibao}"
BIN_DIR="$INSTALL_DIR/bin"
REQUESTED_VERSION="${1:-latest}"

info()  { printf '\033[1;34m==>\033[0m %s\n' "$1"; }
warn()  { printf '\033[1;33m⚠️  %s\033[0m\n' "$1"; }
error() { printf '\033[1;31m❌ %s\033[0m\n' "$1" >&2; exit 1; }

# ─── 1. Phát hiện OS/ARCH ───
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "$OS" in
  linux)  OS_NAME="linux" ;;
  darwin) OS_NAME="macos" ;;
  *) error "Hệ điều hành '$OS' chưa được hỗ trợ. Xem README để build thủ công từ source." ;;
esac

case "$ARCH" in
  x86_64|amd64)  ARCH_NAME="x64" ;;
  arm64|aarch64) ARCH_NAME="arm64" ;;
  *) error "Kiến trúc CPU '$ARCH' chưa được hỗ trợ. Xem README để build thủ công từ source." ;;
esac

TARGET_TRIPLE="${OS_NAME}-${ARCH_NAME}"
info "Phát hiện hệ thống: $TARGET_TRIPLE"

# ─── 2. Xác định URL tải qua GitHub API (tránh phải đoán đúng tên file
#        theo từng version — tìm asset có tên chứa đúng TARGET_TRIPLE) ───
if [ "$REQUESTED_VERSION" = "latest" ]; then
  API_URL="https://api.github.com/repos/${REPO}/releases/latest"
else
  API_URL="https://api.github.com/repos/${REPO}/releases/tags/${REQUESTED_VERSION}"
fi

info "Đang tra cứu bản release ($REQUESTED_VERSION)..."
ASSET_URL="$(curl -fsSL "$API_URL" \
  | grep "browser_download_url.*${TARGET_TRIPLE}" \
  | grep -o 'https://[^"]*' \
  | head -n1)"

if [ -z "$ASSET_URL" ]; then
  error "Không tìm thấy bản release phù hợp cho ${TARGET_TRIPLE}. Kiểm tra: https://github.com/${REPO}/releases"
fi

info "Tải: $ASSET_URL"

# ─── 3. Tải + giải nén ───
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

curl -fsSL "$ASSET_URL" -o "$TMP_DIR/vibao.tar.gz" \
  || error "Tải thất bại. Kiểm tra kết nối mạng hoặc URL release."

info "Giải nén vào $BIN_DIR ..."
rm -rf "$BIN_DIR"
mkdir -p "$BIN_DIR"
tar -xzf "$TMP_DIR/vibao.tar.gz" -C "$TMP_DIR"

# Tarball chứa 1 thư mục con (vd vibao-0.0.4-linux-x64/) — copy nội
# dung thư mục đó (không copy chính thư mục) vào BIN_DIR để layout
# cuối cùng là: $BIN_DIR/vibaoc + $BIN_DIR/pkg/ (đúng như
# default_pkg_dir() trong main.rs mong đợi — pkg/ cạnh vibaoc).
EXTRACTED_DIR="$(find "$TMP_DIR" -mindepth 1 -maxdepth 1 -type d | head -n1)"
if [ -z "$EXTRACTED_DIR" ]; then
  error "Gói tải về có cấu trúc không như mong đợi (không tìm thấy thư mục con)."
fi
cp -R "$EXTRACTED_DIR"/. "$BIN_DIR/"
chmod +x "$BIN_DIR/vibaoc" 2>/dev/null || true

# ─── 4. Thêm vào PATH ───
add_to_path_line='export PATH="$HOME/.vibao/bin:$PATH"'
SHELL_RC=""
case "${SHELL:-}" in
  */zsh)  SHELL_RC="$HOME/.zshrc" ;;
  */bash) SHELL_RC="$HOME/.bashrc" ;;
  *)      SHELL_RC="$HOME/.profile" ;;
esac

if [ -f "$SHELL_RC" ] && grep -qF "$add_to_path_line" "$SHELL_RC" 2>/dev/null; then
  info "PATH đã được cấu hình sẵn trong $SHELL_RC"
else
  echo "" >> "$SHELL_RC"
  echo "# Added by ViBao installer" >> "$SHELL_RC"
  echo "$add_to_path_line" >> "$SHELL_RC"
  info "Đã thêm $BIN_DIR vào PATH (trong $SHELL_RC)"
fi

echo ""
echo "✅ Cài đặt xong!"
echo ""
echo "   Mở terminal MỚI (hoặc chạy: source $SHELL_RC), sau đó thử:"
echo ""
echo "     vibaoc --help"
echo "     vibaoc build app.vbao"
echo ""
