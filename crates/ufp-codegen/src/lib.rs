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
}
