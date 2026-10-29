//! Model F: B's contexts plus predictions from the continuity of pixels across
//! block edges. The 7x7 interior is coded first; then each top-row coefficient
//! (horizontal frequency u) is predicted so that the block's top pixel row,
//! taken as a 1-D cosine series in x, matches the bottom row of the block above;
//! the left column likewise from the block on the left; DC last, from both.

use super::{bucket, Coder, Ctx, Image};
use std::f64::consts::{FRAC_1_SQRT_2, PI, SQRT_2};

pub static CAT: std::sync::Mutex<[f64; 5]> = std::sync::Mutex::new([0.0; 5]);

pub const ZZ: [usize; 64] = [
    0, 1, 8, 16, 9, 2, 3, 10, 17, 24, 32, 25, 18, 11, 4, 5, 12, 19, 26, 33, 40, 48, 41, 34, 27, 20, 13, 6, 7, 14, 21, 28, 35, 42, 49, 56, 57, 50, 43,
    36, 29, 22, 15, 23, 30, 37, 44, 51, 58, 59, 52, 45, 38, 31, 39, 46, 53, 60, 61, 54, 47, 55, 62, 63,
];

fn var(name: &str) -> i32 {
    std::env::var(name).ok().and_then(|v| v.parse().ok()).unwrap_or(0)
}

/// Signed value: zero flag, unary exponent, contexted top mantissa bit, sign.
fn signed(c: &mut Coder, x: &mut Ctx, base: u64, v: i32, sctx: u64) {
    c.code(x.get(base << 8), v != 0, 5);
    if v == 0 {
        return;
    }
    let a = v.unsigned_abs();
    let e = 32 - a.leading_zeros();
    for i in 1..e {
        c.code(x.get((base << 8) | i as u64), true, 5);
    }
    if e < 16 {
        c.code(x.get((base << 8) | e as u64), false, 5);
    }
    for b in (0..e - 1).rev() {
        let bit = (a >> b) & 1 == 1;
        if b == e - 2 {
            c.code(x.get((base << 8) | 0x80 | e as u64), bit, 5);
        } else {
            c.bits += 1.0;
            c.decisions += 1;
        }
    }
    c.code(x.get((base << 8) | 0xF0 | sctx), v < 0, 5);
}

