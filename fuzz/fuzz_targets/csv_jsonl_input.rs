#![no_main]

use canon::{InputFormat, InputValues, SpecialReason, input::parse_input};
use libfuzzer_sys::fuzz_target;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

const COLUMN: &str = "id";
const MAX_BYTES: usize = 16 * 1024;
const MAX_ROWS: usize = 256;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_BYTES {
        return;
    }

    let csv_payload = build_csv_payload(data);
    let jsonl_payload = build_jsonl_payload(data);
    exercise_input("csv", &csv_payload);
    exercise_input("jsonl", &jsonl_payload);
});

fn exercise_input(extension: &str, payload: &[u8]) {
    let digest = blake3::hash(payload).to_hex().to_string();
    let path = fuzz_root().join(format!("canon-fuzz-{}.{extension}", &digest[..16]));
    fs::write(&path, payload).expect("write fuzz input");

    let first = parse_input(
        Path::new(&path),
        COLUMN,
        Some(MAX_BYTES as u64 + 1),
        Some(MAX_ROWS),
    );
    let second = parse_input(
        Path::new(&path),
        COLUMN,
        Some(MAX_BYTES as u64 + 1),
        Some(MAX_ROWS),
    );

    match (first, second) {
        (Ok(left), Ok(right)) => assert_eq!(
            normalize_input_values(&left),
            normalize_input_values(&right)
        ),
        (Err(left), Err(right)) => assert_eq!(left.to_string(), right.to_string()),
        (left, right) => {
            panic!("parse_input changed result across identical runs: {left:?} vs {right:?}")
        }
    }

    let _ = fs::remove_file(path);
}

fn build_csv_payload(data: &[u8]) -> Vec<u8> {
    let mut output = Vec::from(b"id,note\n".as_slice());
    let mut chunks = data.chunks(4).peekable();
    let mut row_index = 0usize;

    while let Some(chunk) = chunks.next() {
        let variant = chunk[0] % 6;
        let token = ascii_token(&chunk[1..], row_index);
        let row = match variant {
            0 => format!("{token},resolved-{row_index}\n"),
            1 => format!("  {token}  ,trimmed-{row_index}\n"),
            2 => format!(",non-empty-note-{row_index}\n"),
            3 => ",\n".to_string(),
            4 => format!("\"{token},quoted\",quoted-{row_index}\n"),
            _ => format!("DUPLICATE,dup-{row_index}\n"),
        };
        output.extend_from_slice(row.as_bytes());
        row_index += 1;
        if row_index >= MAX_ROWS || chunks.peek().is_none() {
            break;
        }
    }

    output
}

fn build_jsonl_payload(data: &[u8]) -> Vec<u8> {
    let mut output = Vec::new();
    let mut row_index = 0usize;

    for chunk in data.chunks(3) {
        let variant = chunk[0] % 7;
        let token = ascii_token(&chunk[1..], row_index);
        let line = match variant {
            0 => format!(r#"{{"{COLUMN}":"{token}","row":{row_index}}}"#),
            1 => format!(r#"{{"{COLUMN}":"  {token}  "}}"#),
            2 => format!(r#"{{"{COLUMN}":null}}"#),
            3 => r#"{"other":"missing"}"#.to_string(),
            4 => format!(r#"{{"{COLUMN}":[1,{row_index},3]}}"#),
            5 => format!(r#"{{"{COLUMN}":{{"nested":"{token}"}}}}"#),
            _ => format!(r#"{{"{COLUMN}":{row_index}}}"#),
        };
        output.extend_from_slice(line.as_bytes());
        output.push(b'\n');
        row_index += 1;
        if row_index >= MAX_ROWS {
            break;
        }
    }

    output
}

fn ascii_token(bytes: &[u8], row_index: usize) -> String {
    let mut token = String::new();
    for byte in bytes {
        let normalized = match byte % 8 {
            0 => char::from(b'A' + (byte % 26)),
            1 => char::from(b'a' + (byte % 26)),
            2 => char::from(b'0' + (byte % 10)),
            3 => '-',
            4 => '_',
            5 => ' ',
            6 => '.',
            _ => '/',
        };
        token.push(normalized);
    }
    let trimmed = token.trim();
    if trimmed.is_empty() {
        format!("ROW_{row_index}")
    } else {
        trimmed.to_string()
    }
}

fn normalize_input_values(
    values: &InputValues,
) -> (
    InputFormat,
    Option<u8>,
    Option<String>,
    Option<u64>,
    BTreeSet<String>,
    BTreeMap<String, usize>,
) {
    let raw_values = values.values.keys().cloned().collect::<BTreeSet<_>>();
    let specials = values
        .special
        .iter()
        .map(|(reason, count)| (special_reason_name(reason), *count))
        .collect::<BTreeMap<_, _>>();
    (
        values.format.clone(),
        values.delimiter,
        values.source_hash.clone(),
        values.source_bytes,
        raw_values,
        specials,
    )
}

fn special_reason_name(reason: &SpecialReason) -> String {
    match reason {
        SpecialReason::EmptyValue => "empty_value".to_string(),
        SpecialReason::NullValue => "null_value".to_string(),
        SpecialReason::MissingField => "missing_field".to_string(),
        SpecialReason::NonScalarValue => "non_scalar_value".to_string(),
    }
}

fn fuzz_root() -> &'static PathBuf {
    static ROOT: OnceLock<PathBuf> = OnceLock::new();
    ROOT.get_or_init(|| {
        let root = std::env::temp_dir().join(format!("canon-fuzz-{}", std::process::id()));
        fs::create_dir_all(&root).expect("create fuzz root");
        root
    })
}
