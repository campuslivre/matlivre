//! Shape algebra shared by every array type: N-D indexing plans, growth on assignment,
//! deletion, implicit expansion and concatenation. Containers only gather or scatter
//! elements at the positions these plans compute.

use crate::error::{rt, LangError};

type R<T> = Result<T, LangError>;

/// Array shape: at least two dimensions, trailing singleton dimensions removed.
pub type Dims = Vec<usize>;

pub fn normalize(mut d: Dims) -> Dims {
    while d.len() < 2 {
        d.push(if d.is_empty() { 0 } else { 1 });
    }
    while d.len() > 2 && d[d.len() - 1] == 1 {
        d.pop();
    }
    d
}

pub fn dims2(r: usize, c: usize) -> Dims {
    vec![r, c]
}

pub fn numel(d: &[usize]) -> usize {
    d.iter().product()
}

/// `2×3×4` size text.
pub fn size_text(d: &[usize]) -> String {
    d.iter().map(|x| x.to_string()).collect::<Vec<_>>().join("×")
}

/// One evaluated subscript, as zero-based positions.
#[derive(Debug, Clone)]
pub enum IndexArg {
    All,
    List { ix: Vec<usize>, dims: Dims, from_mask: bool },
}

impl IndexArg {
    pub fn scalar(i: usize) -> Self {
        IndexArg::List { ix: vec![i], dims: vec![1, 1], from_mask: false }
    }

    fn positions(&self, n: usize) -> Vec<usize> {
        match self {
            IndexArg::All => (0..n).collect(),
            IndexArg::List { ix, .. } => ix.clone(),
        }
    }

    fn count(&self, n: usize) -> usize {
        match self {
            IndexArg::All => n,
            IndexArg::List { ix, .. } => ix.len(),
        }
    }

    fn max_plus_one(&self) -> usize {
        match self {
            IndexArg::All => 0,
            IndexArg::List { ix, .. } => ix.iter().copied().max().map(|m| m + 1).unwrap_or(0),
        }
    }
}

/// Dims seen through `k` subscripts: trailing dimensions fold into the k-th; missing
/// dimensions are 1.
pub fn effective_dims(d: &[usize], k: usize) -> Dims {
    (0..k)
        .map(|i| {
            if i + 1 < k {
                *d.get(i).unwrap_or(&1)
            } else if i < d.len() {
                d[i..].iter().product()
            } else {
                1
            }
        })
        .collect()
}

/// Value of `end` in subscript `k` of `n`.
pub fn end_value(d: &[usize], k: usize, n: usize) -> usize {
    if n == 1 {
        numel(d)
    } else {
        effective_dims(d, n)[k]
    }
}

fn strides(d: &[usize]) -> Vec<usize> {
    let mut s = Vec::with_capacity(d.len());
    let mut acc = 1;
    for &x in d {
        s.push(acc);
        acc *= x;
    }
    s
}

/// Cartesian product of per-dimension positions, column-major, as linear offsets into `d`.
fn product_positions(d: &[usize], lists: &[Vec<usize>]) -> Vec<usize> {
    let st = strides(d);
    let total: usize = lists.iter().map(|l| l.len()).product();
    let mut out = Vec::with_capacity(total);
    if total == 0 {
        return out;
    }
    let mut counter = vec![0usize; lists.len()];
    loop {
        let mut off = 0;
        for (k, c) in counter.iter().enumerate() {
            off += lists[k][*c] * st[k];
        }
        out.push(off);
        let mut k = 0;
        loop {
            if k == counter.len() {
                return out;
            }
            counter[k] += 1;
            if counter[k] < lists[k].len() {
                break;
            }
            counter[k] = 0;
            k += 1;
        }
    }
}

fn bounds_error(pos: usize, limit: usize, n: usize) -> LangError {
    if n == 1 {
        LangError::new(format!("Index exceeds the number of array elements. Index must not exceed {limit}."), 0)
    } else {
        LangError::new(format!("Index in position {} exceeds array bounds. Index must not exceed {limit}.", pos + 1), 0)
    }
}

