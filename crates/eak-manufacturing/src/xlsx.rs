//! Minimal Office Open XML workbook (XLSX) writer: one sheet of inline-string and number
//! cells, packed with the crate's own stored ZIP writer ([`crate::fabpack::zip`]). No
//! third-party dependency; no styles, formulas or shared strings, which every reader treats
//! as optional. Text is never written as a formula, so a cell starting with `=` stays text.

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/><Override PartName="/xl/worksheets/sheet1.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml"/></Types>"#;

const ROOT_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#;

const WORKBOOK_RELS: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet" Target="worksheets/sheet1.xml"/></Relationships>"#;

/// Entry names every workbook written here contains.
pub const PARTS: [&str; 5] = [
    "[Content_Types].xml",
    "_rels/.rels",
    "xl/workbook.xml",
    "xl/_rels/workbook.xml.rels",
    "xl/worksheets/sheet1.xml",
];

/// XML text: escaped, with characters XML 1.0 forbids removed.
pub fn xml_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if (c as u32) < 0x20 || c == '\u{FFFE}' || c == '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
    out
}

/// Spreadsheet column name of a 0-based index: A, B, ..., Z, AA, AB, ...
pub fn column_name(mut i: usize) -> String {
    let mut name = Vec::new();
    loop {
        name.push(b'A' + (i % 26) as u8);
        if i < 26 {
            break;
        }
        i = i / 26 - 1;
    }
    name.reverse();
    String::from_utf8(name).unwrap_or_default()
}

/// A workbook with one sheet: `headers` in row 1, then `rows`. Cells of columns marked in
/// `numeric` are written as numbers when they parse as one, otherwise as text.
pub fn workbook(
    sheet: &str,
    headers: &[String],
    rows: &[Vec<String>],
    numeric: &[bool],
) -> Vec<u8> {
    let mut data = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>",
    );
    let all = std::iter::once((headers, false)).chain(rows.iter().map(|r| (r.as_slice(), true)));
    for (r, (cells, body)) in all.enumerate() {
        let row = r + 1;
        data.push_str(&format!("<row r=\"{row}\">"));
        for (c, value) in cells.iter().enumerate() {
            let at = format!("{}{row}", column_name(c));
            let number = body
                && numeric.get(c).copied().unwrap_or(false)
                && value.trim().parse::<f64>().is_ok_and(f64::is_finite);
            if number {
                data.push_str(&format!("<c r=\"{at}\"><v>{}</v></c>", value.trim()));
            } else {
                data.push_str(&format!(
                    "<c r=\"{at}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{}</t></is></c>",
                    xml_text(value)
                ));
            }
        }
        data.push_str("</row>");
    }
    data.push_str("</sheetData></worksheet>");
    let workbook = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"><sheets><sheet name=\"{}\" sheetId=\"1\" r:id=\"rId1\"/></sheets></workbook>",
        xml_text(sheet)
    );
    let parts: [&[u8]; 5] = [
        CONTENT_TYPES.as_bytes(),
        ROOT_RELS.as_bytes(),
        workbook.as_bytes(),
        WORKBOOK_RELS.as_bytes(),
        data.as_bytes(),
    ];
    let entries: Vec<(String, Vec<u8>)> = PARTS
        .iter()
        .zip(parts)
        .map(|(name, bytes)| (name.to_string(), bytes.to_vec()))
        .collect();
    crate::fabpack::zip(&entries)
}

/// Problems of a workbook written by [`workbook`] (empty when it reads back).
pub fn validate(bytes: &[u8]) -> Vec<String> {
    match crate::fabpack::list(bytes) {
        Err(e) => vec![e],
        Ok(names) => PARTS
            .iter()
            .filter(|p| !names.iter().any(|n| n == *p))
            .map(|p| format!("workbook part {p} is missing"))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn column_names_follow_spreadsheet_order() {
        assert_eq!(column_name(0), "A");
        assert_eq!(column_name(25), "Z");
        assert_eq!(column_name(26), "AA");
        assert_eq!(column_name(27), "AB");
        assert_eq!(column_name(701), "ZZ");
        assert_eq!(column_name(702), "AAA");
    }

    #[test]
    fn workbook_reads_back_with_escaped_text_and_numbers() {
        let w = workbook(
            "BOM",
            &["Comment".into(), "Quantity".into()],
            &[
                vec!["=1+1 <&>".into(), "3".into()],
                vec!["x\u{1}".into(), "n/a".into()],
            ],
            &[false, true],
        );
        assert!(validate(&w).is_empty(), "{:?}", validate(&w));
        let text = String::from_utf8_lossy(&w);
        assert!(text.contains("<c r=\"B2\"><v>3</v></c>"));
        assert!(text.contains("=1+1 &lt;&amp;&gt;"));
        assert!(text.contains("<c r=\"B3\" t=\"inlineStr\"><is><t xml:space=\"preserve\">n/a</t>"));
        assert!(!text.contains("x\u{1}"));
        assert!(!validate(b"not a zip").is_empty());
    }
}
