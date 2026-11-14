//! `subexpr`: rewrite an expression in terms of its most profitable common subexpression.
//!
//! MATLAB picks one subexpression per call. A candidate is a non-atomic node of the expression
//! tree that occurs at least twice, where a sum or product also "occurs" inside a larger sum or
//! product holding all of its terms or factors. The candidate saving the most nodes wins, ties
//! going to the larger and then the later one in canonical order.

use std::cmp::Ordering;

use super::expr::{cmp, Expr};

fn is_atom(e: &Expr) -> bool {
    matches!(e, Expr::Num(_) | Expr::Var(_) | Expr::Const(_))
}

fn size(e: &Expr) -> usize {
    1 + match e {
        Expr::Add(x) | Expr::Mul(x) | Expr::Fn(_, x) => x.iter().map(size).sum(),
        Expr::Pow(a, b) | Expr::Rel(_, a, b) => size(a) + size(b),
        _ => 0,
    }
}

fn nodes<'a>(e: &'a Expr, out: &mut Vec<&'a Expr>) {
    out.push(e);
    match e {
        Expr::Add(x) | Expr::Mul(x) | Expr::Fn(_, x) => x.iter().for_each(|k| nodes(k, out)),
        Expr::Pow(a, b) | Expr::Rel(_, a, b) => {
            nodes(a, out);
            nodes(b, out);
        }
        _ => {}
    }
}

/// `sub` is contained in `all` as a multiset.
fn contains_all(all: &[Expr], sub: &[Expr]) -> bool {
    let mut used = vec![false; all.len()];
    sub.iter().all(|s| match all.iter().enumerate().position(|(i, a)| !used[i] && a == s) {
        Some(i) => {
            used[i] = true;
            true
        }
        None => false,
    })
}

fn covers(node: &Expr, cand: &Expr) -> bool {
    if node == cand {
        return true;
    }
    match (node, cand) {
        (Expr::Add(a), Expr::Add(c)) | (Expr::Mul(a), Expr::Mul(c)) => a.len() > c.len() && contains_all(a, c),
        _ => false,
    }
}

fn replace(e: &Expr, cand: &Expr, v: &Expr) -> Expr {
    if e == cand {
        return v.clone();
    }
    match (e, cand) {
        (Expr::Add(a), Expr::Add(c)) | (Expr::Mul(a), Expr::Mul(c)) if covers(e, cand) => {
            let mut rest = a.clone();
            for s in c {
                if let Some(i) = rest.iter().position(|x| x == s) {
                    rest.remove(i);
                }
            }
            let mut items: Vec<Expr> = rest.iter().map(|x| replace(x, cand, v)).collect();
            items.push(v.clone());
            if matches!(e, Expr::Add(_)) { Expr::add(items) } else { Expr::mul(items) }
        }
        _ => match e {
            Expr::Add(x) => Expr::add(x.iter().map(|k| replace(k, cand, v)).collect()),
            Expr::Mul(x) => Expr::mul(x.iter().map(|k| replace(k, cand, v)).collect()),
            Expr::Fn(n, x) => Expr::func(n, x.iter().map(|k| replace(k, cand, v)).collect()),
            Expr::Pow(a, b) => Expr::pow(replace(a, cand, v), replace(b, cand, v)),
            Expr::Rel(r, a, b) => Expr::rel(*r, replace(a, cand, v), replace(b, cand, v)),
            other => other.clone(),
        },
    }
}

/// The rewritten elements and the chosen subexpression (if any).
pub fn subexpr(elems: &[Expr], name: &str) -> (Vec<Expr>, Option<Expr>) {
    let mut all = Vec::new();
    for e in elems {
        nodes(e, &mut all);
    }
    let mut cands: Vec<&Expr> = Vec::new();
    for n in &all {
        if !is_atom(n) && !matches!(n, Expr::Rel(..)) && !cands.contains(n) {
            cands.push(n);
        }
    }
    let mut best: Option<(&Expr, usize, usize)> = None;
    for c in cands {
        let count = all.iter().filter(|n| covers(n, c)).count();
        if count < 2 {
            continue;
        }
        let sz = size(c);
        let saving = (sz - 1) * (count - 1);
        let better = match &best {
            None => true,
            Some((b, bs, bsz)) => saving
                .cmp(bs)
                .then(sz.cmp(bsz))
                .then_with(|| cmp(c, b))
                == Ordering::Greater,
        };
        if better {
            best = Some((c, saving, sz));
        }
    }
    match best {
        None => (elems.to_vec(), None),
        Some((c, _, _)) => {
            let c = c.clone();
            let v = Expr::var(name);
            (elems.iter().map(|e| replace(e, &c, &v)).collect(), Some(c))
        }
    }
}
