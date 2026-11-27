//! Lookup over Parquet and XLSX input (bd-ko97). The files are written here,
//! so what each test reads is visible in the test.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use parquet::data_type::{ByteArray, ByteArrayType};
use parquet::file::properties::WriterProperties;
use parquet::file::writer::SerializedFileWriter;
use parquet::schema::parser::parse_message_type;
use serde_json::Value;
use tempfile::TempDir;
use zip::write::SimpleFileOptions;

const REGISTRY: &str = "tests/fixtures/registries/cusip-isin";

/// A leading zero, surrounding spaces, a repeat, a miss, an empty lookup cell in
/// a row that has other content, and a row with nothing in it.
const ROWS: [[&str; 2]; 6] = [
    ["037833100", "a"],
    [" 594918104 ", "b"],
    ["037833100", "c"],
    ["NOT-HELD", "d"],
    // Not last: a worksheet cannot hold a trailing empty row.
    ["", ""],
    ["", "e"],
];

fn write_csv(path: &Path, rows: &[[&str; 2]]) {
    let mut writer = csv::Writer::from_path(path).unwrap();
    writer.write_record(["cusip", "note"]).unwrap();
    for row in rows {
        writer.write_record(row).unwrap();
    }
    writer.flush().unwrap();
}

fn write_parquet(path: &Path, rows: &[[&str; 2]]) {
    let schema = parse_message_type(
        "message input { REQUIRED BINARY cusip (UTF8); REQUIRED BINARY note (UTF8); }",
    )
    .unwrap();
    let mut writer = SerializedFileWriter::new(
        File::create(path).unwrap(),
        Arc::new(schema),
        Arc::new(WriterProperties::builder().build()),
    )
    .unwrap();
    let mut row_group = writer.next_row_group().unwrap();
    for column_index in 0..2 {
        let mut column = row_group.next_column().unwrap().unwrap();
        let values: Vec<ByteArray> = rows
            .iter()
            .map(|row| ByteArray::from(row[column_index]))
            .collect();
        column
            .typed::<ByteArrayType>()
            .write_batch(&values, None, None)
            .unwrap();
        column.close().unwrap();
    }
    row_group.close().unwrap();
    writer.close().unwrap();
}

enum XlsxCell<'a> {
    Text(&'a str),
    Number(&'a str),
}

fn sheet_xml(rows: &[Vec<XlsxCell<'_>>]) -> String {
    let mut data = String::new();
    for (row_index, row) in rows.iter().enumerate() {
        let number = row_index + 1;
        data.push_str(&format!("<row r=\"{number}\">"));
        for (column_index, cell) in row.iter().enumerate() {
            let reference = format!("{}{number}", char::from(b'A' + column_index as u8));
            match cell {
                XlsxCell::Text("") => {}
                XlsxCell::Text(text) => data.push_str(&format!(
                    "<c r=\"{reference}\" t=\"inlineStr\"><is><t xml:space=\"preserve\">{text}</t></is></c>"
                )),
                XlsxCell::Number(value) => {
                    data.push_str(&format!("<c r=\"{reference}\"><v>{value}</v></c>"))
                }
            }
        }
        data.push_str("</row>");
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?><worksheet xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\"><sheetData>{data}</sheetData></worksheet>"
    )
}

/// The smallest workbook package the reader accepts, with inline strings.
fn write_xlsx(path: &Path, sheets: &[(&str, Vec<Vec<XlsxCell<'_>>>)]) {
    const DECLARATION: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>";
    const RELATIONSHIPS: &str =
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
    const PACKAGE: &str = "http://schemas.openxmlformats.org/package/2006";

    let mut parts = vec![(
        "_rels/.rels".to_string(),
        format!(
            "{DECLARATION}<Relationships xmlns=\"{PACKAGE}/relationships\"><Relationship Id=\"rId1\" Type=\"{RELATIONSHIPS}/officeDocument\" Target=\"xl/workbook.xml\"/></Relationships>"
        ),
    )];
    let mut overrides = String::new();
    let mut entries = String::new();
    let mut relationships = String::new();
    for (index, (name, rows)) in sheets.iter().enumerate() {
        let id = index + 1;
        overrides.push_str(&format!(
            "<Override PartName=\"/xl/worksheets/sheet{id}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>"
        ));
        entries.push_str(&format!(
            "<sheet name=\"{name}\" sheetId=\"{id}\" r:id=\"rId{id}\"/>"
        ));
        relationships.push_str(&format!(
            "<Relationship Id=\"rId{id}\" Type=\"{RELATIONSHIPS}/worksheet\" Target=\"worksheets/sheet{id}.xml\"/>"
        ));
        parts.push((format!("xl/worksheets/sheet{id}.xml"), sheet_xml(rows)));
    }
    parts.push((
        "[Content_Types].xml".to_string(),
        format!(
            "{DECLARATION}<Types xmlns=\"{PACKAGE}/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Override PartName=\"/xl/workbook.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml\"/>{overrides}</Types>"
        ),
    ));
    parts.push((
        "xl/workbook.xml".to_string(),
        format!(
            "{DECLARATION}<workbook xmlns=\"http://schemas.openxmlformats.org/spreadsheetml/2006/main\" xmlns:r=\"{RELATIONSHIPS}\"><sheets>{entries}</sheets></workbook>"
        ),
    ));
    parts.push((
        "xl/_rels/workbook.xml.rels".to_string(),
        format!(
            "{DECLARATION}<Relationships xmlns=\"{PACKAGE}/relationships\">{relationships}</Relationships>"
        ),
    ));

    let mut archive = zip::ZipWriter::new(File::create(path).unwrap());
    for (name, content) in parts {
        archive
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        archive.write_all(content.as_bytes()).unwrap();
    }
    archive.finish().unwrap();
}

fn text_sheet<'a>(rows: &[[&'a str; 2]]) -> Vec<Vec<XlsxCell<'a>>> {
    let mut sheet = vec![vec![XlsxCell::Text("cusip"), XlsxCell::Text("note")]];
    for row in rows {
        sheet.push(row.iter().map(|text| XlsxCell::Text(text)).collect());
    }
    sheet
}