/// Positions read by `A(idx)` and the result shape.
pub fn index_plan(d: &[usize], idx: &[IndexArg]) -> R<(Vec<usize>, Dims)> {
    let n = numel(d);
    match idx.len() {
        0 => Ok(((0..n).collect(), d.to_vec())),
        1 => match &idx[0] {
            IndexArg::All => Ok(((0..n).collect(), vec![n, 1])),
            IndexArg::List { ix, dims: idims, from_mask } => {
                if ix.iter().any(|&i| i >= n) {
                    return Err(bounds_error(0, n, 1));
                }
                let k = ix.len();
                let src_row = d.len() == 2 && d[0] == 1 && d[1] != 1;
                let src_col = d.len() == 2 && d[1] == 1 && d[0] != 1;
                let idx_vector = idims.len() == 2 && (idims[0] == 1 || idims[1] == 1);
                // A vector source keeps its orientation; otherwise the index's shape wins
                // (a logical mask on a matrix yields a column).
                let shape = if src_row && (idx_vector || *from_mask) {
                    vec![1, k]
                } else if src_col && (idx_vector || *from_mask) || *from_mask {
                    vec![k, 1]
                } else {
                    idims.clone()
                };
                Ok((ix.clone(), normalize(shape)))
            }
        },
        k => {
            let e = effective_dims(d, k);
            let mut lists = Vec::with_capacity(k);
            for (p, a) in idx.iter().enumerate() {
                let l = a.positions(e[p]);
                if l.iter().any(|&i| i >= e[p]) {
                    return Err(bounds_error(p, e[p], k));
                }
                lists.push(l);
            }
            let shape: Dims = lists.iter().map(|l| l.len()).collect();
            Ok((product_positions(&e, &lists), normalize(shape)))
        }
    }
}

/// Plan for `A(idx) = rhs`: the (possibly grown) shape and the positions written, in the
/// order rhs elements are consumed.
pub fn assign_plan(d: &[usize], idx: &[IndexArg], rhs_dims: &[usize]) -> R<(Dims, Vec<usize>)> {
    let rn = numel(rhs_dims);
    match idx.len() {
        0 => rt("Indexing requires at least one subscript."),
        1 => {
            let n = numel(d);
            let pos = idx[0].positions(n);
            if rn != 1 && rn != pos.len() {
                return rt(format!(
                    "Unable to perform assignment because the left and right sides have a different number of elements ({} and {rn}).",
                    pos.len()
                ));
            }
            let need = idx[0].max_plus_one();
            let new_dims = if need > n {
                if n == 0 && d.iter().all(|&x| x == 0 || x == 1) && d.len() == 2 {
                    if rhs_dims.len() == 2 && rhs_dims[1] == 1 && rhs_dims[0] > 1 { vec![need, 1] } else { vec![1, need] }
                } else if d.len() == 2 && d[0] == 1 {
                    vec![1, need]
                } else if d.len() == 2 && d[1] == 1 {
                    vec![need, 1]
                } else if n == 0 {
                    vec![1, need]
                } else {
                    return rt("Attempt to grow array along ambiguous dimension.");
                }
            } else {
                d.to_vec()
            };
            Ok((new_dims, pos))
        }
        k => {
            let e = effective_dims(d, k);
            // ':' on an empty dimension takes its extent from the right-hand side.
            let rhs_nonsingleton: Vec<usize> = rhs_dims.iter().copied().filter(|&x| x != 1).collect();
            let mut rhs_it = rhs_nonsingleton.into_iter();
            let mut new_e = e.clone();
            for p in 0..k {
                match &idx[p] {
                    IndexArg::All => {
                        if e[p] == 0 {
                            new_e[p] = rhs_it.next().unwrap_or(1);
                        } else if idx[p].count(e[p]) != 1 {
                            rhs_it.next();
                        }
                    }
                    a => {
                        new_e[p] = new_e[p].max(a.max_plus_one());
                        if a.count(e[p]) != 1 {
                            rhs_it.next();
                        }
                    }
                }
            }
            let count: usize = idx.iter().zip(&new_e).map(|(a, &n)| a.count(n)).product();
            if rn != 1 && rn != count {
                let lhs: Vec<usize> = idx.iter().zip(&new_e).map(|(a, &n)| a.count(n)).collect();
                return rt(format!(
                    "Unable to perform assignment because the size of the left side is {} and the size of the right side is {}.",
                    size_text(&lhs).replace('×', "-by-"),
                    size_text(rhs_dims).replace('×', "-by-")
                ));
            }
            // Growing a folded trailing dimension is ambiguous unless the array has no
            // further dimensions.
            if k < d.len() && new_e[k - 1] != e[k - 1] && d[k..].iter().any(|&x| x > 1) {
                return rt("Attempt to grow array along ambiguous dimension.");
            }
            let new_dims: Dims = if k >= d.len() {
                let mut nd = new_e.clone();
                nd = normalize(nd);
                nd
            } else {
                let mut nd = new_e[..k - 1].to_vec();
                if new_e[k - 1] == e[k - 1] {
                    nd.extend_from_slice(&d[k - 1..]);
                } else {
                    nd.push(new_e[k - 1]);
                }
                normalize(nd)
            };
            let lists: Vec<Vec<usize>> = idx.iter().zip(&new_e).map(|(a, &n)| a.positions(n)).collect();
            Ok((new_dims, product_positions(&new_e, &lists)))
        }
    }
}

