//! The manual row order as sort keys: every record carries a string (base62,
//! `board::fractional_index`, the rocicorp/fractional-indexing port) that
//! compares between its neighbours'. Moving or inserting a row gives that row
//! a key between the two it lands between; no other row changes. Two copies
//! that inserted at the same place may come up with the same key: the file
//! is read in (key, id) order and `repair` gives one of them another.

use super::model::{RecId, Record};
use crate::board::fractional_index as fi;

/// A key that `fractional_index` can work from.
pub fn valid(k: &str) -> bool { !k.is_empty() && fi::validate_order_key(k).is_ok() }

/// A key after `a` and before `b` (None: the start / end). One that is
/// invalid, or a `b` not above `a`, counts as not given.
pub fn key_between(a: Option<&str>, b: Option<&str>) -> String { keys_between(a, b, 1).pop().unwrap_or_default() }

/// `n` rising keys between `a` and `b`, as short as they come: what a paste
/// or a new file numbers its rows with.
pub fn keys_between(a: Option<&str>, b: Option<&str>, n: usize) -> Vec<String> {
    let a = a.filter(|k| valid(k));
    let b = b.filter(|k| valid(k) && a.is_none_or(|a| a < *k));
    // keys `valid` let through and `a < b`: nothing for it to refuse
    fi::generate_n_keys_between(a, b, n).unwrap_or_default()
}

/// Give every record a key that rises along `recs`, keeping as many as can
/// stay (the longest strictly rising run of valid ones) and filling the
/// rest between their neighbours. Returns the records whose key changed.
pub fn repair(recs: &mut [Record]) -> Vec<RecId> {
    if recs.iter().all(|r| valid(&r.ord)) && recs.windows(2).all(|w| w[0].ord < w[1].ord) { return vec![]; }
    // longest strictly increasing subsequence of the valid keys (patience sort)
    let mut tails: Vec<usize> = vec![];
    let mut prev: Vec<Option<usize>> = vec![None; recs.len()];
    for i in (0..recs.len()).filter(|i| valid(&recs[*i].ord)) {
        let at = tails.partition_point(|&j| recs[j].ord < recs[i].ord);
        // of two equal keys the first stays
        if tails.get(at).is_some_and(|&j| recs[j].ord == recs[i].ord) { continue; }
        prev[i] = at.checked_sub(1).map(|p| tails[p]);
        if at == tails.len() { tails.push(i); } else { tails[at] = i; }
    }
    let mut keep = vec![false; recs.len()];
    let mut i = tails.last().copied();
    while let Some(j) = i { keep[j] = true; i = prev[j]; }
    let mut changed = vec![];
    let mut at = 0;
    while at < recs.len() {
        if keep[at] { at += 1; continue; }
        let end = (at..recs.len()).find(|i| keep[*i]).unwrap_or(recs.len());
        let lo = at.checked_sub(1).map(|i| recs[i].ord.clone());
        let keys = keys_between(lo.as_deref(), recs.get(end).map(|r| r.ord.as_str()), end - at);
        for (r, k) in recs[at..end].iter_mut().zip(keys) { r.ord = k; changed.push(r.id); }
        at = end;
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn between() {
        assert_eq!(key_between(None, None), "a0");
        let (a, b) = (key_between(None, None), key_between(Some("a0"), None));
        assert!(a < b && key_between(None, Some(&a)) < a);
        let m = key_between(Some(&a), Some(&b));
        assert!(a < m && m < b);
        // neighbours with nothing between them at their length: one digit more
        let m = key_between(Some("a0"), Some("a0V"));
        assert!("a0" < m.as_str() && m.as_str() < "a0V");
        // what isn't a key is as good as not given
        assert_eq!(key_between(Some("x"), Some("")), key_between(None, None));
        assert!(key_between(Some("a5"), Some("a1")).as_str() > "a5");
        let ks = keys_between(Some("a1"), Some("a2"), 50);
        assert!(ks.len() == 50 && ks.windows(2).all(|w| w[0] < w[1]) && ks[0].as_str() > "a1" && ks[49].as_str() < "a2");
    }

    /// The same place over and over: each new row right before the last one.
    #[test]
    fn thousand_at_one_place() {
        let (lo, mut hi) = ("a0".to_string(), "a1".to_string());
        let (mut top, mut end) = (0, "a2".to_string());
        for _ in 0..1000 {
            let k = key_between(Some(&lo), Some(&hi));
            assert!(lo < k && k < hi);
            hi = k;
            // and appending at the end, the common case
            let e = key_between(Some(&end), None);
            assert!(e > end);
            end = e;
            top = top.max(hi.len());
        }
        println!("1000 inserts at one place: key length {top}; 1000 appends: {}", end.len());
        assert!(top < 200 && end.len() <= 4);
    }

    #[test]
    fn repair_keeps_what_it_can() {
        let rec = |id, ord: &str| Record { id, ord: ord.into(), ..Default::default() };
        let mut recs = vec![rec(1, "a1"), rec(2, "a2"), rec(3, "a3")];
        assert!(repair(&mut recs).is_empty());
        // a row moved without a new key, an empty key, a duplicate, an invalid one
        let mut recs = vec![rec(1, "a1"), rec(3, "a3"), rec(2, "a2"), rec(4, ""), rec(5, "a5"), rec(6, "a5"), rec(7, "!"), rec(8, "a8")];
        let mut changed = repair(&mut recs);
        changed.sort();
        assert_eq!(changed, [3, 4, 6, 7]);
        assert!(recs.windows(2).all(|w| w[0].ord < w[1].ord) && recs.iter().all(|r| valid(&r.ord)));
    }
}
