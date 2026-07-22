// ============================================================
// VIBAO RUNTIME (Rust/WASM) — runtime/api.rs
// Port phần lõi của 19-runtime-api.ts: gọi fetch thật qua web-sys,
// trả về kết quả chuẩn hoá để action.rs đọc __ok mà rẽ nhánh
// thanh_cong/that_bai. KHÔNG panic ra ngoài khi lỗi mạng/HTTP — luôn
// trả về ApiResult, để action block "that_bai { ... }" luôn chạy được.
//
// Chưa port: __auth (token tự động đính header) — để dành đợt sau, vì
// ViBao hiện chưa có cú pháp khai báo auth trong ngôn ngữ (guard(...)
// chưa thấy xuất hiện ở ast.rs), nên phần đó chưa có chỗ bám vào.
// ============================================================

use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, Response};

use crate::runtime::value::VbValue;

/// Kết quả 1 lời gọi API, chuẩn hoá bất kể thành công/thất bại —
/// tương đương { __ok, status, data, error } ở bản JS cũ.
pub struct ApiResult {
    pub ok: bool,
    pub status: u16,
    pub data: VbValue,
    pub error: Option<String>,
}

impl ApiResult {
    fn failure(status: u16, error: impl Into<String>) -> Self {
        ApiResult {
            ok: false,
            status,
            data: VbValue::Null,
            error: Some(error.into()),
        }
    }
}

/// Ghép endpoint với base URL, tương đương __api.resolveURL. Nếu
/// endpoint đã là URL đầy đủ (http/https), giữ nguyên.
pub fn resolve_url(base_url: &str, endpoint: &str) -> String {
    if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        return endpoint.to_string();
    }
    let base = base_url.trim_end_matches('/');
    let path = endpoint.trim_start_matches('/');
    if base.is_empty() {
        format!("/{}", path)
    } else {
        format!("{}/{}", base, path)
    }
}

/// Thực hiện 1 lời gọi API thật qua `fetch` (web-sys). Tương đương
/// __api.call(method, endpoint, data). `data` (nếu có) được serialize
/// JSON và gửi làm request body — bỏ qua với method GET/DELETE giống
/// hành vi bản JS cũ (2 method này thường không có body theo chuẩn REST).
pub async fn call(base_url: &str, method: &str, endpoint: &str, data: Option<&VbValue>) -> ApiResult {
    let url = resolve_url(base_url, endpoint);
    let method_upper = method.to_uppercase();

    let opts = RequestInit::new();
    opts.set_method(&method_upper);

    if let Some(body_value) = data {
        if method_upper != "GET" && method_upper != "DELETE" {
            let body_json = body_value.to_json_string();
            opts.set_body(&JsValue::from_str(&body_json));
        }
    }

    let request = match Request::new_with_str_and_init(&url, &opts) {
        Ok(r) => r,
        Err(_) => return ApiResult::failure(0, "Không tạo được request"),
    };

    if request.headers().set("Content-Type", "application/json").is_err() {
        // Không chặn tiếp tục nếu set header lỗi — vẫn thử gọi request,
        // giống tinh thần "không throw ra ngoài" của bản JS cũ.
    }

    let window = match web_sys::window() {
        Some(w) => w,
        None => return ApiResult::failure(0, "Không có window (ngoài trình duyệt?)"),
    };

    let resp_value = match JsFuture::from(window.fetch_with_request(&request)).await {
        Ok(v) => v,
        Err(_) => {
            return ApiResult::failure(0, "Không thể kết nối tới máy chủ");
        }
    };

    let response: Response = match resp_value.dyn_into() {
        Ok(r) => r,
        Err(_) => return ApiResult::failure(0, "Phản hồi không hợp lệ"),
    };

    let status = response.status();
    let ok = response.ok();

    // Đọc body dưới dạng text rồi tự parse JSON — cách này đơn giản và
    // an toàn hơn gọi .json() của Response (vốn throw nếu body không
    // phải JSON hợp lệ); ở đây ta tự quyết định fallback về Str thô.
    let text_promise = match response.text() {
        Ok(p) => p,
        Err(_) => return ApiResult::failure(status, "Không đọc được nội dung phản hồi"),
    };
    let text_value = match JsFuture::from(text_promise).await {
        Ok(v) => v,
        Err(_) => return ApiResult::failure(status, "Không đọc được nội dung phản hồi"),
    };
    let text = text_value.as_string().unwrap_or_default();

    let data = if text.is_empty() {
        VbValue::Null
    } else {
        VbValue::from_json_str(&text)
    };

    if !ok {
        let error_msg = data
            .as_object()
            .and_then(|o| o.get("message"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("HTTP {}", status));
        return ApiResult {
            ok: false,
            status,
            data,
            error: Some(error_msg),
        };
    }

    ApiResult {
        ok: true,
        status,
        data,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_url_relative_path() {
        assert_eq!(resolve_url("https://api.vibao.dev", "/users"), "https://api.vibao.dev/users");
    }

    #[test]
    fn test_resolve_url_no_leading_slash() {
        assert_eq!(resolve_url("https://api.vibao.dev", "users"), "https://api.vibao.dev/users");
    }

    #[test]
    fn test_resolve_url_absolute_endpoint_kept_as_is() {
        assert_eq!(
            resolve_url("https://api.vibao.dev", "https://other.com/x"),
            "https://other.com/x"
        );
    }

    #[test]
    fn test_resolve_url_empty_base() {
        assert_eq!(resolve_url("", "/users"), "/users");
    }
}
