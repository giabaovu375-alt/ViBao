// ============================================================
// VIBAO COMPILER (Rust) — codegen/control.rs
// Sinh HTML + JS binding cho các cấu trúc điều khiển: neu/khong_thi
// (If), chon (Switch), lap (Loop). Tương đương phần "6. CONTROL
// FLOW GENERATION" của 11-codegen-core.ts + compileIfCondition/
// compileLoopNode/ifToDataAttr/loopToDataAttr của 08-parser-control.ts.
// ============================================================

use vibao_ast::{IfNode, LoopKind, LoopNode, SwitchNode};
use crate::codegen::css::{esc_attr, indent, indent2};
use crate::codegen::element::ElementCodegenHost;
use crate::codegen::expr::expr_to_js_default;

/// Kết quả phân tích 1 IfNode — hiện chỉ `condition_js` được dùng thật
/// sự ở codegen, `analysis_kind`/`has_alternate` giữ lại để khớp API gốc
/// (compileIfCondition trả về object 3 field) và có thể dùng sau này cho
/// optimize runtime (vd biết trước "empty_check" để bind nhanh hơn).
pub struct CompiledIf {
    pub condition_js: String,
    pub analysis_kind: IfAnalysisKind,
    pub has_alternate: bool,
}

/// Tương đương chuỗi trả về của analyzeControlFlow() ở bản TS cũ — phân
/// loại điều kiện If để runtime có thể tối ưu (chưa dùng ở giai đoạn
/// codegen HTML/JS hiện tại, nhưng giữ lại để không mất thông tin so với
/// bản gốc).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum IfAnalysisKind {
    SimpleShow,
    EmptyCheck,
    NullCheck,
    Logical,
    Comparison,
}

fn analyze_if_condition(node: &IfNode) -> IfAnalysisKind {
    use vibao_ast::{BinaryOp, Expr, LiteralValue};
    match &node.condition {
        Expr::Variable(_, _) => IfAnalysisKind::SimpleShow,
        Expr::MemberAccess { property, .. } if property == "rong" || property == "do_dai" => {
            IfAnalysisKind::EmptyCheck
        }
        Expr::Binary { op, right, .. } => match op {
            BinaryOp::And | BinaryOp::Or => IfAnalysisKind::Logical,
            BinaryOp::Eq | BinaryOp::Neq => match right.as_ref() {
                Expr::Literal(LiteralValue::Str(s), _) if s == "null" => IfAnalysisKind::NullCheck,
                Expr::Literal(LiteralValue::Str(s), _) if s.is_empty() => IfAnalysisKind::EmptyCheck,
                _ => IfAnalysisKind::Comparison,
            },
            _ => IfAnalysisKind::Comparison,
        },
        _ => IfAnalysisKind::SimpleShow,
    }
}

/// Biên dịch điều kiện của 1 IfNode thành JS + phân tích loại. Tương
/// đương compileIfCondition().
pub fn compile_if_condition(node: &IfNode) -> CompiledIf {
    CompiledIf {
        condition_js: expr_to_js_default(&node.condition),
        analysis_kind: analyze_if_condition(node),
        has_alternate: node.alternate.is_some(),
    }
}

/// Sinh attribute `data-vb-if="..."` — tương đương ifToDataAttr().
pub fn if_to_data_attr(condition_js: &str) -> String {
    format!("data-vb-if=\"{}\"", esc_attr(condition_js))
}

