//! Writes a sample 数据表 file: `cargo run -p fdl_core --example table_demo -- <file.sqlite> [rows]`.
//! With `rows`, the large table of `sample::large` (性能测试); else the demo base.

use fdl_core::table::{field, sample, sqlite};

fn main() {
    let mut args = std::env::args().skip(1);
    let path = std::path::PathBuf::from(args.next().expect("usage: table_demo <file.sqlite> [rows]"));
    let base = match args.next().and_then(|n| n.parse().ok()) { Some(rows) => sample::large(rows), None => sample::demo(field::now()) };
    let _ = std::fs::remove_file(&path);
    sqlite::create(&path, &base).expect("write the file");
    println!("{}", path.display());
}