pub fn estimate_f(img: &Image) -> (f64, u64) {
    let grad = var("F_GRAD"); // 0: plain continuity, 1: blend in the neighbour's gradient
    let edge_mode = var("F_EDGE");
    let edge_as_value = edge_mode == 2;
    let dc_mode = var("F_DC");
    let mut cat = [0f64; 5];
    let mut cosk = [0f64; 8];
    let mut cu = [1f64; 8];
    for u in 0..8 {
        cosk[u] = (u as f64 * PI / 16.0).cos();
    }
    cu[0] = FRAC_1_SQRT_2;
    // weights of the far edge (row 7) and of the row before it (row 6), per frequency
    let mut w7 = [0f64; 8];
    let mut w6 = [0f64; 8];
    for v in 0..8 {
        w7[v] = cu[v] * (15.0 * v as f64 * PI / 16.0).cos();
        w6[v] = cu[v] * (13.0 * v as f64 * PI / 16.0).cos();
    }
    let mut cy = [[0f64; 8]; 8];
    for y in 0..8 {
        for v in 0..8 {
            cy[y][v] = cu[v] * ((2 * y + 1) as f64 * v as f64 * PI / 16.0).cos();
        }
    }
    let dc_agg = var("F_DCAGG");
    let mut c = Coder::new();
    let mut total_bits = 0.0;
    let mut total_dec = 0;
    for (ci, comp) in img.comps.iter().enumerate() {
        let cc = ci.min(1) as u64;
        let q = img.qt.get(&comp.tq).copied().unwrap_or([1; 64]);
        let (bw, bh) = (comp.bw, comp.bh);
        let nat: Vec<[i32; 64]> = comp
            .coef
            .iter()
            .map(|b| {
                let mut n = [0i32; 64];
                for k in 0..64 {
                    n[ZZ[k]] = b[k] as i32;
                }
                n
            })
            .collect();
        let deq = |n: &[i32; 64], p: usize| n[p] as f64 * q[p] as f64;
        let mut x = Ctx::default();
        c.bits = 0.0;
        c.decisions = 0;
        let nint = |n: &[i32; 64]| -> u32 {
            let mut k = 0;
            for v in 1..8 {
                for u in 1..8 {
                    k += (n[v * 8 + u] != 0) as u32;
                }
            }
            k
        };
        // interior positions in zigzag order
        let int_zz: Vec<usize> = ZZ.iter().copied().filter(|&p| p / 8 >= 1 && p % 8 >= 1).collect();
        for by in 0..bh {
            for bx in 0..bw {
                let b = by * bw + bx;
                let cur = &nat[b];
                let up = if by > 0 { Some(&nat[b - bw]) } else { None };
                let lf = if bx > 0 { Some(&nat[b - 1]) } else { None };
                // interior nonzero count
                let nctx = match (up.map(nint), lf.map(nint)) {
                    (Some(a), Some(b)) => bucket((a + b + 1) / 2),
                    (Some(a), None) | (None, Some(a)) => bucket(a),
                    _ => 15,
                };
                let b0 = c.bits;
                let n = nint(cur);
                let mut node = 1u64;
                for i in (0..6).rev() {
                    let bit = (n >> i) & 1 == 1;
                    c.code(x.get((cc << 40) | (3 << 32) | (nctx << 8) | node), bit, 5);
                    node = (node << 1) | bit as u64;
                }
                cat[0] += c.bits - b0;
                let b0 = c.bits;
                let mut left = n;
                for (zi, &p) in int_zz.iter().enumerate() {
                    if left == 0 {
                        break;
                    }
                    let nb = up.map_or(0, |u| u[p].unsigned_abs()) + lf.map_or(0, |l| l[p].unsigned_abs());
                    let v = cur[p];
                    let sctx = match (up, lf) {
                        (Some(a), Some(b)) => ((a[p].signum() + b[p].signum() + 2) as u64).min(4),
                        _ => 5,
                    };
                    signed(&mut c, &mut x, (cc << 40) | (4 << 32) | ((zi as u64) << 16) | (bucket(nb).min(7) << 4) | bucket(left).min(7), v, sctx);
                    if v != 0 {
                        left -= 1;
                    }
                }
                cat[1] += c.bits - b0;
                let b0 = c.bits;
                // top row: u = 1..7 (position 0*8+u), predicted from the block above
                for u in 1..8 {
                    let p = u;
                    let colnz = (1..8).filter(|&v| cur[v * 8 + u] != 0).count() as u64;
                    let v = cur[p];
                    if edge_mode == 0 {
                        let nb = up.map_or(0, |a| a[p].unsigned_abs()) + lf.map_or(0, |l| l[p].unsigned_abs());
                        signed(&mut c, &mut x, (cc << 40) | (6 << 32) | ((u as u64) << 16) | (bucket(nb).min(7) << 4) | colnz.min(3), v, 0);
                    } else if let Some(a) = up {
                        let mut a_up = 0.0;
                        let mut a_up6 = 0.0;
                        for vv in 0..8 {
                            a_up += w7[vv] * deq(a, vv * 8 + u);
                            a_up6 += w6[vv] * deq(a, vv * 8 + u);
                        }
                        let target = if grad == 1 { a_up + 0.5 * (a_up - a_up6) } else { a_up };
                        let mut s_cur = 0.0;
                        for vv in 1..8 {
                            s_cur += cu[vv] * cosk[vv] * deq(cur, vv * 8 + u);
                        }
                        let mut pred = SQRT_2 * (target - s_cur) / q[p] as f64;
                        if grad == 2 {
                            let (mut r7, mut r6, mut c0, mut c1) = (0.0, 0.0, 0.0, 0.0);
                            for vv in 0..8 {
                                r7 += cy[7][vv] * deq(a, vv * 8 + u);
                                r6 += cy[6][vv] * deq(a, vv * 8 + u);
                                if vv > 0 {
                                    c0 += cy[0][vv] * deq(cur, vv * 8 + u);
                                    c1 += cy[1][vv] * deq(cur, vv * 8 + u);
                                }
                            }
                            let k = r7 - c0 + ((r7 - r6) + (c1 - c0)) / 2.0;
                            pred = SQRT_2 * k / q[p] as f64;
                        }
                        let pq = pred.round() as i32;
                        if edge_as_value {
                            let pb = (bucket(pq.unsigned_abs()).min(7) as u64) << 1 | (pq < 0) as u64;
                            signed(&mut c, &mut x, (cc << 40) | (6 << 32) | ((u as u64) << 16) | (pb << 4) | colnz.min(3), v, (pq.signum() + 1) as u64);
                        } else {
                            signed(&mut c, &mut x, (cc << 40) | (6 << 32) | ((u as u64) << 16) | (bucket(pq.unsigned_abs()).min(7) << 4) | colnz.min(3), v - pq, 0);
                        }
                    } else {
                        signed(&mut c, &mut x, (cc << 40) | (7 << 32) | ((u as u64) << 16) | colnz.min(3), v, 0);
                    }
                }
                cat[2] += c.bits - b0;
                let b0 = c.bits;
                // left column: v = 1..7 (position v*8+0), predicted from the block on the left
                for vv in 1..8 {
                    let p = vv * 8;
                    let rownz = (1..8).filter(|&u| cur[vv * 8 + u] != 0).count() as u64;
                    let v = cur[p];
                    if edge_mode == 0 {
                        let nb = up.map_or(0, |a| a[p].unsigned_abs()) + lf.map_or(0, |l| l[p].unsigned_abs());
                        signed(&mut c, &mut x, (cc << 40) | (8 << 32) | ((vv as u64) << 16) | (bucket(nb).min(7) << 4) | rownz.min(3), v, 0);
                    } else if let Some(l) = lf {
                        let mut b_lf = 0.0;
                        let mut b_lf6 = 0.0;
                        for u in 0..8 {
                            b_lf += w7[u] * deq(l, vv * 8 + u);
                            b_lf6 += w6[u] * deq(l, vv * 8 + u);
                        }
                        let target = if grad == 1 { b_lf + 0.5 * (b_lf - b_lf6) } else { b_lf };
                        let mut s_cur = 0.0;
                        for u in 1..8 {
                            s_cur += cu[u] * cosk[u] * deq(cur, vv * 8 + u);
                        }
                        let mut pred = SQRT_2 * (target - s_cur) / q[p] as f64;
                        if grad == 2 {
                            let (mut r7, mut r6, mut c0, mut c1) = (0.0, 0.0, 0.0, 0.0);
                            for u in 0..8 {
                                r7 += cy[7][u] * deq(l, vv * 8 + u);
                                r6 += cy[6][u] * deq(l, vv * 8 + u);
                                if u > 0 {
                                    c0 += cy[0][u] * deq(cur, vv * 8 + u);
                                    c1 += cy[1][u] * deq(cur, vv * 8 + u);
                                }
                            }
                            let k = r7 - c0 + ((r7 - r6) + (c1 - c0)) / 2.0;
                            pred = SQRT_2 * k / q[p] as f64;
                        }
                        let pq = pred.round() as i32;
                        if edge_as_value {
                            let pb = (bucket(pq.unsigned_abs()).min(7) as u64) << 1 | (pq < 0) as u64;
                            signed(&mut c, &mut x, (cc << 40) | (8 << 32) | ((vv as u64) << 16) | (pb << 4) | rownz.min(3), v, (pq.signum() + 1) as u64);
                        } else {
                            signed(&mut c, &mut x, (cc << 40) | (8 << 32) | ((vv as u64) << 16) | (bucket(pq.unsigned_abs()).min(7) << 4) | rownz.min(3), v - pq, 0);
                        }
                    } else {
                        signed(&mut c, &mut x, (cc << 40) | (9 << 32) | ((vv as u64) << 16) | rownz.min(3), v, 0);
                    }
                }
                cat[3] += c.bits - b0;
                let b0 = c.bits;
                // DC from both edges
                let mut preds = vec![];
                if let Some(a) = up {
                    let mut a_up = 0.0;
                    let mut a_up6 = 0.0;
                    for vv in 0..8 {
                        a_up += w7[vv] * deq(a, vv * 8);
                        a_up6 += w6[vv] * deq(a, vv * 8);
                    }
                    let target = if grad == 1 { a_up + 0.5 * (a_up - a_up6) } else { a_up };
                    let mut s_cur = 0.0;
                    for vv in 1..8 {
                        s_cur += cu[vv] * cosk[vv] * deq(cur, vv * 8);
                    }
                    preds.push(SQRT_2 * (target - s_cur) / q[0] as f64);
                }
                if let Some(l) = lf {
                    let mut b_lf = 0.0;
                    let mut b_lf6 = 0.0;
                    for u in 0..8 {
                        b_lf += w7[u] * deq(l, u);
                        b_lf6 += w6[u] * deq(l, u);
                    }
                    let target = if grad == 1 { b_lf + 0.5 * (b_lf - b_lf6) } else { b_lf };
                    let mut s_cur = 0.0;
                    for u in 1..8 {
                        s_cur += cu[u] * cosk[u] * deq(cur, u);
                    }
                    preds.push(SQRT_2 * (target - s_cur) / q[0] as f64);
                }
                if dc_mode == 2 {
                    // pixel rows/columns: p(x, y) = 1/4 sum_u sum_v cy[x][u] cy[y][v] D(v, u)
                    let pix = |n: &[i32; 64], x: usize, y: usize, skip_dc: bool| -> f64 {
                        let mut s = 0.0;
                        for v in 0..8 {
                            for u in 0..8 {
                                if skip_dc && u == 0 && v == 0 { continue; }
                                s += cy[x][u] * cy[y][v] * deq(n, v * 8 + u);
                            }
                        }
                        s / 4.0
                    };
                    preds.clear();
                    let mut ests: Vec<f64> = vec![];
                    if let Some(a) = up {
                        for xx in 0..8 {
                            let (u7, u6) = (pix(a, xx, 7, false), pix(a, xx, 6, false));
                            let (c0, c1) = (pix(cur, xx, 0, true), pix(cur, xx, 1, true));
                            ests.push(8.0 * (u7 - c0 + ((u7 - u6) + (c1 - c0)) / 2.0) / q[0] as f64);
                        }
                    }
                    if let Some(l) = lf {
                        for yy in 0..8 {
                            let (l7, l6) = (pix(l, 7, yy, false), pix(l, 6, yy, false));
                            let (c0, c1) = (pix(cur, 0, yy, true), pix(cur, 1, yy, true));
                            ests.push(8.0 * (l7 - c0 + ((l7 - l6) + (c1 - c0)) / 2.0) / q[0] as f64);
                        }
                    }
                    if !ests.is_empty() {
                        ests.sort_by(|a, b| a.partial_cmp(b).unwrap());
                        let m = ests.len();
                        let pv = if dc_agg == 1 { ests[m / 2] } else if dc_agg == 2 { ests[m / 4..m - m / 4].iter().sum::<f64>() / (m - 2 * (m / 4)) as f64 } else { ests.iter().sum::<f64>() / m as f64 };
                        preds.push(pv);
                        preds.push(pv + (ests[m - 1 - m / 4] - ests[m / 4]));
                    }
                }
                let (pred, spread) = match preds.len() {
                    2 => ((preds[0] + preds[1]) / 2.0, (preds[0] - preds[1]).abs()),
                    1 => (preds[0], 64.0),
                    _ => (0.0, 255.0),
                };
                if dc_mode == 0 {
                    let a = lf.map_or(0, |l| l[0]);
                    let bb = up.map_or(0, |u| u[0]);
                    let cdc = if up.is_some() && lf.is_some() { nat[b - bw - 1][0] } else { 0 };
                    let pr = match (lf.is_some(), up.is_some()) {
                        (true, true) => if cdc >= a.max(bb) { a.min(bb) } else if cdc <= a.min(bb) { a.max(bb) } else { a + bb - cdc },
                        (true, false) => a,
                        (false, true) => bb,
                        _ => 0,
                    };
                    let act = bucket(((a - cdc).unsigned_abs() + (bb - cdc).unsigned_abs()) as u32).min(10);
                    signed(&mut c, &mut x, (cc << 40) | (5 << 32) | (act << 4) | bucket(n).min(7), cur[0] - pr, 0);
                } else if dc_mode == 2 {
                    let (pq, sb) = if preds.len() == 2 { (preds[0].round() as i32, bucket((preds[1] - preds[0]).abs().round() as u32).min(10) as u64) } else { (0, 11) };
                    signed(&mut c, &mut x, (cc << 40) | (5 << 32) | (sb << 4) | bucket(n).min(7), cur[0] - pq, 0);
                } else {
                    let pq = pred.round() as i32;
                    let sb = bucket(spread.round() as u32).min(10) as u64;
                    signed(&mut c, &mut x, (cc << 40) | (5 << 32) | (sb << 4) | bucket(n).min(7), cur[0] - pq, 0);
                }
                cat[4] += c.bits - b0;
            }
        }
        total_bits += c.bits;
        total_dec += c.decisions;
    }
    {
        let mut g = CAT.lock().unwrap();
        for i in 0..5 { g[i] += cat[i]; }
    }
    if var("F_DEBUG") == 1 {
        eprintln!("grad {grad} edgeval {edge_as_value} qt0 {:?} comps {}", img.qt.get(&img.comps[0].tq).map(|q| &q[..8]), img.comps.len());
    }
    (total_bits, total_dec)
}
