//! AArch64 branch-target filter for Mach-O code sections.
//!
//! `BL` and `ADRP` encode PC-relative targets, so the same callee looks
//! different at every call site. Converting them to absolute targets makes
//! repeated calls byte-identical, which LZ-style codecs exploit. This is the
//! same bijection as the XZ Utils ARM64 BCJ filter; it never changes length.
//!
//! Unlike a blind BCJ pass we only touch instruction sections found by parsing
//! the Mach-O load commands, so data, strings and x86_64 slices of universal
//! binaries are left alone.

const CPU_TYPE_ARM64: u32 = 0x0100_000C;
const LC_SEGMENT_64: u32 = 0x19;
const S_ATTR_PURE_INSTRUCTIONS: u32 = 0x8000_0000;
const S_ATTR_SOME_INSTRUCTIONS: u32 = 0x0000_0400;

/// Returns the file ranges holding arm64 instructions, sorted and disjoint.
pub fn code_ranges(data: &[u8]) -> Vec<(u64, u64)> {
    let mut ranges = Vec::new();
    let be32 = |o: usize| data.get(o..o + 4).map(|b| u32::from_be_bytes(b.try_into().unwrap()));
    let be64 = |o: usize| data.get(o..o + 8).map(|b| u64::from_be_bytes(b.try_into().unwrap()));

    match be32(0) {
        Some(0xCAFE_BABE) | Some(0xCAFE_BABF) => {
            let is64 = be32(0) == Some(0xCAFE_BABF);
            let n = be32(4).unwrap_or(0).min(32) as usize;
            let stride = if is64 { 32 } else { 20 };
            for i in 0..n {
                let a = 8 + i * stride;
                let Some(cpu) = be32(a) else { break };
                let (off, size) = if is64 {
                    (be64(a + 8), be64(a + 16))
                } else {
                    (be32(a + 8).map(u64::from), be32(a + 12).map(u64::from))
                };
                let (Some(off), Some(size)) = (off, size) else { break };
                if cpu != CPU_TYPE_ARM64 {
                    continue;
                }
                let end = off.saturating_add(size);
                if end > data.len() as u64 {
                    continue;
                }
                thin_ranges(&data[off as usize..end as usize], off, &mut ranges);
            }
        }
        _ => thin_ranges(data, 0, &mut ranges),
    }

    ranges.sort_unstable();
    let mut out: Vec<(u64, u64)> = Vec::with_capacity(ranges.len());
    for (off, len) in ranges {
        if let Some(last) = out.last() {
            if off < last.0 + last.1 {
                continue; // overlapping sections: keep the first, stay a bijection
            }
        }
        out.push((off, len));
    }
    out
}

fn thin_ranges(m: &[u8], base: u64, out: &mut Vec<(u64, u64)>) {
    let le32 = |o: usize| m.get(o..o + 4).map(|b| u32::from_le_bytes(b.try_into().unwrap()));
    let le64 = |o: usize| m.get(o..o + 8).map(|b| u64::from_le_bytes(b.try_into().unwrap()));

    if le32(0) != Some(0xFEED_FACF) || le32(4) != Some(CPU_TYPE_ARM64) {
        return;
    }
    let ncmds = le32(16).unwrap_or(0).min(4096);
    let mut p = 32usize;
    for _ in 0..ncmds {
        let (Some(cmd), Some(size)) = (le32(p), le32(p + 4)) else { return };
        if size < 8 {
            return;
        }
        if cmd == LC_SEGMENT_64 {
            let nsects = le32(p + 64).unwrap_or(0).min(1024) as usize;
            for s in 0..nsects {
                let sp = p + 72 + s * 80;
                let (Some(size), Some(offset), Some(flags)) = (le64(sp + 40), le32(sp + 48), le32(sp + 64)) else {
                    break;
                };
                let code = flags & (S_ATTR_PURE_INSTRUCTIONS | S_ATTR_SOME_INSTRUCTIONS) != 0;
                if !code || offset == 0 || size < 4 {
                    continue;
                }
                let start = (offset as u64 + 3) & !3;
                let end = (offset as u64).saturating_add(size).min(m.len() as u64) & !3;
                if end > start {
                    out.push((base + start, end - start));
                }
            }
        }
        p += size as usize;
    }
}

/// Applies (`encode = true`) or reverses the filter in place over `ranges`.
pub fn filter(data: &mut [u8], ranges: &[(u64, u64)], encode: bool) -> anyhow::Result<()> {
    for &(off, len) in ranges {
        let end = off.checked_add(len).filter(|&e| e <= data.len() as u64);
        anyhow::ensure!(end.is_some() && off % 4 == 0, "invalid arm64 filter range");
        let region = &mut data[off as usize..(off + len) as usize];
        filter_region(region, off as u32, encode);
    }
    Ok(())
}

fn filter_region(buf: &mut [u8], start_pc: u32, encode: bool) {
    for (i, word) in buf.chunks_exact_mut(4).enumerate() {
        let pc = start_pc.wrapping_add((i as u32) << 2);
        let mut instr = u32::from_le_bytes(word.try_into().unwrap());

        if instr >> 26 == 0x25 {
            // BL imm26
            let src = instr;
            let mut p = pc >> 2;
            if !encode {
                p = p.wrapping_neg();
            }
            instr = 0x9400_0000 | (src.wrapping_add(p) & 0x03FF_FFFF);
            word.copy_from_slice(&instr.to_le_bytes());
        } else if instr & 0x9F00_0000 == 0x9000_0000 {
            // ADRP: only targets within +-512 MiB so the mapping stays bijective.
            let src = ((instr >> 29) & 3) | ((instr >> 3) & 0x001F_FFFC);
            if src.wrapping_add(0x0002_0000) & 0x001C_0000 != 0 {
                continue;
            }
            instr &= 0x9000_001F;
            let mut p = pc >> 12;
            if !encode {
                p = p.wrapping_neg();
            }
            let dest = src.wrapping_add(p);
            instr |= (dest & 3) << 29;
            instr |= (dest & 0x0003_FFFC) << 3;
            instr |= (dest & 0x0002_0000).wrapping_neg() & 0x00E0_0000;
            word.copy_from_slice(&instr.to_le_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filter_is_a_bijection_on_random_data() {
        let mut state = 0x1234_5678_9abc_def0u64;
        let mut data: Vec<u8> = (0..1 << 20)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state as u8
            })
            .collect();
        // Sprinkle real BL / ADRP encodings so both paths are exercised.
        for i in (0..data.len()).step_by(64) {
            data[i + 3] = 0x94;
            data[i + 35] = 0x90;
        }
        let orig = data.clone();
        let ranges = vec![(0, 1 << 19), (1 << 19, 1 << 19)];
        filter(&mut data, &ranges, true).unwrap();
        assert_ne!(data, orig);
        filter(&mut data, &ranges, false).unwrap();
        assert_eq!(data, orig);
    }

    #[test]
    fn rejects_bad_ranges() {
        let mut d = vec![0u8; 16];
        assert!(filter(&mut d, &[(4, 100)], true).is_err());
        assert!(filter(&mut d, &[(2, 4)], true).is_err());
    }
}
