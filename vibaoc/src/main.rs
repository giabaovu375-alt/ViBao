// ============================================================
// VIBAO COMPILER (Rust) — main.rs
// CLI entry point.
//
// Lệnh `vibaoc build <file.v>` chạy full pipeline Lexer → Parser →
// Codegen, RỒI GHI RA FILE THẬT trong thư mục output (mặc định
// "dist/") — đây là bộ file mà TRÌNH DUYỆT đọc để chạy trang, tương tự
// cách `tsc` biên dịch .ts ra .js: người dùng ViBao không "đọc" nội
// dung các file này, họ chỉ cần chúng tồn tại đúng chỗ để mở index.html
// lên là chạy được.
//
// Lệnh `vibaoc check <file.v> [--ast]` giữ hành vi debug cũ (in ra
// terminal, không ghi file) — dùng lúc phát triển compiler, không phải
// lệnh dành cho người dùng ViBao cuối.
// ============================================================

mod lexer;
mod parser;
mod codegen;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        print_usage();
        process::exit(1);
    }

    let subcommand = &args[1];
    let path = &args[2];

    match subcommand.as_str() {
        "build" => {
            let out_dir = parse_out_dir(&args).unwrap_or_else(|| PathBuf::from("dist"));
            cmd_build(path, &out_dir);
        }
        "check" => {
            let ast_only = args.iter().any(|a| a == "--ast");
            cmd_check(path, ast_only);
        }
        _ => {
            print_usage();
            process::exit(1);
        }
    }
}

fn print_usage() {
    eprintln!("Cách dùng:");
    eprintln!("  vibaoc build <file.vbao> [--out <thư_mục>]");
    eprintln!("      Biên dịch và GHI RA FILE thật (mặc định vào ./dist/) —");
    eprintln!("      đây là bộ file để trình duyệt đọc, giống output của tsc.");
    eprintln!("  vibaoc check <file.vbao> [--ast]");
    eprintln!("      Chỉ kiểm tra lỗi, in kết quả ra terminal để debug —");
    eprintln!("      KHÔNG ghi file. Thêm --ast để in cây cú pháp thay vì HTML/CSS/JS.");
}

fn parse_out_dir(args: &[String]) -> Option<PathBuf> {
    let idx = args.iter().position(|a| a == "--out")?;
    args.get(idx + 1).map(PathBuf::from)
}

// ════════════════════════════════════════════════════════════
// PIPELINE DÙNG CHUNG — lexer → parser → codegen
// ════════════════════════════════════════════════════════════

fn compile(path: &str) -> codegen::CodegenOutput {
    // Quy ước đuôi file: ".vbao" — CHỦ ĐÍCH chọn khác ".v" vì ".v" đã
    // trùng với 2 ngôn ngữ phổ biến khác (V-lang tại vlang.io, và
    // Verilog — ngôn ngữ mô tả phần cứng rất phổ biến trong ngành điện
    // tử), dễ gây nhầm cú pháp highlight trong editor và xung đột
    // tooling (LSP...) về sau. Đây chỉ là CẢNH BÁO MỀM — không chặn
    // build — vì compiler không thực sự quan tâm đuôi file, chỉ đọc nội
    // dung text thô; validate ở đây thuần tuý để nhắc người dùng theo
    // đúng quy ước, tránh nhầm lẫn khi chia sẻ code/tài liệu.
    if !path.ends_with(".vbao") {
        eprintln!(
            "⚠️  Quy ước ViBao dùng đuôi file \".vbao\" (vd \"app.vbao\") — \
             \"{}\" không khớp. Vẫn tiếp tục build bình thường.",
            path
        );
    }

    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Không đọc được file '{}': {}", path, e);
            process::exit(1);
        }
    };

    let tokens = match lexer::tokenize(&source) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("❌ {}", e);
            process::exit(1);
        }
    };

    let program = match parser::parse(tokens) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("❌ {}", e);
            process::exit(1);
        }
    };

    let mut gen = codegen::Codegen::new(codegen::CodegenOptions::default());
    let output = gen.generate(&program);

    for w in &output.warnings {
        eprintln!("⚠️  {}", w);
    }

    output
}