/// Sinh HTML + đăng ký JS binding cho 1 IfNode hoàn chỉnh — tương đương
/// genIf() ở bản TS cũ (Codegen.genIf).
pub fn gen_if(node: &IfNode, host: &mut dyn ElementCodegenHost) -> String {
    let compiled = compile_if_condition(node);
    let consequent_html = host.gen_children(&node.consequent);
    let alternate_html = node.alternate.as_ref().map(|c| host.gen_children(c)).unwrap_or_default();

    let if_id = host.next_id("if");
    let else_id = host.next_id("else");

    let if_block = format!(
        "<div id=\"{}\" {}>\n{}\n</div>",
        if_id,
        if_to_data_attr(&compiled.condition_js),
        indent2(&consequent_html)
    );

    let else_block = if !alternate_html.is_empty() {
        format!(
            "<div id=\"{}\" data-vb-else data-vb-if-ref=\"{}\" style=\"display:none\">\n{}\n</div>",
            else_id, if_id, indent2(&alternate_html)
        )
    } else {
        String::new()
    };

    // else_id chỉ thực sự tồn tại (được runtime dùng) nếu else_block
    // không rỗng — bind cả 2 nhánh dù else_id "lãng phí" khi không có
    // alternate, khớp đúng hành vi bản TS cũ luôn gọi nextId("else") vô
    // điều kiện (kể cả không dùng), để giữ đúng số thứ tự id sinh ra.
    let else_id_for_binding = if !alternate_html.is_empty() { Some(else_id.as_str()) } else { None };
    host.add_js(&gen_if_binding(&if_id, &compiled.condition_js, else_id_for_binding));

    [if_block, else_block].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join("\n")
}

fn gen_if_binding(id: &str, cond_js: &str, else_id: Option<&str>) -> String {
    match else_id {
        Some(eid) => format!("__vb.bindIf('{}', () => {}, '{}');", id, cond_js, eid),
        None => format!("__vb.bindIf('{}', () => {});", id, cond_js),
    }
}

// ════════════════════════════════════════════════════════════
// SWITCH (chon / truong_hop)
// ════════════════════════════════════════════════════════════

/// Sinh HTML cho 1 SwitchNode — dùng expr registry (Rust thuần), KHÔNG
/// sinh JS. `data-vb-switch="<exprId>"` trên div gốc mang id của biểu
/// thức chủ đề (subject); mỗi case con mang
/// `data-vb-case="<exprId_case>"` (biểu thức giá trị case, để runtime tự
/// so sánh bằng expr_eval sau khi tính subject) hoặc `data-vb-default`
/// cho nhánh mặc định. Runtime (dom.rs::bind_switch, VIẾT MỚI cùng lúc
/// với thay đổi này) tự đọc các attribute này, tính subject 1 lần, rồi
/// so khớp lần lượt từng case, hiện đúng 1 nhánh khớp.
pub fn gen_switch(node: &SwitchNode, host: &mut dyn ElementCodegenHost) -> String {
    let subject_id = crate::codegen::expr::register_expr(node.subject.clone());
    let switch_id = host.next_id("switch");

    let mut html = format!("<div id=\"{}\" data-vb-switch=\"{}\">\n", switch_id, subject_id);

    for case in &node.cases {
        let value_id = crate::codegen::expr::register_expr(case.value.clone());
        let body_html = host.gen_children(&case.body);
        let case_id = host.next_id("case");
        html.push_str(&format!(
            "  <div id=\"{}\" data-vb-case=\"{}\" style=\"display:none\">\n{}\n  </div>\n",
            case_id,
            value_id,
            indent(&body_html, 4)
        ));
    }

    if let Some(default_case) = &node.default_case {
        let default_html = host.gen_children(default_case);
        let default_id = host.next_id("default");
        html.push_str(&format!(
            "  <div id=\"{}\" data-vb-default style=\"display:none\">\n{}\n  </div>\n",
            default_id,
            indent(&default_html, 4)
        ));
    }

    html.push_str("</div>");
    html
}

// ════════════════════════════════════════════════════════════
// LOOP (lap moi / lap tu)
// ════════════════════════════════════════════════════════════

/// Kết quả biên dịch 1 LoopNode — 2 biến thể tương ứng LoopKind::Each
/// (lặp qua mảng) và LoopKind::Range (lặp từ số → số). Tương đương union
/// return type của compileLoopNode() ở bản TS cũ.
pub enum CompiledLoop {
    Each {
        iterable_js: String,
        item_var: String,
        index_var: Option<String>,
        index_name: String,
    },
    Range {
        from: i64,
        to: i64,
        index_name: String,
    },
}