/// Exit code and stdout of one lookup.
fn lookup(input: &Path, extra: &[&str]) -> (i32, String) {
    let output = Command::new(env!("CARGO_BIN_EXE_canon"))
        .arg(input)
        .args(["--registry", REGISTRY, "--column", "cusip", "--no-witness"])
        .args(extra)
        .output()
        .unwrap();
    (
        output.status.code().unwrap(),
        String::from_utf8(output.stdout).unwrap(),
    )
}

fn refusal(input: &Path, extra: &[&str]) -> Value {
    let (code, stdout) = lookup(input, extra);
    assert_eq!(code, 2, "{stdout}");
    assert!(!stdout.is_empty(), "no refusal on stdout for {extra:?}");
    serde_json::from_str::<Value>(&stdout).unwrap()["refusal"].clone()
}

fn three_formats(dir: &TempDir) -> [PathBuf; 3] {
    let csv = dir.path().join("input.csv");
    let parquet = dir.path().join("input.parquet");
    let xlsx = dir.path().join("input.xlsx");
    write_csv(&csv, &ROWS);
    write_parquet(&parquet, &ROWS);
    write_xlsx(&xlsx, &[("data", text_sheet(&ROWS))]);
    [csv, parquet, xlsx]
}

#[test]
fn the_same_table_gives_the_same_bytes_in_every_format() {
    let dir = TempDir::new().unwrap();
    let [csv, parquet, xlsx] = three_formats(&dir);

    for extra in [&["--json"][..], &["--explicit", "--plain-json-values"]] {
        let expected = lookup(&csv, extra);
        // Not every value resolves, so the lookup is PARTIAL.
        assert_eq!(expected.0, 1, "{}", expected.1);
        assert_eq!(lookup(&parquet, extra), expected);
        assert_eq!(lookup(&xlsx, extra), expected);
        assert_eq!(lookup(&parquet, extra), expected, "a second run differs");
    }

    let result: Value = serde_json::from_str(&lookup(&parquet, &["--json"]).1).unwrap();
    assert_eq!(result["summary"]["resolved"], 2, "{result}");
    assert_eq!(result["summary"]["unresolved"], 2, "{result}");
}

#[test]
fn a_whole_number_cell_matches_its_text_form() {
    let dir = TempDir::new().unwrap();
    let csv = dir.path().join("input.csv");
    let xlsx = dir.path().join("input.xlsx");
    write_csv(&csv, &[["594918104", "a"]]);
    write_xlsx(
        &xlsx,
        &[(
            "data",
            vec![
                vec![XlsxCell::Text("cusip"), XlsxCell::Text("note")],
                vec![XlsxCell::Number("594918104"), XlsxCell::Text("a")],
            ],
        )],
    );
    let expected = lookup(&csv, &["--json"]);
    assert_eq!(expected.0, 0, "{}", expected.1);
    assert_eq!(lookup(&xlsx, &["--json"]), expected);
}

