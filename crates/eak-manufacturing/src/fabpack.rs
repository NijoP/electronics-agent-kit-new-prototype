//! Fabrication pack: the Gerber and drill files plus a README of layers in one ZIP archive,
//! the form fabricators' order pages accept. The archive is written with stored (uncompressed)
//! entries so no compression dependency is needed; every reader supports that method.

/// CRC-32 (IEEE 802.3, reflected, polynomial 0xEDB88320) as required by the ZIP format.
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in data {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// A ZIP archive of `(name, bytes)` entries, stored without compression, with a fixed
/// timestamp (1980-01-01) so the same inputs always give the same bytes.
pub fn zip(entries: &[(String, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let crc = crc32(data);
        let offset = out.len() as u32;
        let size = data.len() as u32;
        let header = |sig: u32, central: bool| {
            let mut h = Vec::new();
            h.extend(sig.to_le_bytes());
            if central {
                h.extend(20u16.to_le_bytes()); // version made by
            }
            h.extend(20u16.to_le_bytes()); // version needed
            h.extend(0u16.to_le_bytes()); // flags
            h.extend(0u16.to_le_bytes()); // method: stored
            h.extend(0u16.to_le_bytes()); // time
            h.extend(0x21u16.to_le_bytes()); // date 1980-01-01
            h.extend(crc.to_le_bytes());
            h.extend(size.to_le_bytes());
            h.extend(size.to_le_bytes());
            h.extend((name.len() as u16).to_le_bytes());
            h.extend(0u16.to_le_bytes()); // extra length
            if central {
                h.extend(0u16.to_le_bytes()); // comment length
                h.extend(0u16.to_le_bytes()); // disk number
                h.extend(0u16.to_le_bytes()); // internal attributes
                h.extend(0u32.to_le_bytes()); // external attributes
                h.extend(offset.to_le_bytes());
            }
            h.extend(name.as_bytes());
            h
        };
        out.extend(header(0x0403_4b50, false));
        out.extend(data);
        central.extend(header(0x0201_4b50, true));
    }
    let cd_offset = out.len() as u32;
    let cd_size = central.len() as u32;
    out.extend(central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out.extend((entries.len() as u16).to_le_bytes());
    out.extend((entries.len() as u16).to_le_bytes());
    out.extend(cd_size.to_le_bytes());
    out.extend(cd_offset.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}

/// Names of the entries of a stored ZIP written by [`zip`], read back from its central
/// directory, with a CRC check of each entry's data. Used to self-verify the fab pack.
pub fn list(zip: &[u8]) -> Result<Vec<String>, String> {
    let rd16 = |o: usize| -> Result<usize, String> {
        zip.get(o..o + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]) as usize)
            .ok_or_else(|| "truncated archive".to_string())
    };
    let rd32 = |o: usize| -> Result<usize, String> {
        zip.get(o..o + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
            .ok_or_else(|| "truncated archive".to_string())
    };
    if zip.len() < 22 || rd32(zip.len() - 22)? != 0x0605_4b50 {
        return Err("no end-of-central-directory record".into());
    }
    let eocd = zip.len() - 22;
    let count = rd16(eocd + 10)?;
    let mut at = rd32(eocd + 16)?;
    let mut names = Vec::new();
    for _ in 0..count {
        if rd32(at)? != 0x0201_4b50 {
            return Err("bad central directory entry".into());
        }
        let crc = rd32(at + 16)? as u32;
        let size = rd32(at + 20)?;
        let name_len = rd16(at + 28)?;
        let local = rd32(at + 42)?;
        let name = String::from_utf8_lossy(
            zip.get(at + 46..at + 46 + name_len)
                .ok_or("truncated name")?,
        )
        .to_string();
        let data_at = local + 30 + rd16(local + 26)? + rd16(local + 28)?;
        let data = zip
            .get(data_at..data_at + size)
            .ok_or_else(|| format!("{name}: truncated data"))?;
        if crc32(data) != crc {
            return Err(format!("{name}: CRC mismatch"));
        }
        names.push(name);
        at += 46 + name_len;
    }
    Ok(names)
}

/// What each fabrication file is, for the pack README.
pub fn describe(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().unwrap_or("");
    match ext {
        "gtl" => "Top copper",
        "gbl" => "Bottom copper",
        "gts" => "Top soldermask (openings)",
        "gbs" => "Bottom soldermask (openings)",
        "gtp" => "Top paste (stencil)",
        "gbp" => "Bottom paste (stencil)",
        "gto" => "Top silkscreen (legend)",
        "gbo" => "Bottom silkscreen (legend)",
        "gm1" => "Board outline (Edge cuts)",
        "drl" if name.ends_with("-PTH.drl") => "Plated through holes (Excellon)",
        "drl" => "Blind/buried via holes for one layer span (Excellon)",
        e if e.starts_with('g') && e[1..].chars().all(|c| c.is_ascii_digit()) => {
            "Inner copper layer"
        }
        _ => "Other",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_matches_the_standard_check_value() {
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    }

    #[test]
    fn zip_round_trips_names_and_detects_corruption() {
        let z = zip(&[
            ("a.gtl".into(), b"G04 top*\nM02*\n".to_vec()),
            ("README.txt".into(), b"hello".to_vec()),
        ]);
        assert_eq!(list(&z).unwrap(), vec!["a.gtl", "README.txt"]);
        assert_eq!(
            z,
            zip(&[
                ("a.gtl".into(), b"G04 top*\nM02*\n".to_vec()),
                ("README.txt".into(), b"hello".to_vec()),
            ])
        );
        let mut bad = z.clone();
        bad[30 + 5] ^= 0xFF; // first byte of a.gtl's data
        assert!(list(&bad).unwrap_err().contains("CRC"));
        assert!(list(b"not a zip").is_err());
    }

    #[test]
    fn layer_descriptions_cover_generated_names() {
        assert_eq!(describe("b-F_Cu.gtl"), "Top copper");
        assert_eq!(describe("b-In1_Cu.g2"), "Inner copper layer");
        assert_eq!(describe("b-PTH.drl"), "Plated through holes (Excellon)");
        assert_eq!(describe("b-Edge_Cuts.gm1"), "Board outline (Edge cuts)");
    }
}
