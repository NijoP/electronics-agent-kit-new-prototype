//! Minimal store-only (uncompressed) ZIP writer and verifying reader.
//!
//! JLCPCB takes the Gerber and drill files as one `.zip`. Gerber text compresses well, but
//! these files are small and a dependency-free writer keeps the build identical on every OS.
//! Entries use a fixed DOS timestamp (1980-01-01) so the archive is byte-for-byte
//! reproducible from the same inputs.

const DOS_TIME: u16 = 0;
const DOS_DATE: u16 = (1 << 5) | 1; // 1980-01-01

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, slot) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *slot = c;
    }
    let mut crc = !0u32;
    for b in data {
        crc = table[((crc ^ *b as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    !crc
}

fn le16(v: &mut Vec<u8>, x: u16) {
    v.extend_from_slice(&x.to_le_bytes());
}
fn le32(v: &mut Vec<u8>, x: u32) {
    v.extend_from_slice(&x.to_le_bytes());
}

/// Build a ZIP archive holding `entries` (name, bytes) uncompressed.
pub fn stored(entries: &[(String, Vec<u8>)]) -> Result<Vec<u8>, String> {
    if entries.len() > u16::MAX as usize {
        return Err("too many entries for a plain ZIP".into());
    }
    let mut out = vec![];
    let mut central = vec![];
    for (name, data) in entries {
        if name.is_empty() || name.contains(['/', '\\']) || !name.is_ascii() {
            return Err(format!(
                "entry name {name:?} must be a plain ASCII file name"
            ));
        }
        if data.len() as u64 >= u32::MAX as u64 || out.len() as u64 >= u32::MAX as u64 {
            return Err("archive would need ZIP64".into());
        }
        let offset = out.len() as u32;
        let crc = crc32(data);
        // local file header
        le32(&mut out, 0x0403_4b50);
        le16(&mut out, 20);
        le16(&mut out, 0);
        le16(&mut out, 0); // stored
        le16(&mut out, DOS_TIME);
        le16(&mut out, DOS_DATE);
        le32(&mut out, crc);
        le32(&mut out, data.len() as u32);
        le32(&mut out, data.len() as u32);
        le16(&mut out, name.len() as u16);
        le16(&mut out, 0);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        // central directory header
        le32(&mut central, 0x0201_4b50);
        le16(&mut central, 20);
        le16(&mut central, 20);
        le16(&mut central, 0);
        le16(&mut central, 0);
        le16(&mut central, DOS_TIME);
        le16(&mut central, DOS_DATE);
        le32(&mut central, crc);
        le32(&mut central, data.len() as u32);
        le32(&mut central, data.len() as u32);
        le16(&mut central, name.len() as u16);
        le16(&mut central, 0);
        le16(&mut central, 0);
        le16(&mut central, 0);
        le16(&mut central, 0);
        le32(&mut central, 0);
        le32(&mut central, offset);
        central.extend_from_slice(name.as_bytes());
    }
    let cd_offset = out.len() as u32;
    let cd_len = central.len() as u32;
    out.extend_from_slice(&central);
    le32(&mut out, 0x0605_4b50);
    le16(&mut out, 0);
    le16(&mut out, 0);
    le16(&mut out, entries.len() as u16);
    le16(&mut out, entries.len() as u16);
    le32(&mut out, cd_len);
    le32(&mut out, cd_offset);
    le16(&mut out, 0);
    Ok(out)
}

fn rd16(b: &[u8], at: usize) -> Result<u16, String> {
    b.get(at..at + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or_else(|| "truncated archive".to_string())
}
fn rd32(b: &[u8], at: usize) -> Result<u32, String> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| "truncated archive".to_string())
}

/// Read a store-only archive back through its central directory, checking every CRC.
pub fn read_stored(zip: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    if zip.len() < 22 {
        return Err("too short to be a ZIP".into());
    }
    let eocd = (0..=zip.len() - 22)
        .rev()
        .find(|&i| rd32(zip, i) == Ok(0x0605_4b50))
        .ok_or("no end-of-central-directory record")?;
    let count = rd16(zip, eocd + 10)? as usize;
    let mut at = rd32(zip, eocd + 16)? as usize;
    let mut out = vec![];
    for _ in 0..count {
        if rd32(zip, at)? != 0x0201_4b50 {
            return Err("bad central directory entry".into());
        }
        if rd16(zip, at + 10)? != 0 {
            return Err("entry is compressed (not store-only)".into());
        }
        let crc = rd32(zip, at + 16)?;
        let size = rd32(zip, at + 24)? as usize;
        let nlen = rd16(zip, at + 28)? as usize;
        let xlen = rd16(zip, at + 30)? as usize;
        let clen = rd16(zip, at + 32)? as usize;
        let local = rd32(zip, at + 42)? as usize;
        let name = String::from_utf8(
            zip.get(at + 46..at + 46 + nlen)
                .ok_or("truncated name")?
                .to_vec(),
        )
        .map_err(|e| e.to_string())?;
        if rd32(zip, local)? != 0x0403_4b50 {
            return Err(format!("{name}: bad local header"));
        }
        let lname = rd16(zip, local + 26)? as usize;
        let lextra = rd16(zip, local + 28)? as usize;
        let start = local + 30 + lname + lextra;
        let data = zip
            .get(start..start + size)
            .ok_or_else(|| format!("{name}: truncated data"))?
            .to_vec();
        if crc32(&data) != crc {
            return Err(format!("{name}: CRC mismatch"));
        }
        out.push((name, data));
        at += 46 + nlen + xlen + clen;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_the_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn round_trips_and_detects_corruption() {
        let entries = vec![
            ("a.gtl".to_string(), b"G04 test*\nM02*\n".to_vec()),
            ("empty.txt".to_string(), vec![]),
        ];
        let z = stored(&entries).unwrap();
        assert_eq!(read_stored(&z).unwrap(), entries);
        let mut bad = z.clone();
        bad[30 + 5] ^= 0xFF; // inside the first entry's data
        assert!(read_stored(&bad).unwrap_err().contains("CRC"));
        assert!(stored(&[("../x".into(), vec![])]).is_err());
    }
}
