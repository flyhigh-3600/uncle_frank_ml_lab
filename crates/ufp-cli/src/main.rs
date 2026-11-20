//! `ufp` — command-line driver for Uncle Frank's Plan.
//!
//! M0 scaffold. The real commands (`tokens`, `parse`, `io`, `run`) arrive with
//! UF-101/UF-102/UF-108.

fn main() {
    println!(
        "ufp {} — lexer/parser not wired yet (UF-101)",
        ufp_syntax::UFP_VERSION
    );
    println!("planned: ufp tokens <file.ufp> | ufp parse <file.ufp> | ufp run <file.ufp>");
}