/// Biên dịch 1 LoopNode thành CompiledLoop. Tương đương compileLoopNode().
pub fn compile_loop_node(node: &LoopNode) -> CompiledLoop {
    match &node.kind {
        LoopKind::Each { iterable, item_var, index_var } => CompiledLoop::Each {
            iterable_js: expr_to_js_default(iterable),
            // Bản TS cũ strip ký tự "$" khỏi tên biến ($item → item) —
            // AST Rust của ta lưu item_var đã KHÔNG có "$" ngay từ lúc
            // parse (xem parser/control.rs), nên .replace("$","") ở đây
            // là dư thừa nhưng vẫn giữ lại phòng khi có input còn sót "$".
            item_var: item_var.replace('$', ""),
            index_var: index_var.as_ref().map(|v| v.replace('$', "")),
            index_name: index_var.as_ref().map(|v| v.replace('$', "")).unwrap_or_else(|| "i".to_string()),
        },
        LoopKind::Range { from, to } => CompiledLoop::Range {
            from: *from,
            to: *to,
            index_name: "i".to_string(),
        },
    }
}

/// Sinh attribute data-vb-for/-in/-index hoặc data-vb-range-* tuỳ loại
/// loop. Tương đương loopToDataAttr().
pub fn loop_to_data_attr(compiled: &CompiledLoop) -> String {
    match compiled {
        CompiledLoop::Each { iterable_js, item_var, index_var, .. } => {
            let mut attrs = vec![
                format!("data-vb-for=\"{}\"", item_var),
                format!("data-vb-in=\"{}\"", esc_attr(iterable_js)),
            ];
            if let Some(iv) = index_var {
                attrs.push(format!("data-vb-index=\"{}\"", iv));
            }
            attrs.join(" ")
        }
        CompiledLoop::Range { from, to, index_name } => {
            format!(
                "data-vb-range-from=\"{}\" data-vb-range-to=\"{}\" data-vb-range-var=\"{}\"",
                from, to, index_name
            )
        }
    }
}

/// Sinh HTML (template + container) + JS binding cho 1 LoopNode hoàn
/// chỉnh. Tương đương genLoop().
pub fn gen_loop(node: &LoopNode, host: &mut dyn ElementCodegenHost) -> String {
    let compiled = compile_loop_node(node);
    let loop_id = host.next_id("loop");
    let template_id = host.next_id("tpl");
    let body_html = host.gen_children(&node.body);

    let template = format!("<template id=\"{}\">\n{}\n</template>", template_id, indent2(&body_html));
    let container = format!(
        "<div id=\"{}\" {} data-vb-template=\"{}\"></div>",
        loop_id,
        loop_to_data_attr(&compiled),
        template_id
    );

    host.add_js(&gen_loop_binding(&loop_id, &template_id, &compiled));
    format!("{}\n{}", template, container)
}

fn gen_loop_binding(container_id: &str, template_id: &str, compiled: &CompiledLoop) -> String {
    match compiled {
        CompiledLoop::Each { iterable_js, item_var, .. } => format!(
            "__vb.bindLoop('{}', '{}', () => {}, '{}');",
            container_id, template_id, iterable_js, item_var
        ),
        CompiledLoop::Range { from, to, index_name } => format!(
            "__vb.bindRange('{}', '{}', {}, {}, '{}');",
            container_id, template_id, from, to, index_name
        ),
    }
}

