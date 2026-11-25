//! `Store::list` and `Store::save_editor` timed on a library, for
//! docs/PERFORMANCE.md ("Large libraries"). Give it a copy: it writes there.
//!
//!     QUIET_MD_HOME=<lib> cargo run --release -p fdl_core --example store_bench <lib> [n]
use fdl_core::store::Store;
use std::time::Instant;

fn ms(t: Instant) -> f64 { t.elapsed().as_secs_f64() * 1000. }
fn med(mut v: Vec<f64>) -> f64 { v.sort_by(f64::total_cmp); v[v.len() / 2] }

fn main() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).expect("a library folder"));
    let n: usize = std::env::args().nth(2).map_or(15, |s| s.parse().unwrap());
    let mut s = Store::open_top_folder(&root).unwrap();
    let t = Instant::now();
    let entries = s.list();
    println!("first list {:.1} ms, {} entries", ms(t), entries.len());
    let list: Vec<f64> = (0..n).map(|_| { let t = Instant::now(); s.list(); ms(t) }).collect();
    // A note typed into and saved, as autosave does, without listing in between.
    let id = entries.iter().find(|e| e.meta.folder.is_none()).unwrap().id.clone();
    let mut doc = s.load(&id).unwrap();
    let save: Vec<f64> = (0..n).map(|i| {
        doc.blocks.push(fdl_core::model::Block::text(&format!("typed {i}")));
        let t = Instant::now(); s.save_editor(&id, &doc).unwrap(); ms(t)
    }).collect();
    println!("list median {:.1} ms, save median {:.1} ms ({n} each)", med(list), med(save[1..].to_vec()));
}