/// Copies `data` (shape `old`) into a buffer of shape `new`, filling new cells.
pub fn regrow<T: Clone>(data: Vec<T>, old: &[usize], new: &[usize], fill: T) -> Vec<T> {
    if old == new {
        return data;
    }
    let n_new = numel(new);
    if numel(old) == 0 {
        return vec![fill; n_new];
    }
    // Growing only the last dimension keeps the element order.
    let same_prefix = old.len() == new.len() && old[..old.len() - 1] == new[..new.len() - 1];
    if same_prefix {
        let mut v = data;
        v.resize(n_new, fill);
        return v;
    }
    let mut out = vec![fill; n_new];
    let os = strides(old);
    let ns = strides(new);
    let total = numel(old);
    for lin in 0..total {
        let mut rem = lin;
        let mut off = 0;
        for k in (0..old.len()).rev() {
            let sub = rem / os[k];
            rem %= os[k];
            off += sub * ns.get(k).copied().unwrap_or(0);
        }
        out[off] = data[lin].clone();
    }
    out
}

/// Applies `A(idx) = rhs` to a container in place. `rhs` has one element (broadcast) or one
/// per position.
pub fn assign<T: Clone>(dims: &mut Dims, data: &mut Vec<T>, idx: &[IndexArg], rhs_dims: &[usize], rhs: Vec<T>, fill: T) -> R<()> {
    let (new_dims, pos) = assign_plan(dims, idx, rhs_dims)?;
    if new_dims != *dims {
        let old = std::mem::take(data);
        *data = regrow(old, dims, &new_dims, fill);
        *dims = new_dims;
    }
    if rhs.len() == 1 {
        let v = rhs.into_iter().next().unwrap();
        for p in pos {
            data[p] = v.clone();
        }
    } else {
        for (p, v) in pos.into_iter().zip(rhs) {
            data[p] = v;
        }
    }
    Ok(())
}

