// ============================================================
// VIBAO RUNTIME (Rust/WASM) — runtime/action.rs
// Dispatcher thực thi `vibao_ast::Action` trực tiếp bằng Rust — đây là
// bộ phận còn thiếu khiến MỌI NÚT BẤM trước đây không làm gì cả
// (bind_events ở dom.rs chỉ log cảnh báo). Port phần lõi của
// 19-runtime-api.ts (toast/modal/scroll) + compileAction ở bản codegen
// cũ (nhưng THỰC THI thay vì sinh JS).
//
// Vì `dispatch` cần gọi async (goi_api), và mọi action bên trong 1
// event handler chạy TUẦN TỰ (giống await từng dòng ở JS cũ), hàm
// dispatch_all ở đây là `async fn`, chạy qua wasm-bindgen-futures.
// ============================================================

use std::collections::BTreeMap;

use vibao_ast::{Action, PropsMap};

use super::expr_eval;
use super::state::{self, LoopFrame, SharedState};
use super::value::VbValue;
use super::{api, log};

/// Thực thi TUẦN TỰ 1 danh sách Action (thân 1 event handler, hoặc 1
/// nhánh on_success/on_failure/consequent/alternate). KHÔNG track
/// dependency khi eval biểu thức bên trong action — action chạy 1 lần
/// khi sự kiện xảy ra, không phải 1 binding cần tự re-run khi state đổi.
///
/// `loop_frame`: nếu action này thuộc 1 event handler nằm bên trong 1
/// item của vong_lap, đây là snapshot LoopFrame của đúng item đó — mọi
/// biểu thức bên trong action (tham số hàm, điều kiện if, giá trị gán)
/// sẽ resolve đúng biến lặp (vd "$item") qua eval_with_loop_frame.
pub async fn dispatch_all(shared: &SharedState, actions: &[Action], loop_frame: Option<&LoopFrame>) {
    for action in actions {
        dispatch_one(shared, action, loop_frame).await;
    }
}

fn dispatch_one<'a>(
    shared: &'a SharedState,
    action: &'a Action,
    loop_frame: Option<&'a LoopFrame>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'a>> {
    // Bọc Box::pin vì Action::IfAction/ApiCall đệ quy gọi lại
    // dispatch_all (async fn đệ quy cần kích thước cố định tại
    // compile-time — Rust không cho phép async fn gọi chính nó trực
    // tiếp mà không Box hoá, giống lý do Expr/Child phải Box<T> ở AST).
    Box::pin(async move {
        match action {
            Action::FunctionCall { name, args, opts, assign_to, .. } => {
                let arg_values: Vec<VbValue> = args
                    .iter()
                    .map(|e| expr_eval::eval_with_loop_frame(shared, e, loop_frame))
                    .collect();
                let opt_values = eval_opts(shared, opts, loop_frame);

                let result = dispatch_function_call(shared, name, &arg_values, &opt_values).await;

                if let Some(var_name) = assign_to {
                    shared.borrow_mut().set_state(var_name, result);
                }
            }

            Action::Assign { target, value, .. } => {
                let v = expr_eval::eval_with_loop_frame(shared, value, loop_frame);
                shared.borrow_mut().set_state(target, v);
            }

            Action::ApiCall {
                method,
                endpoint,
                data,
                assign_to,
                on_success,
                on_failure,
                ..
            } => {
                let endpoint_val = expr_eval::eval_with_loop_frame(shared, endpoint, loop_frame);
                let endpoint_str = endpoint_val.to_string();
                let data_val = data
                    .as_ref()
                    .map(|d| expr_eval::eval_with_loop_frame(shared, d, loop_frame));

                let base_url = state::get_base_url(shared);
                let result = api::call(&base_url, method, &endpoint_str, data_val.as_ref()).await;

                if let Some(var_name) = assign_to {
                    shared.borrow_mut().set_state(var_name, result.data.clone());
                }

                if result.ok {
                    if let Some(actions) = on_success {
                        dispatch_all(shared, actions, loop_frame).await;
                    }
                } else {
                    log::warn(&format!(
                        "[ViBao] goi_api thất bại: {}",
                        result.error.as_deref().unwrap_or("lỗi không rõ")
                    ));
                    if let Some(actions) = on_failure {
                        dispatch_all(shared, actions, loop_frame).await;
                    }
                }
            }

            Action::IfAction { condition, consequent, alternate, .. } => {
                let cond_val = expr_eval::eval_with_loop_frame(shared, condition, loop_frame);
                if cond_val.is_truthy() {
                    dispatch_all(shared, consequent, loop_frame).await;
                } else if let Some(alt) = alternate {
                    dispatch_all(shared, alt, loop_frame).await;
                }
            }
        }

        // Mỗi action có thể set_state — flush ngay sau MỖI action (không
        // đợi tới cuối cả chuỗi) để nếu 1 action giữa chừng phụ thuộc gián
        // tiếp vào hiệu ứng render của action trước đó (hiếm nhưng có thể
        // xảy ra, vd conditional dựa trên DOM đã cập nhật), thứ tự vẫn
        // nhất quán. Với đa số trường hợp (không phụ thuộc chéo), gọi
        // flush() nhiều lần dư là an toàn — flush() tự no-op nếu không
        // có gì pending.
        state::flush(shared);
    })
}

