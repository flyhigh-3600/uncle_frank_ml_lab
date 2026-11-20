//! Native C emitters for the text-terminal slice and a narrow ggml matrix subset.
//!
//! This intentionally supports only direct input-to-output text pipes. It proves
//! the end-to-end artifact path; typed IR and ggml partitions replace this small
//! emitter in E3/E6.

use std::{collections::BTreeMap, fs, path::Path, process::Command};
use ufp_syntax::{AstPlan, Expr, ExprKind, Item, Plan, SyntaxKind, SyntaxNode, TypeAst};

pub fn emit_text_c(plan: &Plan) -> Result<String, String> {
    let mut inputs = BTreeMap::new();
    let mut outputs = Vec::new();
    let mut pipes = Vec::new();
    for item in &plan.items {
        match item {
            Item::Input { name, value, .. } => {
                inputs.insert(name, value);
            }
            Item::Output { name, .. } => outputs.push(name),
            Item::Pipe { from, to, .. } => pipes.push((from, to)),
            Item::Text { .. } | Item::Portal { .. } => {}
            Item::Other { keyword, .. } => {
                return Err(format!("native M0 emitter does not support `{keyword}`"))
            }
        }
    }
    let mut c = String::from("#include <stdio.h>\nint main(void) {\n");
    for output in outputs {
        let links: Vec<_> = pipes
            .iter()
            .filter(|(_, to)| to.node == *output && to.port == "text")
            .collect();
        if links.len() != 1 {
            return Err(format!("output `{output}` must have one text pipe"));
        }
        let (from, _) = links[0];
        let Some(text) = inputs.get(&from.node) else {
            return Err(format!(
                "native M0 emitter only supports input source `{}`",
                from.node
            ));
        };
        c.push_str(&format!(
            "  puts(\"{}: {}\");\n",
            escape(output),
            escape(text)
        ));
    }
    c.push_str("  return 0;\n}\n");
    Ok(c)
}

pub fn compile_text_plan(plan: &Plan, output: &Path) -> Result<(), String> {
    let source = emit_text_c(plan)?;
    let c_path = output.with_extension("c");
    fs::write(&c_path, source).map_err(|e| e.to_string())?;
    let status = Command::new("cc")
        .arg("-O2")
        .arg(&c_path)
        .arg("-o")
        .arg(output)
        .status()
        .map_err(|e| format!("could not invoke cc: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("cc failed with {status}"))
    }
}

/// Emit a standalone ggml C program proving one f32 matrix multiplication.
///
/// This is intentionally a fixed smoke fixture, not a compiler for UFP plans.
pub fn emit_ggml_matmul_c() -> &'static str {
    include_str!("../fixtures/ggml_matmul.c")
}

/// Lower the supported initialized-f32-matrix UFP subset to native ggml C.
///
/// It supports matrix literals and a single zero-argument procedure containing
/// ordered `let` bindings over identifiers and `+`, `-`, `.*`, `*`, followed
/// by a return. Matrix dimensions and literals use conventional row-major order.
pub fn emit_ggml_matmul_ufp(source: &str) -> Result<String, String> {
    let ast = ufp_syntax::parse_ast(source).map_err(|errors| {
        errors
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join("\n")
    })?;
    emit_matmul_ast(&ast)
}

