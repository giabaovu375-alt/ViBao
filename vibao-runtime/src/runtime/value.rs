// ============================================================
// VIBAO RUNTIME (Rust/WASM) — runtime/value.rs
// VbValue: kiểu giá trị động dùng trong toàn bộ runtime, tương
// đương "bất kỳ giá trị JS nào" (string/number/bool/null/array/
// object) mà __state từng chứa ở bản JS cũ.
//
// Dùng chung 1 kiểu duy nhất (thay vì generic <T>) vì runtime cần
// lưu state không đồng nhất kiểu trong cùng 1 store (giống JS),
// và cần serialize/deserialize dễ dàng qua wasm-bindgen <-> JsValue
// khi cần hiển thị debug hoặc tương tác với code JS còn sót lại
// (vd: JSON.stringify ở __inspectState của bản cũ).
// ============================================================

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;

use wasm_bindgen::JsValue;

/// Giá trị động runtime — tương đương "any" phía JS.
///
/// Dùng `BTreeMap` cho Object thay vì `HashMap` để giữ thứ tự lặp qua
/// ổn định (quan trọng khi serialize ra JSON để debug hoặc gửi lên API),
/// tương tự lý do PropsMap ở ast.rs dùng Vec thay vì HashMap.
#[derive(Debug, Clone, PartialEq)]
pub enum VbValue {
    Null,
    Bool(bool),
    /// Số luôn lưu dạng f64 — khớp với JS Number (không phân biệt int/float),
    /// tránh phải đồng bộ 2 loại số riêng biệt xuyên suốt runtime.
    Num(f64),
    Str(String),
    Array(Vec<VbValue>),
    Object(BTreeMap<String, VbValue>),
}

impl Default for VbValue {
    fn default() -> Self {
        VbValue::Null
    }
}

impl fmt::Display for VbValue {
    /// Tương đương `String(value)` phía JS — dùng cho bindText/toast/...
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            VbValue::Null => write!(f, ""),
            VbValue::Bool(b) => write!(f, "{}", b),
            VbValue::Num(n) => {
                // JS in số nguyên không có ".0" (vd 16 chứ không phải 16.0).
                if n.fract() == 0.0 && n.is_finite() {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{}", n)
                }
            }
            VbValue::Str(s) => write!(f, "{}", s),
            VbValue::Array(_) | VbValue::Object(_) => {
                write!(f, "{}", self.to_json_string())
            }
        }
    }
}

impl VbValue {
    // ── Constructors tiện dụng ─────────────────────────────────────────

    pub fn str(s: impl Into<String>) -> Self {
        VbValue::Str(s.into())
    }

    pub fn num(n: f64) -> Self {
        VbValue::Num(n)
    }

    pub fn bool(b: bool) -> Self {
        VbValue::Bool(b)
    }

    pub fn array(items: Vec<VbValue>) -> Self {
        VbValue::Array(items)
    }

    pub fn object(entries: Vec<(String, VbValue)>) -> Self {
        VbValue::Object(entries.into_iter().collect())
    }

    // ── Truy vấn kiểu ────────────────────────────────────────────────

    pub fn is_null(&self) -> bool {
        matches!(self, VbValue::Null)
    }

    pub fn is_array(&self) -> bool {
        matches!(self, VbValue::Array(_))
    }