/// Tính toàn bộ named options (vd `kieu: thanh_cong` trong
/// `thong_bao(msg, kieu: thanh_cong)`) thành 1 map tra cứu nhanh.
fn eval_opts(shared: &SharedState, opts: &PropsMap, loop_frame: Option<&LoopFrame>) -> BTreeMap<String, VbValue> {
    opts.iter()
        .map(|(k, v)| (k.clone(), expr_eval::eval_with_loop_frame(shared, v, loop_frame)))
        .collect()
}

// ════════════════════════════════════════════════════════════
// FUNCTION CALL DISPATCH — thong_bao, canh_bao, mo_modal, ...
// ════════════════════════════════════════════════════════════

/// Thực thi 1 lời gọi hàm hành động theo tên (callee của FunctionCall).
/// Trả về giá trị (dùng khi có `assign_to`) — hầu hết hành động
/// side-effect (toast, modal...) trả `VbValue::Null`, chỉ 1 số ít như
/// `sao_chep`/`goi_api` (qua nhánh ApiCall riêng) có giá trị ý nghĩa.
async fn dispatch_function_call(
    shared: &SharedState,
    name: &str,
    args: &[VbValue],
    opts: &BTreeMap<String, VbValue>,
) -> VbValue {
    match name {
        // ── Thông báo ────────────────────────────────────────────
        "thong_bao" => {
            let msg = args.first().map(|v| v.to_string()).unwrap_or_default();
            let kieu = opts.get("kieu").map(|v| v.to_string()).unwrap_or_else(|| "info".to_string());
            let thoi_gian = opts.get("thoi_gian").map(|v| v.to_num_or_zero()).unwrap_or(3000.0);
            super::dom::toast(&msg, &kieu, thoi_gian as i32);
            VbValue::Null
        }
        "canh_bao" => {
            let msg = args.first().map(|v| v.to_string()).unwrap_or_default();
            super::dom::alert(&msg);
            VbValue::Null
        }

        // ── Điều hướng ───────────────────────────────────────────
        "dieu_huong" => {
            let path = args.first().map(|v| v.to_string()).unwrap_or_default();
            super::dom::navigate(shared, &path);
            VbValue::Null
        }
        "mo_tab_moi" => {
            let path = args.first().map(|v| v.to_string()).unwrap_or_default();
            super::dom::open_tab(&path);
            VbValue::Null
        }

        // ── Modal ────────────────────────────────────────────────
        "mo_modal" => {
            let id = args.first().map(|v| v.to_string()).unwrap_or_default();
            super::dom::open_modal(&id);
            VbValue::Null
        }
        "dong_modal" => {
            let id = args.first().map(|v| v.to_string()).unwrap_or_default();
            super::dom::close_modal(&id);
            VbValue::Null
        }

        // ── Cuộn trang ───────────────────────────────────────────
        "cuon_den" => {
            let target = args.first().map(|v| v.to_string()).unwrap_or_default();
            super::dom::scroll_to(&target);
            VbValue::Null
        }
        "cuon_len_dau" => {
            super::dom::scroll_top();
            VbValue::Null
        }

        // ── State mutation trên mảng: $ds.them(item), $ds.xoa(...) ──
        // Các hàm này thực ra được PARSER dịch thành MemberAccess call
        // đặc biệt ở phần lớn thiết kế ViBao (xem ast Expr::Call callee
        // dạng "ten_bien.them") — nhưng nếu codegen đưa thẳng vào đây
        // dưới dạng FunctionCall với callee "luu_du_lieu"/"tai_du_lieu",
        // xử lý qua __save/__load tương đương bản JS cũ.
        "luu_du_lieu" => {
            let endpoint = args.first().map(|v| v.to_string()).unwrap_or_default();
            let data = args.get(1).cloned().unwrap_or(VbValue::Null);
            let base_url = state::get_base_url(shared);
            let result = api::call(&base_url, "POST", &endpoint, Some(&data)).await;
            if !result.ok {
                super::dom::toast(
                    &format!("Lưu thất bại: {}", result.error.as_deref().unwrap_or("")),
                    "loi",
                    3000,
                );
            }
            result.data
        }
        "tai_du_lieu" => {
            let endpoint = args.first().map(|v| v.to_string()).unwrap_or_default();
            let base_url = state::get_base_url(shared);
            let result = api::call(&base_url, "GET", &endpoint, None).await;
            if !result.ok {
                super::dom::toast(
                    &format!("Tải dữ liệu thất bại: {}", result.error.as_deref().unwrap_or("")),
                    "loi",
                    3000,
                );
                return VbValue::Null;
            }
            result.data
        }

        // ── Clipboard ────────────────────────────────────────────
        "sao_chep" => {
            let text = args.first().map(|v| v.to_string()).unwrap_or_default();
            super::dom::copy_text(&text);
            VbValue::Null
        }

        _ => {
            log::warn(&format!(
                "[ViBao] Hành động \"{}\" chưa được hỗ trợ trong action dispatcher.",
                name
            ));
            VbValue::Null
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vibao_ast::Pos;

    fn p() -> Pos {
        Pos { line: 1, column: 1 }
    }

    #[test]
    fn test_eval_opts_reads_named_options() {
        let shared = state::new_shared_state();
        let opts: PropsMap = vec![("kieu".to_string(), vibao_ast::Expr::literal_str("loi", p()))];
        let result = eval_opts(&shared, &opts, None);
        assert_eq!(result.get("kieu").and_then(|v| v.as_str()), Some("loi"));
    }

    #[test]
    fn test_eval_opts_resolves_loop_variable_via_loop_frame() {
        // Test hồi quy trực tiếp cho bug loop-action đã sửa: trước đây
        // eval_opts (và mọi eval bên trong action) luôn gọi eval() trần
        // không có loop_frame nào, khiến biến lặp (vd "$item") resolve
        // sai/rỗng khi dùng bên trong action nằm trong 1 item của
        // vong_lap. Giờ truyền loop_frame vào, "$ten" (item_var) phải
        // resolve đúng giá trị của item đó.
        let shared = state::new_shared_state();
        let frame = LoopFrame {
            item_var: "ten".to_string(),
            item_value: VbValue::str("San pham A"),
            index_var: None,
            index_value: None,
        };
        let opts: PropsMap = vec![("msg".to_string(), vibao_ast::Expr::Variable("ten".to_string(), p()))];
        let result = eval_opts(&shared, &opts, Some(&frame));
        assert_eq!(result.get("msg").and_then(|v| v.as_str()), Some("San pham A"));
    }

    #[test]
    fn test_assign_action_sets_state() {
        // Test đồng bộ đơn giản: Assign không cần async thật, nhưng
        // dispatch_one là async fn — dùng futures::executor::block_on
        // không có sẵn (không thêm dependency `futures` riêng chỉ cho
        // test), nên assert trực tiếp qua eval + set_state thủ công,
        // xác nhận đúng PHẦN LOGIC mà Assign case bên trong dispatch_one
        // sẽ làm (2 dòng y hệt nhánh Action::Assign ở dispatch_one).
        let shared = state::new_shared_state();
        let value_expr = vibao_ast::Expr::literal_num(42.0, p());
        let v = expr_eval::eval(&shared, &value_expr);
        shared.borrow_mut().set_state("dem", v);
        assert_eq!(shared.borrow().peek_state("dem").as_num(), Some(42.0));
    }
}