/// Plan for `A(idx) = []`: kept positions and the new shape.
pub fn delete_plan(d: &[usize], idx: &[IndexArg]) -> R<(Vec<usize>, Dims)> {
    let n = numel(d);
    match idx.len() {
        1 => {
            let mut kill = vec![false; n];
            match &idx[0] {
                IndexArg::All => kill.iter_mut().for_each(|k| *k = true),
                IndexArg::List { ix, .. } => {
                    for &i in ix {
                        if i >= n {
                            return Err(bounds_error(0, n, 1));
                        }
                        kill[i] = true;
                    }
                }
            }
            let keep: Vec<usize> = (0..n).filter(|&i| !kill[i]).collect();
            let k = keep.len();
            let shape = if matches!(idx[0], IndexArg::All) {
                vec![0, 0]
            } else if d.len() == 2 && d[1] == 1 && d[0] != 1 {
                vec![k, 1]
            } else {
                vec![1, k]
            };
            Ok((keep, shape))
        }
        k => {
            let e = effective_dims(d, k);
            // Exactly one subscript may be a real selection; the others must cover everything.
            let mut sel = None;
            for (p, a) in idx.iter().enumerate() {
                let full = match a {
                    IndexArg::All => true,
                    IndexArg::List { ix, .. } => {
                        let mut v = ix.clone();
                        v.sort_unstable();
                        v.dedup();
                        v.len() == e[p] && v.iter().enumerate().all(|(i, &x)| i == x)
                    }
                };
                if !full {
                    if sel.is_some() {
                        return rt("A null assignment can have only one non-colon index.");
                    }
                    sel = Some(p);
                }
            }
            let Some(p) = sel else {
                return Ok((Vec::new(), { let mut z = e.clone(); z[0] = 0; normalize(z) }));
            };
            let kill: std::collections::HashSet<usize> = idx[p].positions(e[p]).into_iter().collect();
            if kill.iter().any(|&i| i >= e[p]) {
                return Err(bounds_error(p, e[p], k));
            }
            let lists: Vec<Vec<usize>> = (0..k).map(|q| if q == p { (0..e[q]).filter(|i| !kill.contains(i)).collect() } else { (0..e[q]).collect() }).collect();
            let mut shape: Dims = lists.iter().map(|l| l.len()).collect();
            if k < d.len() && p + 1 != k {
                // Restore the folded dimensions.
                shape.truncate(k - 1);
                shape.extend_from_slice(&d[k - 1..]);
            }
            Ok((product_positions(&e, &lists), normalize(shape)))
        }
    }
}

pub fn delete<T: Clone>(dims: &mut Dims, data: &mut Vec<T>, idx: &[IndexArg]) -> R<()> {
    let (keep, shape) = delete_plan(dims, idx)?;
    *data = keep.into_iter().map(|p| data[p].clone()).collect();
    *dims = shape;
    Ok(())
}

/// Result shape of implicit expansion.
pub fn broadcast_dims(a: &[usize], b: &[usize]) -> R<Dims> {
    let n = a.len().max(b.len());
    let mut out = Vec::with_capacity(n);
    for k in 0..n {
        let x = *a.get(k).unwrap_or(&1);
        let y = *b.get(k).unwrap_or(&1);
        out.push(if x == y {
            x
        } else if x == 1 {
            y
        } else if y == 1 {
            x
        } else {
            return rt("Arrays have incompatible sizes for this operation.");
        });
    }
    Ok(normalize(out))
}

/// Source offsets into `a` and `b` for each element of the expanded result.
pub fn broadcast_pairs(a: &[usize], b: &[usize]) -> R<(Dims, Vec<(usize, usize)>)> {
    let out = broadcast_dims(a, b)?;
    let n = numel(&out);
    let na = numel(a);
    let nb = numel(b);
    if a == b {
        return Ok((out, (0..n).map(|i| (i, i)).collect()));
    }
    if na == 1 {
        return Ok((out, (0..n).map(|i| (0, i)).collect()));
    }
    if nb == 1 {
        return Ok((out, (0..n).map(|i| (i, 0)).collect()));
    }
    let sa = strides(a);
    let sb = strides(b);
    let so = strides(&out);
    let mut pairs = Vec::with_capacity(n);
    for lin in 0..n {
        let mut rem = lin;
        let (mut oa, mut ob) = (0, 0);
        for k in (0..out.len()).rev() {
            let sub = rem / so[k];
            rem %= so[k];
            if *a.get(k).unwrap_or(&1) != 1 {
                oa += sub * sa[k];
            }
            if *b.get(k).unwrap_or(&1) != 1 {
                ob += sub * sb[k];
            }
        }
        pairs.push((oa, ob));
    }
    Ok((out, pairs))
}