    pub fn as_array(&self) -> Option<&Vec<VbValue>> {
        match self {
            VbValue::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_array_mut(&mut self) -> Option<&mut Vec<VbValue>> {
        match self {
            VbValue::Array(a) => Some(a),
            _ => None,
        }
    }

    pub fn as_object(&self) -> Option<&BTreeMap<String, VbValue>> {
        match self {
            VbValue::Object(o) => Some(o),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            VbValue::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_num(&self) -> Option<f64> {
        match self {
            VbValue::Num(n) => Some(*n),
            _ => None,
        }
    }

    /// Tương đương `Number(value) || 0` phía JS — dùng ở bindProgress,
    /// lam_tron, phan_tram... nơi bản cũ luôn ép kiểu về số có fallback 0.
    pub fn to_num_or_zero(&self) -> f64 {
        match self {
            VbValue::Num(n) => *n,
            VbValue::Str(s) => s.trim().parse().unwrap_or(0.0),
            VbValue::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            _ => 0.0,
        }
    }

    /// Tương đương truthiness JS: `!!value`. Null/false/0/""/NaN → false.
    pub fn is_truthy(&self) -> bool {
        match self {
            VbValue::Null => false,
            VbValue::Bool(b) => *b,
            VbValue::Num(n) => *n != 0.0 && !n.is_nan(),
            VbValue::Str(s) => !s.is_empty(),
            VbValue::Array(_) => true,
            VbValue::Object(_) => true,
        }
    }

    /// `.rong` — field đặc biệt ViBao trên mảng/chuỗi (xem __resolveSpecialField).
    pub fn is_rong(&self) -> bool {
        match self {
            VbValue::Array(a) => a.is_empty(),
            VbValue::Str(s) => s.is_empty(),
            VbValue::Null => true,
            _ => false,
        }
    }

    /// `.do_dai` — field đặc biệt ViBao: độ dài mảng/chuỗi.
    pub fn do_dai(&self) -> f64 {
        match self {
            VbValue::Array(a) => a.len() as f64,
            VbValue::Str(s) => s.chars().count() as f64,
            _ => 0.0,
        }
    }

    /// Truy cập field lồng nhau theo path "a.b.c", tự xử lý field đặc biệt
    /// (.rong/.do_dai) và index số cho mảng (path segment là số).
    /// Tương đương __digPath / __get ở bản JS cũ.
    pub fn dig_path(&self, path: &str) -> VbValue {
        let mut cur = self.clone();
        for part in path.split('.') {
            if cur.is_null() {
                return VbValue::Null;
            }
            cur = cur.get_field(part);
        }
        cur
    }

    /// Lấy 1 field/index trực tiếp (không đệ quy qua path), xử lý field
    /// đặc biệt trước, rồi object field, rồi array index.
    pub fn get_field(&self, field: &str) -> VbValue {
        match field {
            "rong" => VbValue::Bool(self.is_rong()),
            "do_dai" => VbValue::Num(self.do_dai()),
            _ => match self {
                VbValue::Object(o) => o.get(field).cloned().unwrap_or(VbValue::Null),
                VbValue::Array(a) => field
                    .parse::<usize>()
                    .ok()
                    .and_then(|i| a.get(i))
                    .cloned()
                    .unwrap_or(VbValue::Null),
                _ => VbValue::Null,
            },
        }
    }

    /// So sánh bằng kiểu "===" JS (không coerce kiểu) — dùng ở bindSwitch
    /// (so khớp case value) và __setState (reference/value identity check
    /// đơn giản hoá thành value equality vì VbValue đã Clone theo giá trị).
    pub fn strict_eq(&self, other: &VbValue) -> bool {
        self == other
    }

    /// So sánh thứ tự cho sap_xep()/`<`/`>` — số so số, chuỗi so chuỗi
    /// theo thứ tự từ điển, khác kiểu thì coi bằng nhau (ổn định, không panic).
    pub fn partial_cmp_loose(&self, other: &VbValue) -> Ordering {
        match (self, other) {
            (VbValue::Num(a), VbValue::Num(b)) => {
                a.partial_cmp(b).unwrap_or(Ordering::Equal)
            }
            (VbValue::Str(a), VbValue::Str(b)) => a.cmp(b),
            _ => {
                let a = self.to_num_or_zero();
                let b = other.to_num_or_zero();
                a.partial_cmp(&b).unwrap_or(Ordering::Equal)
            }
        }
    }

    // ── JSON (debug / gửi API / bindSwitch data-vb-case parse) ─────────

    pub fn to_json_string(&self) -> String {
        match self {
            VbValue::Null => "null".to_string(),
            VbValue::Bool(b) => b.to_string(),
            VbValue::Num(n) => n.to_string(),
            VbValue::Str(s) => serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string()),
            VbValue::Array(items) => {
                let parts: Vec<String> = items.iter().map(|v| v.to_json_string()).collect();
                format!("[{}]", parts.join(","))
            }
            VbValue::Object(map) => {
                let parts: Vec<String> = map
                    .iter()
                    .map(|(k, v)| {
                        format!(
                            "{}:{}",
                            serde_json::to_string(k).unwrap_or_else(|_| "\"\"".to_string()),
                            v.to_json_string()
                        )
                    })
                    .collect();
                format!("{{{}}}", parts.join(","))
            }
        }
    }

    pub fn from_json_str(s: &str) -> VbValue {
        serde_json::from_str::<serde_json::Value>(s)
            .map(VbValue::from)
            .unwrap_or(VbValue::Str(s.to_string()))
    }

    // ── Cầu nối với JS (wasm-bindgen) ───────────────────────────────────
    // Dùng khi cần trả giá trị ra ngoài cho code JS còn lại (vd hiển thị
    // devtools console, hoặc input value 2-way binding qua HtmlInputElement).

    pub fn to_js_value(&self) -> JsValue {
        match self {
            VbValue::Null => JsValue::NULL,
            VbValue::Bool(b) => JsValue::from_bool(*b),
            VbValue::Num(n) => JsValue::from_f64(*n),
            VbValue::Str(s) => JsValue::from_str(s),
            VbValue::Array(_) | VbValue::Object(_) => {
                // Không có dependency serde-wasm-bindgen ở đây để giữ Cargo.toml
                // gọn — với Array/Object ta đi qua JSON string rồi parse bên
                // JS bằng JSON.parse nếu cần cấu trúc đầy đủ. Với phần lớn use
                // case runtime (text/attr/style/input binding) giá trị hiển
                // thị luôn là scalar nên nhánh này ít khi được gọi tới.
                JsValue::from_str(&self.to_json_string())
            }
        }
    }

    pub fn from_js_value(v: &JsValue) -> VbValue {
        if v.is_null() || v.is_undefined() {
            return VbValue::Null;
        }
        if let Some(b) = v.as_bool() {
            return VbValue::Bool(b);
        }
        if let Some(n) = v.as_f64() {
            return VbValue::Num(n);
        }
        if let Some(s) = v.as_string() {
            return VbValue::Str(s);
        }
        VbValue::Null
    }
}

impl From<serde_json::Value> for VbValue {
    fn from(v: serde_json::Value) -> Self {
        match v {
            serde_json::Value::Null => VbValue::Null,
            serde_json::Value::Bool(b) => VbValue::Bool(b),
            serde_json::Value::Number(n) => VbValue::Num(n.as_f64().unwrap_or(0.0)),
            serde_json::Value::String(s) => VbValue::Str(s),
            serde_json::Value::Array(items) => {
                VbValue::Array(items.into_iter().map(VbValue::from).collect())
            }
            serde_json::Value::Object(map) => {
                VbValue::Object(map.into_iter().map(|(k, v)| (k, VbValue::from(v))).collect())
            }
        }
    }
}

impl From<&str> for VbValue {
    fn from(s: &str) -> Self {
        VbValue::Str(s.to_string())
    }
}

impl From<String> for VbValue {
    fn from(s: String) -> Self {
        VbValue::Str(s)
    }
}

impl From<f64> for VbValue {
    fn from(n: f64) -> Self {
        VbValue::Num(n)
    }
}

impl From<bool> for VbValue {
    fn from(b: bool) -> Self {
        VbValue::Bool(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_integer_like_number() {
        assert_eq!(VbValue::Num(16.0).to_string(), "16");
        assert_eq!(VbValue::Num(16.5).to_string(), "16.5");
    }

    #[test]
    fn test_truthy_matches_js_semantics() {
        assert!(!VbValue::Null.is_truthy());
        assert!(!VbValue::Num(0.0).is_truthy());
        assert!(!VbValue::Str(String::new()).is_truthy());
        assert!(VbValue::Str("0".to_string()).is_truthy()); // "0" is truthy in JS
        assert!(VbValue::Array(vec![]).is_truthy()); // [] is truthy in JS
    }

    #[test]
    fn test_rong_and_do_dai() {
        let arr = VbValue::Array(vec![VbValue::Num(1.0), VbValue::Num(2.0)]);
        assert!(!arr.is_rong());
        assert_eq!(arr.do_dai(), 2.0);

        let empty = VbValue::Array(vec![]);
        assert!(empty.is_rong());
        assert_eq!(empty.do_dai(), 0.0);
    }

    #[test]
    fn test_dig_path_nested_object() {
        let mut inner = BTreeMap::new();
        inner.insert("ten".to_string(), VbValue::str("An"));
        let mut outer = BTreeMap::new();
        outer.insert("nguoi_dung".to_string(), VbValue::Object(inner));
        let root = VbValue::Object(outer);

        let result = root.dig_path("nguoi_dung.ten");
        assert_eq!(result.as_str(), Some("An"));
    }

    #[test]
    fn test_dig_path_special_field_through_path() {
        let arr = VbValue::Array(vec![VbValue::Num(1.0)]);
        let mut outer = BTreeMap::new();
        outer.insert("ds".to_string(), arr);
        let root = VbValue::Object(outer);

        let result = root.dig_path("ds.do_dai");
        assert_eq!(result.as_num(), Some(1.0));
    }

    #[test]
    fn test_json_roundtrip() {
        let v = VbValue::object(vec![
            ("a".to_string(), VbValue::num(1.0)),
            ("b".to_string(), VbValue::str("x")),
        ]);
        let json = v.to_json_string();
        let parsed = VbValue::from_json_str(&json);
        assert_eq!(parsed, v);
    }
}