#[test]
fn sheet_selects_a_worksheet_by_name_or_index() {
    let dir = TempDir::new().unwrap();
    let xlsx = dir.path().join("two.xlsx");
    write_xlsx(
        &xlsx,
        &[
            ("notes", text_sheet(&[["NOT-HELD", "x"]])),
            ("loans", text_sheet(&[["037833100", "y"]])),
        ],
    );

    for (selector, code) in [("loans", 0), ("1", 0), ("notes", 1), ("0", 1)] {
        // `notes` resolves nothing: exit 1, without being a refusal.
        let (actual, stdout) = lookup(&xlsx, &["--sheet", selector, "--json"]);
        assert_eq!(actual, code, "--sheet {selector}: {stdout}");
        let result: Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(
            result["summary"]["resolved"],
            i32::from(code == 0),
            "--sheet {selector}: {result}"
        );
    }

    for (extra, reader_code) in [
        (&["--json"][..], "E_AMBIGUOUS_SHEET"),
        (&["--sheet", "absent"], "E_SHEET_NOT_FOUND"),
        (&["--sheet", "2"], "E_SHEET_NOT_FOUND"),
    ] {
        let refusal = refusal(&xlsx, extra);
        assert_eq!(refusal["code"], "E_PARSE", "{refusal}");
        assert!(refusal.to_string().contains(reader_code), "{refusal}");
    }
}

#[test]
fn sheet_is_refused_for_input_that_has_no_worksheets() {
    let dir = TempDir::new().unwrap();
    let [csv, parquet, _] = three_formats(&dir);
    for input in [csv, parquet] {
        let refusal = refusal(&input, &["--sheet", "0"]);
        assert_eq!(refusal["code"], "E_PARSE", "{refusal}");
        assert!(
            refusal
                .to_string()
                .contains("--sheet applies only to .xlsx"),
            "{refusal}"
        );
    }
}

#[test]
fn limits_and_refusals_hold_for_parquet_and_xlsx() {
    let dir = TempDir::new().unwrap();
    let [csv, parquet, xlsx] = three_formats(&dir);

    for (input, name) in [(&parquet, "parquet"), (&xlsx, "xlsx")] {
        // The row limit bites where it does for CSV.
        for limit in ["4", "5", "6"] {
            let extra = ["--max-rows", limit, "--json"];
            assert_eq!(
                lookup(input, &extra),
                lookup(&csv, &extra),
                "{name} --max-rows {limit}"
            );
        }
        assert_eq!(lookup(input, &["--max-rows", "6", "--json"]).0, 1);
        assert_eq!(
            refusal(input, &["--max-rows", "4"])["code"],
            "E_TOO_LARGE",
            "{name}"
        );
        assert_eq!(
            refusal(input, &["--max-bytes", "10"])["code"],
            "E_TOO_LARGE",
            "{name}"
        );

        // With `--emit csv`, refusals go to stderr and stdout stays empty.
        let output = Command::new(env!("CARGO_BIN_EXE_canon"))
            .arg(input)
            .args(["--registry", REGISTRY, "--column", "cusip", "--no-witness"])
            .args(["--emit", "csv"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2), "{name}");
        assert!(output.stdout.is_empty(), "{name}");
        let emit: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(emit["refusal"]["code"], "E_EMIT_FORMAT", "{name}: {emit}");
        assert_eq!(emit["refusal"]["detail"]["input_format"], name, "{emit}");

        let output = Command::new(env!("CARGO_BIN_EXE_canon"))
            .arg(input)
            .args(["--registry", REGISTRY, "--column", "absent", "--no-witness"])
            .output()
            .unwrap();
        let missing: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(missing["refusal"]["code"], "E_COLUMN_NOT_FOUND", "{name}");

        let broken = dir.path().join(format!("broken.{name}"));
        let bytes = std::fs::read(input).unwrap();
        std::fs::write(&broken, &bytes[..bytes.len() / 2]).unwrap();
        assert_eq!(refusal(&broken, &["--json"])["code"], "E_PARSE", "{name}");
    }
}

/// The shared reader can switch `serde_json/preserve_order` on for the whole
/// build, which would reorder the keys in canon's own output. Keep it off.
#[test]
fn canon_json_keys_stay_sorted() {
    let value: Value = serde_json::from_str(r#"{"b":1,"a":2}"#).unwrap();
    assert_eq!(value.to_string(), r#"{"a":2,"b":1}"#);
}
