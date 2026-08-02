use std::borrow::Cow;

use csv::{ReaderBuilder, StringRecord};
use encoding_rs::WINDOWS_1251;

use crate::{entities::import::StudentInput, errors::ImportError};

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";
const EXPECTED_HEADERS: [&str; 5] = ["First", "Last", "Patronymic", "Email", "Group"];

/// Декодирует UTF-8/Windows-1251 CSV и преобразует его строки в `StudentInput`.
pub(super) fn parse_csv(bytes: &[u8]) -> Result<Vec<StudentInput>, ImportError> {
    let decoded = decode_csv(bytes)?;
    let mut reader = ReaderBuilder::new()
        .has_headers(true)
        .flexible(false)
        .from_reader(decoded.as_bytes());

    let headers = reader.headers().map_err(|error| parse_error(1, error))?;
    validate_headers(headers)?;

    reader
        .records()
        .enumerate()
        .map(|(index, record)| {
            let source_row = index + 2;
            let record = record.map_err(|error| parse_error(source_row, error))?;
            student_from_record(source_row, &record)
        })
        .collect()
}

/// Предпочитает UTF-8, удаляя его BOM, и использует Windows-1251 как fallback.
fn decode_csv(bytes: &[u8]) -> Result<Cow<'_, str>, ImportError> {
    let bytes = bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes);

    let decoded = if let Ok(decoded) = std::str::from_utf8(bytes) {
        Cow::Borrowed(decoded)
    } else {
        WINDOWS_1251
            .decode_without_bom_handling_and_without_replacement(bytes)
            .filter(|decoded| !decoded.contains('\u{FFFD}'))
            .ok_or(ImportError::Decode)?
    };

    if decoded
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
    {
        return Err(ImportError::Decode);
    }

    Ok(decoded)
}

/// Требует точный набор и порядок колонок первой версии формата.
fn validate_headers(headers: &StringRecord) -> Result<(), ImportError> {
    if headers.iter().eq(EXPECTED_HEADERS) {
        return Ok(());
    }

    Err(ImportError::Parse {
        row: 1,
        message: format!(
            "expected header `{}`, got `{}`",
            EXPECTED_HEADERS.join(","),
            headers.iter().collect::<Vec<_>>().join(",")
        ),
    })
}

/// Преобразует проверенную пятиколоночную CSV-запись, убирая краевые пробелы.
fn student_from_record(
    source_row: usize,
    record: &StringRecord,
) -> Result<StudentInput, ImportError> {
    let field = |index: usize| {
        record
            .get(index)
            .map(str::trim)
            .map(str::to_owned)
            .ok_or_else(|| ImportError::Parse {
                row: source_row,
                message: format!("missing column `{}`", EXPECTED_HEADERS[index]),
            })
    };

    Ok(StudentInput {
        source_row,
        first_name: field(0)?,
        last_name: field(1)?,
        patronymic: field(2)?,
        email: field(3)?,
        group: field(4)?,
    })
}

/// Сохраняет номер проблемной строки и диагностическое сообщение csv crate.
fn parse_error(fallback_row: usize, error: csv::Error) -> ImportError {
    let row = error
        .position()
        .map(|position| position.line() as usize)
        .unwrap_or(fallback_row);

    ImportError::Parse {
        row,
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_utf8_csv_with_exact_headers() {
        let csv = concat!(
            "First,Last,Patronymic,Email,Group\n",
            "Иван,Иванов,Иванович,ivan@example.com,001\n",
            "Пётр,Петров,Петрович,petr@example.com,002\n",
        );

        let students = parse_csv(csv.as_bytes()).expect("корректный UTF-8 CSV должен читаться");

        assert_eq!(students.len(), 2);
        assert_eq!(students[0].source_row, 2);
        assert_eq!(students[0].first_name, "Иван");
        assert_eq!(students[0].last_name, "Иванов");
        assert_eq!(students[0].patronymic, "Иванович");
        assert_eq!(students[0].email, "ivan@example.com");
        assert_eq!(students[0].group, "001");
        assert_eq!(students[1].source_row, 3);
        assert_eq!(students[1].first_name, "Пётр");
    }

    #[test]
    fn parses_utf8_csv_with_bom() {
        let mut csv = UTF8_BOM.to_vec();
        csv.extend_from_slice(
            b"First,Last,Patronymic,Email,Group\nIvan,Ivanov,Ivanovich,test@example.com,101\n",
        );

        let students = parse_csv(&csv).expect("UTF-8 BOM должен поддерживаться");

        assert_eq!(students.len(), 1);
        assert_eq!(students[0].first_name, "Ivan");
    }

    #[test]
    fn parses_windows_1251_csv() {
        let csv = concat!(
            "First,Last,Patronymic,Email,Group\n",
            "Иван,Иванов,Иванович,ivan@example.com,101\n",
        );
        let (encoded, _, had_errors) = WINDOWS_1251.encode(csv);
        assert!(!had_errors);

        let students = parse_csv(&encoded).expect("Windows-1251 CSV должен читаться");

        assert_eq!(students.len(), 1);
        assert_eq!(students[0].first_name, "Иван");
        assert_eq!(students[0].last_name, "Иванов");
    }

    #[test]
    fn trims_fields_and_preserves_leading_zeroes_in_group() {
        let csv = concat!(
            "First,Last,Patronymic,Email,Group\n",
            "  Иван  , Иванов , Иванович , student@example.com , 00101 \n",
        );

        let students = parse_csv(csv.as_bytes()).expect("краевые пробелы допустимы");

        assert_eq!(students[0].first_name, "Иван");
        assert_eq!(students[0].last_name, "Иванов");
        assert_eq!(students[0].email, "student@example.com");
        assert_eq!(students[0].group, "00101");
    }

    #[test]
    fn rejects_wrong_or_reordered_headers() {
        let csv = "Last,First,Patronymic,Email,Group\nИванов,Иван,Иванович,a@b.c,101\n";

        let error = parse_csv(csv.as_bytes()).expect_err("порядок колонок должен быть точным");

        assert!(matches!(error, ImportError::Parse { row: 1, .. }));
    }

    #[test]
    fn rejects_record_with_wrong_number_of_fields() {
        let csv = concat!(
            "First,Last,Patronymic,Email,Group\n",
            "Иван,Иванов,Иванович,ivan@example.com\n",
        );

        let error = parse_csv(csv.as_bytes()).expect_err("неполная строка должна быть ошибкой");

        assert!(matches!(error, ImportError::Parse { row: 2, .. }));
    }

    #[test]
    fn rejects_bytes_invalid_for_utf8_and_windows_1251() {
        let error = parse_csv(&[0x98]).expect_err("недекодируемые байты должны быть ошибкой");

        assert!(matches!(error, ImportError::Decode));
    }
}
