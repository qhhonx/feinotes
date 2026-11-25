//! `Store::open_top_folder`, `Store::list` and `Store::save_editor` timed on a library, for
//! docs/PERFORMANCE.md ("Large libraries"), and the bytes of device
//! metadata (`.quiet/meta/`) each save writes. Give it a copy: it writes there.
//!
//!     QUIET_MD_HOME=<lib> cargo run --release -p fdl_core --example store_bench <lib> [n]
use fdl_core::store::Store;
use std::time::Instant;

fn ms(t: Instant) -> f64 { t.elapsed().as_secs_f64() * 1000. }
fn med(mut v: Vec<f64>) -> f64 { v.sort_by(f64::total_cmp); v[v.len() / 2] }

/// Every file under `.quiet/meta/` with its size and what a write changes.
fn meta_files(root: &std::path::Path) -> std::collections::BTreeMap<std::path::PathBuf, (u64, std::time::SystemTime)> {
    let mut out = std::collections::BTreeMap::new();
    let mut dirs = vec![root.join(".quiet/meta")];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let Ok(m) = e.metadata() else { continue };
            if m.is_dir() { dirs.push(e.path()); } else { out.insert(e.path(), (m.len(), m.modified().unwrap())); }
        }
    }
    out
}

fn main() {
    let root = std::path::PathBuf::from(std::env::args().nth(1).expect("a library folder"));
    let n: usize = std::env::args().nth(2).map_or(15, |s| s.parse().unwrap());
    let t = Instant::now();
    let mut s = Store::open_top_folder(&root).unwrap();
    println!("open {:.1} ms", ms(t));
    let t = Instant::now();
    let entries = s.list();
    println!("first list {:.1} ms, {} entries", ms(t), entries.len());
    let list: Vec<f64> = (0..n).map(|_| { let t = Instant::now(); s.list(); ms(t) }).collect();
    // A note typed into and saved, as autosave does, without listing in between.
    let id = entries.iter().find(|e| e.meta.folder.is_none()).unwrap().id.clone();
    let mut doc = s.load(&id).unwrap();
    let mut bytes = vec![];
    let save: Vec<f64> = (0..n).map(|i| {
        doc.blocks.push(fdl_core::model::Block::text(&format!("typed {i}")));
        let before = meta_files(&root);
        let t = Instant::now(); s.save_editor(&id, &doc).unwrap(); let t = ms(t);
        // Files new or rewritten (backups included).
        bytes.push(meta_files(&root).into_iter().filter(|(p, f)| before.get(p) != Some(f)).map(|(_, f)| f.0).sum::<u64>() as f64);
        t
    }).collect();
    // The second carries the day's clean-up when it is due (FORMAT 6).
    println!("list median {:.1} ms (the second {:.1} ms), save median {:.1} ms ({n} each)", med(list.clone()), list[0], med(save[1..].to_vec()));
    println!("metadata written per save: median {:.0} bytes, largest {:.0}; all of it {} bytes", med(bytes.clone()), bytes.iter().cloned().fold(0., f64::max), meta_files(&root).values().map(|f| f.0).sum::<u64>());
    // A change to the metadata each time: 置顶 on and off.
    let mut bytes = vec![];
    let pin: Vec<f64> = (0..n).map(|i| {
        let before = meta_files(&root);
        let t = Instant::now(); s.set_pinned(&id, i % 2 == 0).unwrap(); let t = ms(t);
        bytes.push(meta_files(&root).into_iter().filter(|(p, f)| before.get(p) != Some(f)).map(|(_, f)| f.0).sum::<u64>() as f64);
        t
    }).collect();
    println!("set_pinned median {:.1} ms, metadata written median {:.0} bytes ({n} each)", med(pin), med(bytes));
}
