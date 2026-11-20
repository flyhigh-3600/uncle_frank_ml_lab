//! Native C emitter for the M0 text-terminal slice.
//!
//! This intentionally supports only direct input-to-output text pipes. It proves
//! the end-to-end artifact path; typed IR and ggml partitions replace this small
//! emitter in E3/E6.

use std::{collections::BTreeMap, fs, path::Path, process::Command};
use ufp_syntax::{Item, Plan};

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
}
