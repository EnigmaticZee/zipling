//! The hash behind block checks, object checks and deduplication: POLYVAL
//! (RFC 8452, the universal hash of AES-GCM-SIV), keyed with 128 random bits
//! drawn for each archive.
//!
//! Sixteen blocks at a time on the carry-less multiplier it runs at about
//! 14.6 GB/s on one M1 Pro core, 8x BLAKE3 with NEON. It is not a cryptographic
//! hash, and doesn't need to be for what it does here:
//!
//! * Dedup. The key is drawn after the files exist, so for any two different
//!   inputs the chance of a collision is at most (blocks + 2) / 2^128 over the
//!   key, however the files were made. That is a proof, not an assumption about
//!   a hash function: 2^32 chunks of 2 MiB leave a 2^-48 chance of any collision
//!   in 8 PB.
//! * Damage. The same bound holds for any damage that doesn't depend on the key.
//!
//! What it gives up: with the key in hand, a tamperer can make different bytes
//! hash alike. The key is in the index, so an edited archive can pass the block
//! checks, as it always could by rewriting BLAKE3 hashes next to the bytes. The
//! index itself stays under BLAKE3 (`format::hash16`), since the key lives there.

/// Blocks hashed between reductions. 8 gives 11.3 GB/s, 16 gives 14.6.
const STRIDE: usize = 16;
const BLOCK: usize = 16;

/// A POLYVAL key with the powers the wide path multiplies by.
pub struct Key {
    bytes: [u8; 16],
    #[cfg(target_arch = "aarch64")]
    wide: neon::Powers,
}

impl Key {
    /// A key from the system's random source.
    pub fn random() -> Key {
        let mut bytes = [0u8; 16];
        // SAFETY: valid out-pointer and length.
        unsafe { libc::arc4random_buf(bytes.as_mut_ptr().cast(), bytes.len()) };
        Key::new(bytes)
    }

    pub fn new(bytes: [u8; 16]) -> Key {
        Key {
            bytes,
            #[cfg(target_arch = "aarch64")]
            wide: neon::Powers::new(&bytes),
        }
    }

    pub fn bytes(&self) -> [u8; 16] {
        self.bytes
    }

    pub fn hash(&self, data: &[u8]) -> [u8; 16] {
        #[cfg(target_arch = "aarch64")]
        return self.wide.hash(data);
        #[cfg(not(target_arch = "aarch64"))]
        return scalar::hash(&self.bytes, data);
    }
}

/// What an archive's recorded hashes were made with.
pub enum Hasher {
    /// Versions 1 and 2: BLAKE3-128.
    Blake3,
    Polyval(Key),
}

impl Hasher {
    pub fn hash(&self, data: &[u8]) -> [u8; 16] {
        match self {
            Hasher::Blake3 => crate::format::hash16(data),
            Hasher::Polyval(k) => k.hash(data),
        }
    }
}

#[cfg(target_arch = "aarch64")]
mod neon {
    use super::{BLOCK, STRIDE};
    use std::arch::aarch64::*;

    /// The reduction constant of x^128 + x^127 + x^126 + x^121 + 1.
    const P: u64 = 0xc200_0000_0000_0000;

    /// `pow[i]` is H^(i+1); `mid[i]` holds its low half xor its high half.
    pub struct Powers {
        pow: [uint8x16_t; STRIDE],
        mid: [uint8x16_t; STRIDE],
    }

    #[inline(always)]
    unsafe fn low(x: uint8x16_t, y: uint8x16_t) -> uint8x16_t {
        let (a, b) = (vreinterpretq_p64_u8(x), vreinterpretq_p64_u8(y));
        vreinterpretq_u8_p128(vmull_p64(vgetq_lane_p64::<0>(a), vgetq_lane_p64::<0>(b)))
    }

    #[inline(always)]
    unsafe fn high(x: uint8x16_t, y: uint8x16_t) -> uint8x16_t {
        vreinterpretq_u8_p128(vmull_high_p64(vreinterpretq_p64_u8(x), vreinterpretq_p64_u8(y)))
    }

    #[inline(always)]
    unsafe fn swap(x: uint8x16_t) -> uint8x16_t {
        vextq_u8::<8>(x, x)
    }

    /// Karatsuba's three products. Staying in vector registers matters: the same
    /// sum through general registers ran at 8.9 GB/s.
    #[inline(always)]
    unsafe fn product(x: uint8x16_t, y: uint8x16_t, y_mid: uint8x16_t) -> (uint8x16_t, uint8x16_t, uint8x16_t) {
        (low(x, y), low(veorq_u8(x, swap(x)), y_mid), high(x, y))
    }

    /// 256 bits from Karatsuba's terms, reduced in Montgomery form.
    #[inline(always)]
    unsafe fn reduce(l: uint8x16_t, m: uint8x16_t, h: uint8x16_t) -> uint8x16_t {
        let (zero, p) = (vdupq_n_u8(0), vreinterpretq_u8_u64(vdupq_n_u64(P)));
        let t = veorq_u8(veorq_u8(m, l), h);
        let lo = veorq_u8(l, vextq_u8::<8>(zero, t));
        let hi = veorq_u8(h, vextq_u8::<8>(t, zero));
        let b = veorq_u8(swap(lo), low(lo, p));
        let d = veorq_u8(swap(b), low(b, p));
        veorq_u8(d, hi)
    }

    #[inline(always)]
    unsafe fn mul(x: uint8x16_t, y: uint8x16_t) -> uint8x16_t {
        let (l, m, h) = product(x, y, veorq_u8(y, swap(y)));
        reduce(l, m, h)
    }