// ════════════════════════════════════════════════════════════
// UNIT TESTS
// ════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use vibao_ast::{BinaryOp, Expr, LiteralValue, Pos};

    fn p() -> Pos {
        Pos { line: 1, column: 1 }
    }

    #[test]
    fn test_analyze_if_simple_show() {
        let node = IfNode {
            condition: Expr::Variable("dang_hien".to_string(), p()),
            consequent: vec![],
            alternate: None,
            pos: p(),
        };
        assert_eq!(analyze_if_condition(&node), IfAnalysisKind::SimpleShow);
    }

    #[test]
    fn test_analyze_if_empty_check_via_member_access() {
        let node = IfNode {
            condition: vibao_ast::Expr::MemberAccess {
                object: Box::new(Expr::Variable("ds".to_string(), p())),
                property: "rong".to_string(),
                pos: p(),
            },
            consequent: vec![],
            alternate: None,
            pos: p(),
        };
        assert_eq!(analyze_if_condition(&node), IfAnalysisKind::EmptyCheck);
    }

    #[test]
    fn test_analyze_if_logical() {
        let node = IfNode {
            condition: Expr::Binary {
                op: BinaryOp::And,
                left: Box::new(Expr::Variable("a".to_string(), p())),
                right: Box::new(Expr::Variable("b".to_string(), p())),
                pos: p(),
            },
            consequent: vec![],
            alternate: None,
            pos: p(),
        };
        assert_eq!(analyze_if_condition(&node), IfAnalysisKind::Logical);
    }

    #[test]
    fn test_analyze_if_null_check() {
        let node = IfNode {
            condition: Expr::Binary {
                op: BinaryOp::Eq,
                left: Box::new(Expr::Variable("x".to_string(), p())),
                right: Box::new(Expr::Literal(LiteralValue::Str("null".to_string()), p())),
                pos: p(),
            },
            consequent: vec![],
            alternate: None,
            pos: p(),
        };
        assert_eq!(analyze_if_condition(&node), IfAnalysisKind::NullCheck);
    }

    #[test]
    fn test_if_to_data_attr_escapes_quotes() {
        let attr = if_to_data_attr("__s.x === \"y\"");
        assert!(attr.starts_with("data-vb-if=\""));
        assert!(attr.contains("&quot;"));
    }

    #[test]
    fn test_compile_loop_each_strips_dollar_sign() {
        let node = LoopNode {
            kind: LoopKind::Each {
                iterable: Expr::Variable("ds".to_string(), p()),
                item_var: "item".to_string(),
                index_var: None,
            },
            body: vec![],
            pos: p(),
        };
        match compile_loop_node(&node) {
            CompiledLoop::Each { item_var, index_name, .. } => {
                assert_eq!(item_var, "item");
                assert_eq!(index_name, "i");
            }
            _ => panic!("Phải là Each"),
        }
    }

    #[test]
    fn test_compile_loop_range() {
        let node = LoopNode {
            kind: LoopKind::Range { from: 1, to: 5 },
            body: vec![],
            pos: p(),
        };
        match compile_loop_node(&node) {
            CompiledLoop::Range { from, to, index_name } => {
                assert_eq!(from, 1);
                assert_eq!(to, 5);
                assert_eq!(index_name, "i");
            }
            _ => panic!("Phải là Range"),
        }
    }

    #[test]
    fn test_loop_to_data_attr_each() {
        let compiled = CompiledLoop::Each {
            iterable_js: "__s.ds".to_string(),
            item_var: "item".to_string(),
            index_var: Some("idx".to_string()),
            index_name: "idx".to_string(),
        };
        let attr = loop_to_data_attr(&compiled);
        assert!(attr.contains("data-vb-for=\"item\""));
        assert!(attr.contains("data-vb-in=\"__s.ds\""));
        assert!(attr.contains("data-vb-index=\"idx\""));
    }

    #[test]
    fn test_loop_to_data_attr_range() {
        let compiled = CompiledLoop::Range { from: 1, to: 10, index_name: "i".to_string() };
        let attr = loop_to_data_attr(&compiled);
        assert_eq!(attr, "data-vb-range-from=\"1\" data-vb-range-to=\"10\" data-vb-range-var=\"i\"");
    }
}