pub fn compile_ggml_matmul_ufp(
    source: &str,
    output: &Path,
    ggml_source: &Path,
    ggml_build: &Path,
) -> Result<(), String> {
    let c = emit_ggml_matmul_ufp(source)?;
    let c_path = output.with_extension("c");
    fs::write(&c_path, c).map_err(|e| e.to_string())?;
    let status = Command::new("c++")
        .arg("-O2")
        .arg(&c_path)
        .arg(format!("-I{}", ggml_source.join("include").display()))
        .arg(format!("-L{}", ggml_build.join("src").display()))
        .arg(format!("-L{}", ggml_build.join("src/ggml-cpu").display()))
        .arg(format!("-Wl,-rpath,{}", ggml_build.join("src").display()))
        .arg(format!(
            "-Wl,-rpath,{}",
            ggml_build.join("src/ggml-cpu").display()
        ))
        .args(["-lggml", "-lggml-cpu", "-lggml-base", "-lm", "-o"])
        .arg(output)
        .status()
        .map_err(|e| format!("could not invoke c++: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("c++ failed with {status}"))
    }
}

#[derive(Debug)]
struct Matrix {
    name: String,
    rows: usize,
    cols: usize,
    values: Vec<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Shape {
    rows: usize,
    cols: usize,
}

#[derive(Clone, Copy)]
struct Value {
    slot: usize,
    shape: Shape,
}

fn emit_matmul_ast(ast: &AstPlan) -> Result<String, String> {
    let declarations: Vec<_> = ast
        .declarations
        .iter()
        .filter(|n| n.kind == SyntaxKind::Matrix)
        .collect();
    let procedures: Vec<_> = ast
        .declarations
        .iter()
        .filter(|n| n.kind == SyntaxKind::Procedure)
        .collect();
    if declarations.is_empty()
        || procedures.len() != 1
        || ast.declarations.len() != declarations.len() + 1
    {
        return Err(
            "ggml UFP slice requires one or more matrix declarations and one procedure".into(),
        );
    }
    let matrices = declarations
        .iter()
        .map(|node| parse_matrix(node))
        .collect::<Result<Vec<_>, _>>()?;
    let mut names = BTreeMap::new();
    for (index, matrix) in matrices.iter().enumerate() {
        if names.insert(matrix.name.as_str(), index).is_some() {
            return Err(format!("duplicate matrix declaration `{}`", matrix.name));
        }
    }
    if ast.declarations.iter().any(|n| {
        (n.kind == SyntaxKind::Matrix || n.kind == SyntaxKind::Procedure)
            && n.children
                .iter()
                .any(|child| child.kind == SyntaxKind::Annotation)
    }) {
        return Err("annotations are not supported by ggml UFP codegen".into());
    }
    let (body, output, label) = parse_procedure(procedures[0], &matrices)?;
    let mut c = String::from(
        "#include <stdio.h>\n#include <stddef.h>\n#include <ggml.h>\n#include <ggml-backend.h>\n#include <ggml-alloc.h>\n#include <ggml-cpu.h>\n",
    );
    for (i, matrix) in matrices.iter().enumerate() {
        c.push_str(&format!(
            "static const float data_{i}[] = {{{}}};\n",
            c_values(&matrix.values)
        ));
    }
    c.push_str(&format!(
        "#ifdef __cplusplus\nextern \"C\" {{\n#endif\nint ufp_execute(float *output, size_t output_len) {{\n  if (!output || output_len != {}) return 1;\n  struct ggml_init_params p = {{.mem_size=64*1024*1024, .mem_buffer=NULL, .no_alloc=true}};\n  struct ggml_context *ctx=ggml_init(p); if(!ctx) return 1;\n",
        output.shape.rows * output.shape.cols
    ));
    for (i, matrix) in matrices.iter().enumerate() {
        c.push_str(&format!(
            "  struct ggml_tensor *t_{i}=ggml_new_tensor_2d(ctx,GGML_TYPE_F32,{},{});\n",
            matrix.cols, matrix.rows
        ));
    }
    c.push_str(&body);
    c.push_str(&format!(
        "  struct ggml_cgraph *g=ggml_new_graph(ctx); ggml_build_forward_expand(g,t_{});\n  ggml_backend_t be=ggml_backend_cpu_init();\n  if(!be || !ggml_backend_alloc_ctx_tensors(ctx,be)) {{ fprintf(stderr,\"ggml CPU allocation failed\\\\n\"); return 1; }}\n",
        output.slot
    ));
    for i in 0..matrices.len() {
        c.push_str(&format!(
            "  ggml_backend_tensor_set(t_{i},data_{i},0,sizeof(data_{i}));\n"
        ));
    }
    c.push_str(&format!(
        "  if(ggml_backend_graph_compute(be,g)!=GGML_STATUS_SUCCESS) {{ fprintf(stderr,\"ggml compute failed\\\\n\"); ggml_backend_free(be); ggml_free(ctx); return 1; }}\n  ggml_backend_tensor_get(t_{},output,0,output_len*sizeof(float));\n  ggml_backend_free(be); ggml_free(ctx); return 0;\n}}\n#ifdef __cplusplus\n}}\n#endif\n#ifndef UFP_NO_MAIN\nint main(void) {{\n  float result[{}]; if(ufp_execute(result,{})) return 1;\n  printf(\"{}: [\"); for(size_t r=0;r<{};++r) {{ if(r) printf(\", \"); printf(\"[\"); for(size_t col=0;col<{};++col) {{ if(col) printf(\", \"); printf(\"%.6g\",result[r*{}+col]); }} printf(\"]\"); }} puts(\"]\"); return 0;\n}}\n#endif\n",
        output.slot,
        output.shape.rows * output.shape.cols,
        output.shape.rows * output.shape.cols,
        label,
        output.shape.rows,
        output.shape.cols,
        output.shape.cols
    ));
    Ok(c)
}

fn parse_matrix(node: &SyntaxNode) -> Result<Matrix, String> {
    let TypeAst::Matrix {
        rank: Some(rank),
        dimensions,
        element: Some(element),
        ..
    } = node
        .type_ast
        .as_ref()
        .ok_or("matrix declaration has no type")?
    else {
        return Err("matrix declarations require rank, dimensions, and element type".into());
    };
    if rank != "2" || !is_f32(element) {
        return Err("only rank-2 `matrix<2>[rows,cols] of float<32>` is supported".into());
    }
    if dimensions.len() != 2 {
        return Err("matrix must have exactly two dimensions".into());
    }
    let rows = dimensions[0]
        .parse::<usize>()
        .map_err(|_| "matrix dimensions must be positive integer literals")?;
    let cols = dimensions[1]
        .parse::<usize>()
        .map_err(|_| "matrix dimensions must be positive integer literals")?;
    if rows == 0 || cols == 0 {
        return Err("matrix dimensions must be positive".into());
    }
    let name = node
        .children
        .iter()
        .find_map(|child| child.token.as_ref().map(|t| t.text.clone()))
        .ok_or("matrix declaration requires a name")?;
    let initializer = node
        .expression
        .as_ref()
        .ok_or_else(|| format!("matrix `{name}` requires an initializer"))?;
    let (values, literal_shape) = parse_matrix_values(initializer)?;
    if let Some((literal_rows, literal_cols)) = literal_shape {
        if literal_rows != rows || literal_cols != cols {
            return Err(format!("nested initializer for `{name}` is {literal_rows}x{literal_cols}, declared shape is {rows}x{cols}"));
        }
    }
    let count = rows
        .checked_mul(cols)
        .ok_or("matrix element count overflows")?;
    if values.len() != count {
        return Err(format!(
            "matrix `{name}` has {} values but shape requires {count}",
            values.len()
        ));
    }
    Ok(Matrix {
        name,
        rows,
        cols,
        values,
    })
}

fn parse_f32_literal(expr: &Expr) -> Result<f32, String> {
    match &expr.kind {
        ExprKind::Literal(text) => {
            let value = text
                .parse::<f32>()
                .map_err(|_| format!("expected finite numeric matrix literal, got `{text}`"))?;
            if !value.is_finite() {
                return Err(format!("matrix literal `{text}` is not finite"));
            }
            Ok(value)
        }
        _ => Err("matrix initializers support only numeric literals".into()),
    }
}

fn parse_matrix_values(expr: &Expr) -> Result<(Vec<f32>, Option<(usize, usize)>), String> {
    let children = match &expr.kind {
        ExprKind::Matrix(v) | ExprKind::List(v) => v,
        _ => {
            return Err(
                "matrix initializer must be a flat or nested list of numeric literals".into(),
            )
        }
    };
    if children.is_empty() {
        return Ok((Vec::new(), None));
    }
    if children
        .iter()
        .all(|e| matches!(e.kind, ExprKind::List(_) | ExprKind::Matrix(_)))
    {
        let mut flat = Vec::new();
        let mut row_width = None;
        for row in children {
            let values = match &row.kind {
                ExprKind::List(v) | ExprKind::Matrix(v) => v
                    .iter()
                    .map(parse_f32_literal)
                    .collect::<Result<Vec<_>, _>>()?,
                _ => unreachable!(),
            };
            if row_width
                .replace(values.len())
                .is_some_and(|width| width != values.len())
            {
                return Err("nested matrix initializer rows must have equal lengths".into());
            }
            flat.extend(values);
        }
        return Ok((flat, Some((children.len(), row_width.unwrap_or(0)))));
    }
    if children
        .iter()
        .any(|e| matches!(e.kind, ExprKind::List(_) | ExprKind::Matrix(_)))
    {
        return Err("matrix initializer cannot mix nested rows and scalar values".into());
    }
    Ok((
        children
            .iter()
            .map(parse_f32_literal)
            .collect::<Result<Vec<_>, _>>()?,
        None,
    ))
}

fn parse_procedure(
    node: &SyntaxNode,
    matrices: &[Matrix],
) -> Result<(String, Value, String), String> {
    if node.children.iter().any(|n| n.kind == SyntaxKind::Pin) {
        return Err("ggml UFP slice currently supports only zero-argument procedures".into());
    }
    let name = node
        .children
        .iter()
        .find_map(|n| n.token.as_ref().map(|t| t.text.clone()))
        .unwrap_or_default();
    if name != "main" {
        return Err("ggml UFP slice requires `procedure main()`".into());
    }
    let blocks: Vec<_> = node
        .children
        .iter()
        .filter(|n| n.kind == SyntaxKind::Block)
        .collect();
    if blocks.len() != 1 || blocks[0].children.len() < 2 {
        return Err(
            "procedure body must contain one or more `let` bindings and a final return".into(),
        );
    }
    let statements = &blocks[0].children;
    let return_node = statements.last().unwrap();
    if return_node.kind != SyntaxKind::Return {
        return Err("procedure must end with `return expression;`".into());
    }
    let mut env = BTreeMap::new();
    let mut next_slot = matrices.len();
    for (i, matrix) in matrices.iter().enumerate() {
        env.insert(
            matrix.name.clone(),
            Value {
                slot: i,
                shape: Shape {
                    rows: matrix.rows,
                    cols: matrix.cols,
                },
            },
        );
    }
    let mut code = String::new();
    for statement in &statements[..statements.len() - 1] {
        if statement.kind != SyntaxKind::Let {
            return Err("only sequential `let` bindings are supported in the procedure".into());
        }
        let name = statement
            .children
            .iter()
            .find_map(|n| n.token.as_ref().map(|t| t.text.clone()))
            .ok_or("let binding needs a name")?;
        if env.contains_key(&name) {
            return Err(format!(
                "duplicate or shadowed binding `{name}` is unsupported"
            ));
        }
        let expr = statement
            .expression
            .as_ref()
            .ok_or("let binding has no expression")?;
        let value = emit_expr(expr, &env, &mut next_slot, &mut code)?;
        if let Some(ty) = statement.type_ast.as_ref() {
            check_declared_shape(ty, value.shape)?;
        }
        env.insert(name, value);
    }
    let return_expr = return_node
        .expression
        .as_ref()
        .ok_or("return requires a matrix expression")?;
    let label = match &return_expr.kind {
        ExprKind::Identifier(name) => name.clone(),
        _ => "out".into(),
    };
    let result = emit_expr(return_expr, &env, &mut next_slot, &mut code)?;
    Ok((code, result, label))
}

fn check_declared_shape(ty: &TypeAst, shape: Shape) -> Result<(), String> {
    let TypeAst::Matrix {
        rank: Some(rank),
        dimensions,
        element: Some(element),
        ..
    } = ty
    else {
        return Err("typed let bindings must use `matrix<2>[rows,cols] of float<32>`".into());
    };
    if rank != "2" || !is_f32(element) {
        return Err("typed let bindings support only rank-2 float<32> matrices".into());
    }
    if dimensions.len() != 2
        || dimensions[0].parse::<usize>().ok() != Some(shape.rows)
        || dimensions[1].parse::<usize>().ok() != Some(shape.cols)
    {
        return Err("typed let dimensions do not match inferred expression shape".into());
    }
    Ok(())
}

fn is_f32(ty: &TypeAst) -> bool {
    matches!(ty, TypeAst::Sized { name, bits: Some(bits), format: None } if name == "float" && bits == "32")
}

fn emit_expr(
    expr: &Expr,
    env: &BTreeMap<String, Value>,
    next: &mut usize,
    code: &mut String,
) -> Result<Value, String> {
    match &expr.kind {
        ExprKind::Identifier(name) => env
            .get(name)
            .copied()
            .ok_or_else(|| format!("unknown matrix value `{name}`")),
        ExprKind::Binary { op, left, right } => {
            let lhs = emit_expr(left, env, next, code)?;
            let rhs = emit_expr(right, env, next, code)?;
            let (shape, operation) = match op.as_str() {
                "+" | "-" | ".*" => {
                    if lhs.shape != rhs.shape {
                        return Err(format!("operator `{op}` requires equal matrix shapes"));
                    }
                    let function = match op.as_str() {
                        "+" => "ggml_add",
                        "-" => "ggml_sub",
                        _ => "ggml_mul",
                    };
                    (
                        lhs.shape,
                        format!("{function}(ctx,t_{},t_{})", lhs.slot, rhs.slot),
                    )
                }
                "*" => {
                    if lhs.shape.cols != rhs.shape.rows {
                        return Err(format!(
                            "matmul dimension mismatch: {} columns but {} rows",
                            lhs.shape.cols, rhs.shape.rows
                        ));
                    }
                    let shape = Shape {
                        rows: lhs.shape.rows,
                        cols: rhs.shape.cols,
                    };
                    (shape, format!("ggml_cont(ctx,ggml_transpose(ctx,ggml_mul_mat(ctx,t_{},ggml_cont(ctx,ggml_transpose(ctx,t_{})))))", lhs.slot, rhs.slot))
                }
                _ => {
                    return Err(format!(
                        "operator `{op}` is unsupported by ggml UFP codegen"
                    ))
                }
            };
            let slot = *next;
            *next += 1;
            code.push_str(&format!("  struct ggml_tensor *t_{slot}={operation};\n"));
            Ok(Value { slot, shape })
        }
        _ => {
            Err("only matrix names and binary matrix operators are supported in expressions".into())
        }
    }
}

fn c_values(values: &[f32]) -> String {
    values
        .iter()
        .map(|v| format!("{v:.9}f"))
        .collect::<Vec<_>>()
        .join(",")
}

/* Keep row-major matrix literal helpers local to the codegen boundary. */

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use ufp_syntax::parse_plan;
    #[test]
    fn emits_c() {
        let p = parse_plan("input x=\"hi\"; output y; pipe x.text -> y.text;").unwrap();
        assert!(emit_text_c(&p).unwrap().contains("y: hi"));
    }

    #[test]
    fn ggml_fixture_uses_native_matmul_and_checks_values() {
        let source = emit_ggml_matmul_c();
        assert!(source.contains("ggml_mul_mat"));
        assert!(source.contains("expected"));
        assert!(source.contains("ggml_backend_graph_compute"));
    }

    #[test]
    fn lowers_ufp_matrix_ast_to_ggml_matmul() {
        let source = include_str!("../../../examples/ggml_matmul.ufp");
        let c = emit_ggml_matmul_ufp(source).unwrap();
        assert!(c.contains("ggml_mul_mat"));
        assert!(c.contains("const float data_0[] = {1.000000000f"));
        assert!(c.contains("out:"));
        assert!(c.contains("int ufp_execute(float *output, size_t output_len)"));
    }

    #[test]
    fn rejects_non_supported_matrix_programs() {
        let source = include_str!("../../../examples/ggml_matmul.ufp")
            .replace("let out = a * b;", "let out = a / b;");
        assert!(emit_ggml_matmul_ufp(&source)
            .unwrap_err()
            .contains("operator `/` is unsupported"));
    }

    #[test]
    fn lowers_nested_literals_typed_shapes_and_elementwise_ops() {
        let source = include_str!("../../../examples/ggml_matrix_ops.ufp");
        let c = emit_ggml_matmul_ufp(source).unwrap();
        assert!(c.contains("ggml_add"));
        assert!(c.contains("ggml_mul"));
        assert!(c.contains("ggml_sub"));
        assert!(c.contains("1.000000000f,2.000000000f,3.000000000f,4.000000000f"));
    }

    #[test]
    fn lowers_three_matrix_matmul_chain() {
        let source = include_str!("../../../examples/ggml_matmul_chain.ufp");
        let c = emit_ggml_matmul_ufp(source).unwrap();
        assert_eq!(c.matches("ggml_mul_mat").count(), 2);
    }

    #[test]
    fn rejects_ragged_nested_matrix_rows() {
        let source = include_str!("../../../examples/ggml_matrix_ops.ufp")
            .replace("[[1.0, 2.0], [3.0, 4.0]]", "[[1.0, 2.0], [3.0]]");
        assert!(emit_ggml_matmul_ufp(&source)
            .unwrap_err()
            .contains("equal lengths"));
    }

    #[test]
    fn rejects_incompatible_elementwise_shapes() {
        let source = include_str!("../../../examples/ggml_matrix_ops.ufp").replace(
            "matrix<2>[2,2] of float<32> b",
            "matrix<2>[1,4] of float<32> b",
        );
        assert!(emit_ggml_matmul_ufp(&source)
            .unwrap_err()
            .contains("equal matrix shapes"));
    }
}