    impl Powers {
        pub fn new(key: &[u8; 16]) -> Powers {
            // SAFETY: the pointers are to 16 bytes; the features are in the Apple
            // Silicon baseline.
            unsafe {
                let h = vld1q_u8(key.as_ptr());
                let mut pow = [h; STRIDE];
                for i in 1..STRIDE {
                    pow[i] = mul(pow[i - 1], h);
                }
                let mut mid = pow;
                for m in mid.iter_mut() {
                    *m = veorq_u8(*m, swap(*m));
                }
                Powers { pow, mid }
            }
        }

        pub fn hash(&self, data: &[u8]) -> [u8; 16] {
            // SAFETY: every load stays within `data` or a local block.
            unsafe {
                let mut y = vdupq_n_u8(0);
                let mut p = data.as_ptr();
                let mut n = data.len();
                while n >= STRIDE * BLOCK {
                    let top = STRIDE - 1;
                    let (mut l, mut m, mut h) = product(veorq_u8(y, vld1q_u8(p)), self.pow[top], self.mid[top]);
                    for i in 1..STRIDE {
                        let (a, b, c) = product(vld1q_u8(p.add(BLOCK * i)), self.pow[top - i], self.mid[top - i]);
                        l = veorq_u8(l, a);
                        m = veorq_u8(m, b);
                        h = veorq_u8(h, c);
                    }
                    y = reduce(l, m, h);
                    p = p.add(STRIDE * BLOCK);
                    n -= STRIDE * BLOCK;
                }
                let h = self.pow[0];
                while n >= BLOCK {
                    y = mul(veorq_u8(y, vld1q_u8(p)), h);
                    p = p.add(BLOCK);
                    n -= BLOCK;
                }
                if n > 0 {
                    let mut last = [0u8; BLOCK];
                    std::ptr::copy_nonoverlapping(p, last.as_mut_ptr(), n);
                    y = mul(veorq_u8(y, vld1q_u8(last.as_ptr())), h);
                }
                // The length ends the message, so zero bytes at its end count.
                let mut len = [0u8; BLOCK];
                len[..8].copy_from_slice(&(data.len() as u64).to_le_bytes());
                y = mul(veorq_u8(y, vld1q_u8(len.as_ptr())), h);
                let mut out = [0u8; BLOCK];
                vst1q_u8(out.as_mut_ptr(), y);
                out
            }
        }
    }
}

/// One bit at a time: for other platforms, and to check the wide path against.
#[cfg(any(test, not(target_arch = "aarch64")))]
mod scalar {
    use super::BLOCK;

    const POLY: u128 = (1 << 127) | (1 << 126) | (1 << 121) | 1;

    /// a * b * x^-128 modulo the polynomial.
    pub fn mul(a: u128, b: u128) -> u128 {
        let (mut z, mut v) = (0u128, b);
        for i in 0..128 {
            if (a >> i) & 1 == 1 {
                z ^= v;
            }
            v = (v << 1) ^ if v >> 127 == 1 { POLY } else { 0 };
        }
        for _ in 0..128 {
            z = (z >> 1) ^ if z & 1 == 1 { (POLY >> 1) | (1 << 127) } else { 0 };
        }
        z
    }

    pub fn hash(key: &[u8; 16], data: &[u8]) -> [u8; 16] {
        let h = u128::from_le_bytes(*key);
        let mut y = 0u128;
        for c in data.chunks(BLOCK) {
            let mut b = [0u8; BLOCK];
            b[..c.len()].copy_from_slice(c);
            y = mul(y ^ u128::from_le_bytes(b), h);
        }
        y = mul(y ^ data.len() as u128, h);
        y.to_le_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(n: usize, seed: u64) -> Vec<u8> {
        let mut s = seed;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1);
                (s >> 56) as u8
            })
            .collect()
    }

    fn hex(s: &str) -> [u8; 16] {
        std::array::from_fn(|i| u8::from_str_radix(&s[2 * i..2 * i + 2], 16).unwrap())
    }

    /// The worked example in RFC 8452, appendix A.
    #[test]
    fn multiply_matches_the_rfc() {
        let h = u128::from_le_bytes(hex("25629347589242761d31f826ba4b757b"));
        let x1 = u128::from_le_bytes(hex("4f4f95668c83dfb6401762bb2d01a262"));
        let x2 = u128::from_le_bytes(hex("d1a24ddd2721d006bbe45f20d3c9f362"));
        let y = scalar::mul(scalar::mul(x1, h) ^ x2, h);
        assert_eq!(y.to_le_bytes(), hex("f7a3b47b846119fae5b7866cf5e5b77e"));
    }

    #[test]
    fn wide_path_matches_one_bit_at_a_time() {
        let key = Key::new(hex("25629347589242761d31f826ba4b757b"));
        let data = bytes(5000, 3);
        for n in (0..700).chain([4095, 4096, 4097, 5000]) {
            assert_eq!(key.hash(&data[..n]), scalar::hash(&key.bytes(), &data[..n]), "length {n}");
        }
    }

    #[test]
    fn length_counts_and_any_flip_shows() {
        let key = Key::random();
        assert_ne!(key.hash(&[0; 16]), key.hash(&[0; 32]));
        assert_ne!(key.hash(&[0; 15]), key.hash(&[0; 16]));
        let mut data = bytes(1000, 9);
        let want = key.hash(&data);
        for bit in (0..8000).step_by(37) {
            data[bit / 8] ^= 1 << (bit % 8);
            assert_ne!(key.hash(&data), want);
            data[bit / 8] ^= 1 << (bit % 8);
        }
    }

    #[test]
    fn keys_differ() {
        let data = bytes(1000, 11);
        assert_ne!(Key::random().hash(&data), Key::random().hash(&data));
    }
}