/// Concatenates parts along `dim` (0-based). Empty `[]` parts are skipped by the caller.
pub fn concat<T: Clone>(parts: Vec<(Dims, Vec<T>)>, dim: usize) -> R<(Dims, Vec<T>)> {
    if parts.is_empty() {
        return Ok((vec![0, 0], Vec::new()));
    }
    let nd = parts.iter().map(|p| p.0.len()).max().unwrap().max(dim + 1);
    let pad = |d: &Dims| -> Dims {
        let mut v = d.clone();
        v.resize(nd, 1);
        v
    };
    let first = pad(&parts[0].0);
    let mut total = 0;
    for (d, _) in &parts {
        let pd = pad(d);
        for k in 0..nd {
            if k != dim && pd[k] != first[k] {
                return rt("Dimensions of arrays being concatenated are not consistent.");
            }
        }
        total += pd[dim];
    }
    let mut out_dims = first.clone();
    out_dims[dim] = total;
    // Block size below `dim` and number of outer blocks above it.
    let inner: usize = first[..dim].iter().product();
    let outer: usize = first[dim + 1..].iter().product();
    let mut data = Vec::with_capacity(numel(&out_dims));
    for o in 0..outer {
        for (d, v) in &parts {
            let pd = pad(d);
            let chunk = inner * pd[dim];
            data.extend_from_slice(&v[o * chunk..(o + 1) * chunk]);
        }
    }
    Ok((normalize(out_dims), data))
}

/// `permute(A, order)` for any element type.
pub fn permute<T: Clone>(d: &[usize], data: &[T], order: &[usize]) -> R<(Dims, Vec<T>)> {
    let n = order.len().max(d.len());
    let mut src = d.to_vec();
    src.resize(n, 1);
    let mut seen = vec![false; n];
    for &o in order {
        if o >= n || seen[o] {
            return rt("ORDER must be a permutation of the dimensions.");
        }
        seen[o] = true;
    }
    if order.len() < n {
        return rt("ORDER must have at least N elements for an N-D array.");
    }
    let new: Dims = order.iter().map(|&o| src[o]).collect();
    let ss = strides(&src);
    let ns = strides(&new);
    let total = numel(&src);
    let mut out: Vec<Option<T>> = vec![None; total];
    for lin in 0..total {
        let mut rem = lin;
        let mut subs = vec![0; n];
        for k in (0..n).rev() {
            subs[k] = rem / ss[k];
            rem %= ss[k];
        }
        let mut off = 0;
        for (j, &o) in order.iter().enumerate() {
            off += subs[o] * ns[j];
        }
        out[off] = Some(data[lin].clone());
    }
    Ok((normalize(new), out.into_iter().map(|x| x.unwrap()).collect()))
}

/// 2-D transpose for any element type.
pub fn transpose<T: Clone>(d: &[usize], data: &[T]) -> R<(Dims, Vec<T>)> {
    if d.len() > 2 {
        return rt("Transpose on ND array is not defined. Use PERMUTE instead.");
    }
    permute(d, data, &[1, 0])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list(v: &[usize]) -> IndexArg {
        IndexArg::List { ix: v.to_vec(), dims: vec![1, v.len()], from_mask: false }
    }

    #[test]
    fn three_d_indexing() {
        let d = vec![2, 3, 4];
        let (pos, shape) = index_plan(&d, &[IndexArg::All, list(&[1]), list(&[2])]).unwrap();
        assert_eq!(shape, vec![2, 1]);
        assert_eq!(pos, vec![2 * 1 + 6 * 2, 1 + 2 * 1 + 6 * 2]);
        // Two subscripts fold the trailing dimensions.
        let (_, shape2) = index_plan(&d, &[IndexArg::All, IndexArg::All]).unwrap();
        assert_eq!(shape2, vec![2, 12]);
    }

    #[test]
    fn grow_into_third_dimension() {
        let mut dims = vec![2, 2];
        let mut data = vec![1, 2, 3, 4];
        assign(&mut dims, &mut data, &[IndexArg::All, IndexArg::All, list(&[1])], &[2, 2], vec![5, 6, 7, 8], 0).unwrap();
        assert_eq!(dims, vec![2, 2, 2]);
        assert_eq!(data, vec![1, 2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn concat_pages() {
        let (d, v) = concat(vec![(vec![2, 1], vec![1, 2]), (vec![2, 1], vec![3, 4])], 2).unwrap();
        assert_eq!(d, vec![2, 1, 2]);
        assert_eq!(v, vec![1, 2, 3, 4]);
    }

    #[test]
    fn permute_swaps() {
        let (d, v) = permute(&[2, 3], &[1, 2, 3, 4, 5, 6], &[1, 0]).unwrap();
        assert_eq!(d, vec![3, 2]);
        assert_eq!(v, vec![1, 3, 5, 2, 4, 6]);
    }
}
