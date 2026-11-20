//! Native C emitter for the M0 text-terminal slice.
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

/// Lower the currently supported UFP matrix program to native ggml C.
///
/// Supported shape: two top-level initialized `matrix<2>[rows, cols] of
/// float<32>` declarations, and one zero-argument procedure whose body is
/// exactly `let out = a * b; return out;`. Dimensions use conventional UFP
/// row/column order and matrix values are row-major.
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
    if declarations.len() != 2 || procedures.len() != 1 || ast.declarations.len() != 3 {
        return Err(
            "ggml UFP slice requires exactly two matrix declarations and one procedure".into(),
        );
    }
    let a = parse_matrix(declarations[0])?;
    let b = parse_matrix(declarations[1])?;
    if a.name == b.name {
        return Err("matrix declarations must have distinct names".into());
    }
    let (out_name, lhs, rhs) = parse_matmul_procedure(procedures[0])?;
    if out_name == a.name || out_name == b.name {
        return Err("matmul result binding must differ from matrix names".into());
    }
    if lhs != a.name || rhs != b.name {
        return Err(format!(
            "procedure must multiply declared matrices in order: {} * {}",
            a.name, b.name
        ));
    }
    if a.cols != b.rows {
        return Err(format!(
            "matrix dimension mismatch: {} columns but {} rows",
            a.cols, b.rows
        ));
    }
    let (a_values, b_values) = (c_values(&a.values), transpose_values(&b));
    let b_values = c_values(&b_values);
    let mut c = String::from(
        "#include <stdio.h>\n#include <math.h>\n#include <ggml.h>\n#include <ggml-backend.h>\n#include <ggml-alloc.h>\n#include <ggml-cpu.h>\nint main(void) {\n",
    );
    c.push_str(&format!(
        "  const float a_data[] = {{{a_values}}};\n  const float b_data[] = {{{b_values}}};\n"
    ));
    c.push_str(&format!(
        "  struct ggml_init_params p = {{.mem_size=16*1024*1024, .mem_buffer=NULL, .no_alloc=true}};\n  struct ggml_context *ctx=ggml_init(p); if(!ctx) return 1;\n  struct ggml_tensor *a=ggml_new_tensor_2d(ctx,GGML_TYPE_F32,{},{});\n  struct ggml_tensor *b=ggml_new_tensor_2d(ctx,GGML_TYPE_F32,{},{});\n  struct ggml_tensor *out=ggml_mul_mat(ctx,a,b);\n  struct ggml_cgraph *g=ggml_new_graph(ctx); ggml_build_forward_expand(g,out);\n  ggml_backend_t be=ggml_backend_cpu_init();\n  if(!be || !ggml_backend_alloc_ctx_tensors(ctx,be)) {{ fprintf(stderr,\"ggml CPU allocation failed\\\\n\"); return 1; }}\n  ggml_backend_tensor_set(a,a_data,0,sizeof(a_data)); ggml_backend_tensor_set(b,b_data,0,sizeof(b_data));\n  if(ggml_backend_graph_compute(be,g)!=GGML_STATUS_SUCCESS) {{ fprintf(stderr,\"ggml compute failed\\\\n\"); return 1; }}\n  float result[{}]; ggml_backend_tensor_get(out,result,0,sizeof(result));\n",
        a.cols, a.rows, b.rows, b.cols, a.rows * b.cols
    ));
    c.push_str(&format!(
        "  printf(\"{}:\"); for(int r=0;r<{};++r) for(int col=0;col<{};++col) printf(\" %.6g\",result[r+col*{}]); puts(\"\");\n  ggml_backend_free(be); ggml_free(ctx); return 0;\n}}\n",
        out_name, a.rows, b.cols, a.rows
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
    if rank != "2"
        || !matches!(element.as_ref(), TypeAst::Sized { name, bits: Some(bits), .. } if name == "float" && bits == "32")
    {
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
    let values = match node.expression.as_ref().map(|e| &e.kind) {
        Some(ExprKind::Matrix(values)) | Some(ExprKind::List(values)) => values
            .iter()
            .map(parse_f32_literal)
            .collect::<Result<Vec<_>, _>>()?,
        _ => {
            return Err(format!(
                "matrix `{name}` requires a flat numeric initializer"
            ))
        }
    };
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

fn parse_matmul_procedure(node: &SyntaxNode) -> Result<(String, String, String), String> {
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
    if blocks.len() != 1 || blocks[0].children.len() != 2 {
        return Err("procedure body must contain exactly `let out = a * b; return out;`".into());
    }
    let let_node = &blocks[0].children[0];
    let return_node = &blocks[0].children[1];
    if let_node.kind != SyntaxKind::Let || return_node.kind != SyntaxKind::Return {
        return Err("procedure body must contain exactly `let out = a * b; return out;`".into());
    }
    let out = let_node
        .children
        .iter()
        .find_map(|n| n.token.as_ref().map(|t| t.text.clone()))
        .ok_or("matmul result binding needs a name")?;
    let ExprKind::Binary { op, left, right } = &let_node
        .expression
        .as_ref()
        .ok_or("missing matmul expression")?
        .kind
    else {
        return Err("result must be a direct matrix multiplication".into());
    };
    if op != "*" {
        return Err("only `*` matrix multiplication is supported".into());
    }
    let identifier = |expr: &Expr| match &expr.kind {
        ExprKind::Identifier(name) => Ok(name.clone()),
        _ => Err("matmul operands must be declared matrix names".to_string()),
    };
    let (lhs, rhs) = (identifier(left)?, identifier(right)?);
    if !matches!(&return_node.expression.as_ref().map(|e| &e.kind), Some(ExprKind::Identifier(name)) if name == &out)
    {
        return Err("procedure must return the matmul result binding".into());
    }
    Ok((out, lhs, rhs))
}

fn transpose_values(matrix: &Matrix) -> Vec<f32> {
    let mut out = Vec::with_capacity(matrix.values.len());
    for col in 0..matrix.cols {
        for row in 0..matrix.rows {
            out.push(matrix.values[row * matrix.cols + col]);
        }
    }
    out
}

fn c_values(values: &[f32]) -> String {
    values
        .iter()
        .map(|v| format!("{v:.9}f"))
        .collect::<Vec<_>>()
        .join(",")
}

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
        assert!(c.contains("const float a_data[] = {1.000000000f"));
        assert!(c.contains("out:"));
    }

    #[test]
    fn rejects_non_supported_matrix_programs() {
        let source = include_str!("../../../examples/ggml_matmul.ufp")
            .replace("let out = a * b;", "let out = a + b;");
        assert!(emit_ggml_matmul_ufp(&source)
            .unwrap_err()
            .contains("only `*`"));
    }
}