// ════════════════════════════════════════════════════════════
// `vibaoc check` — debug, in ra terminal, KHÔNG ghi file
// ════════════════════════════════════════════════════════════

fn cmd_check(path: &str, ast_only: bool) {
    if ast_only {
        let source = match fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Không đọc được file '{}': {}", path, e);
                process::exit(1);
            }
        };
        let tokens = match lexer::tokenize(&source) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("❌ {}", e);
                process::exit(1);
            }
        };
        let program = match parser::parse(tokens) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("❌ {}", e);
                process::exit(1);
            }
        };
        println!("✅ Parse thành công!\n");
        println!("{:#?}", program);
        return;
    }

    let output = compile(path);
    println!("=== HTML ===\n{}\n", output.html);
    println!("=== CSS ===\n{}\n", output.css);
    println!("=== JS ===\n{}\n", output.js);
}

// ════════════════════════════════════════════════════════════
// `vibaoc build` — GHI FILE THẬT, đây là lệnh người dùng ViBao dùng
// ════════════════════════════════════════════════════════════

fn cmd_build(path: &str, out_dir: &Path) {
    let output = compile(path);

    if let Err(e) = fs::create_dir_all(out_dir) {
        eprintln!("Không tạo được thư mục output '{}': {}", out_dir.display(), e);
        process::exit(1);
    }

    // ── style.css ──────────────────────────────────────────────────
    let css_path = out_dir.join("style.css");
    write_file(&css_path, &output.css);

    // ── app.js ───────────────────────────────────────────────────────
    let js_path = out_dir.join("app.js");
    write_file(&js_path, &output.js);

    // ── index.html — SPA THẬT: TẤT CẢ trang gộp chung 1 file ──────────
    // Mỗi route trong output.pages đã tự mang theo
    // `<div class="vb-page" data-route="...">` (xem codegen/mod.rs).
    // Ở đây chỉ cần NỐI toàn bộ các div đó lại, không tách file riêng —
    // router.rs (runtime WASM) sẽ tự ẩn/hiện đúng div theo URL hiện tại,
    // không có bước load lại trang nào cả. Đây LÀ điểm khác biệt cốt lõi
    // so với thiết kế MPA cũ (mỗi route 1 file .html riêng) — ViBao build
    // ra 1 ứng dụng SPA thật, đúng bản chất "ung_dung" (ứng dụng), không
    // phải nhiều "trang tĩnh" rời rạc.
    //
    // Thứ tự nối: ưu tiên route "/" lên đầu (nếu có) để dễ đọc bằng mắt
    // khi debug — thứ tự không ảnh hưởng hành vi runtime vì router chọn
    // đúng div theo `data-route`, không dựa vào vị trí trong DOM.
    let mut routes: Vec<&String> = output.pages.keys().collect();
    routes.sort_by_key(|r| if r.as_str() == "/" { 0 } else { 1 });

    let all_pages_html: String = routes
        .iter()
        .map(|r| output.pages.get(*r).cloned().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");

    let full_html = assemble_html_page(&all_pages_html);
    let index_path = out_dir.join("index.html");
    write_file(&index_path, &full_html);

    // ── pkg/ (runtime WASM) ──────────────────────────────────────────
    // Đây là 2 file do `wasm-bindgen-cli` sinh ra từ crate vibao-runtime
    // (KHÔNG phải do vibaoc tự build lúc này — build wasm là việc làm
    // 1 LẦN bởi người phát triển ViBao, không phải mỗi lần Dev gõ
    // `vibaoc build`). Ở đây ta chỉ COPY chúng từ nơi đóng gói cùng
    // vibaoc (VIBAO_PKG_DIR) sang thư mục output, nếu tìm thấy.
    copy_runtime_pkg(out_dir);

    println!("✅ Build xong! Mở file sau trong trình duyệt để xem kết quả:");
    println!("   {}", index_path.display());
}

fn write_file(path: &Path, content: &str) {
    if let Err(e) = fs::write(path, content) {
        eprintln!("Không ghi được file '{}': {}", path.display(), e);
        process::exit(1);
    }
}

/// Ráp `body_html` (nội dung do codegen sinh cho 1 trang) thành 1 file
/// .html hoàn chỉnh — thêm khung <!DOCTYPE html><html><head>...</head>
/// <body>...</body></html>, liên kết style.css và app.js đúng thứ tự.
///
/// LƯU Ý: app.js được nhúng bằng <script src="./app.js"></script>
/// KHÔNG có `type="module"` — vì bootstrap bên trong app.js dùng
/// dynamic import() (xem codegen/mod.rs::gen_app_js), cú pháp này hoạt
/// động trong classic script bình thường, không bắt buộc phải khai
/// type="module" ở đây.
fn assemble_html_page(body_html: &str) -> String {
    format!(
        r#"<!DOCTYPE html>
<html lang="vi">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>ViBao App</title>
  <link rel="stylesheet" href="./style.css">
</head>
<body>
{body}
  <script src="./app.js"></script>
</body>
</html>
"#,
        body = body_html
    )
}

/// Copy pkg/vibao_runtime.js + pkg/vibao_runtime_bg.wasm từ thư mục
/// đóng gói cùng vibaoc (xác định qua biến môi trường VIBAO_PKG_DIR,
/// hoặc thư mục "pkg" cạnh chính binary vibaoc nếu biến đó không đặt)
/// sang <out_dir>/pkg/. Nếu không tìm thấy nguồn, in cảnh báo thay vì
/// panic — người dùng vẫn có HTML/CSS/JS để xem, chỉ thiếu phần WASM
/// (trang sẽ hiển thị khung nhưng không có tương tác động).
fn copy_runtime_pkg(out_dir: &Path) {
    let src_dir = env::var("VIBAO_PKG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| default_pkg_dir());

    let files = ["vibao_runtime.js", "vibao_runtime_bg.wasm"];
    let dest_pkg_dir = out_dir.join("pkg");

    let all_exist = files.iter().all(|f| src_dir.join(f).is_file());
    if !all_exist {
        eprintln!(
            "⚠️  Không tìm thấy runtime WASM đóng gói sẵn tại '{}'.",
            src_dir.display()
        );
        eprintln!(
            "   Trang sinh ra sẽ thiếu phần tương tác động (state, if, vòng lặp...)."
        );
        eprintln!(
            "   Đặt biến môi trường VIBAO_PKG_DIR trỏ tới thư mục chứa"
        );
        eprintln!("   vibao_runtime.js + vibao_runtime_bg.wasm nếu bạn đã build sẵn.");
        return;
    }

    if let Err(e) = fs::create_dir_all(&dest_pkg_dir) {
        eprintln!("Không tạo được thư mục '{}': {}", dest_pkg_dir.display(), e);
        return;
    }

    for f in files {
        let src = src_dir.join(f);
        let dest = dest_pkg_dir.join(f);
        if let Err(e) = fs::copy(&src, &dest) {
            eprintln!(
                "Không copy được '{}' sang '{}': {}",
                src.display(),
                dest.display(),
                e
            );
        }
    }
}

/// Thư mục pkg/ mặc định: cạnh chính binary vibaoc đang chạy. Cách này
/// cho phép đóng gói vibaoc kèm sẵn pkg/ khi phân phối (vd cùng 1 thư
/// mục cài đặt), Dev không cần biết đường dẫn tuyệt đối trên máy họ.
fn default_pkg_dir() -> PathBuf {
    env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|p| p.join("pkg")))
        .unwrap_or_else(|| PathBuf::from("pkg"))
}
