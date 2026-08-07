use std::{borrow::Cow, io::Cursor};

use csv::{ReaderBuilder, StringRecord};
use encoding_rs::WINDOWS_1251;

use crate::{
    entities::import::{Groups, PreparedStudent, SecretString, StudentInput},
    errors::ImportError,
};

use super::credentials::normalize_conflict_login;

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";
const EXPECTED_HEADERS: [&str; 5] = ["First", "Last", "Patronymic", "Email", "Group"];
const EXPECTED_RESULT_HEADERS: [&str; 7] = [
    "First",
    "Last",
    "Patronymic",
    "Email",
    "Group",
    "Login",
    "Pass",
];

/// Минимальный набор данных итогового CSV, необходимый для рассылки.
#[derive(Debug)]
pub(crate) struct MailCredentialRow {
    /// Номер строки CSV с единицы для сопоставления результата.
    pub(crate) source_row: usize,
    /// Личная почта получателя.
    pub(crate) email: String,
    /// Логин созданной учётной записи.
    pub(crate) login: String,
    /// Временный пароль созданной учётной записи.
    pub(crate) password: String,
}

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

/// Читает ранее сформированный CSV с credentials для запуска LDAP-создания.
pub(crate) fn parse_result_csv(
    bytes: &[u8],
    groups: &Groups,
) -> Result<Vec<PreparedStudent>, ImportError> {
    let mut reader = result_csv_reader(bytes)?;

    reader
        .records()
        .enumerate()
        .map(|(index, record)| {
            let source_row = index + 2;
            let record = record.map_err(|error| parse_error(source_row, error))?;
            result_student_from_record(source_row, &record, groups)
        })
        .collect()
}

/// Читает из итогового CSV только поля, нужные для отправки письма.
pub(crate) fn parse_credentials_csv(bytes: &[u8]) -> Result<Vec<MailCredentialRow>, ImportError> {
    let mut reader = result_csv_reader(bytes)?;

    reader
        .records()
        .enumerate()
        .map(|(index, record)| {
            let source_row = index + 2;
            let record = record.map_err(|error| parse_error(source_row, error))?;
            let field = |column: usize| {
                record
                    .get(column)
                    .map(str::trim)
                    .map(str::to_owned)
                    .ok_or_else(|| ImportError::Parse {
                        row: source_row,
                        message: format!("missing column `{}`", EXPECTED_RESULT_HEADERS[column]),
                    })
            };
            let login = field(5)?;
            let password = field(6)?;
            if login.is_empty() || password.is_empty() {
                return Err(ImportError::Validation {
                    row: source_row,
                    message: "логин или пароль пустой".to_owned(),
                });
            }
            Ok(MailCredentialRow {
                source_row,
                email: field(3)?,
                login,
                password,
            })
        })
        .collect()
}

/// Декодирует итоговый CSV, создаёт reader и проверяет его заголовок.
fn result_csv_reader(bytes: &[u8]) -> Result<csv::Reader<Cursor<Vec<u8>>>, ImportError> {
    let decoded = decode_csv(bytes)?;
    let mut reader = ReaderBuilder::new()
        .has_headers(true)
        .flexible(false)
        .from_reader(Cursor::new(decoded.into_owned().into_bytes()));
    let headers = reader.headers().map_err(|error| parse_error(1, error))?;
    if !headers.iter().eq(EXPECTED_RESULT_HEADERS) {
        return Err(ImportError::Parse {
            row: 1,
            message: format!(
                "expected header `{}`, got `{}`",
                EXPECTED_RESULT_HEADERS.join(","),
                headers.iter().collect::<Vec<_>>().join(",")
            ),
        });
    }
    Ok(reader)
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

/// Преобразует строку итогового CSV в данные для LDAP-создания.
fn result_student_from_record(
    source_row: usize,
    record: &StringRecord,
    groups: &Groups,
) -> Result<PreparedStudent, ImportError> {
    let field = |index: usize| {
        record
            .get(index)
            .map(str::trim)
            .map(str::to_owned)
            .ok_or_else(|| ImportError::Parse {
                row: source_row,
                message: format!("missing column `{}`", EXPECTED_RESULT_HEADERS[index]),
            })
    };
    let source = StudentInput {
        source_row,
        first_name: field(0)?,
        last_name: field(1)?,
        patronymic: field(2)?,
        email: field(3)?,
        group: field(4)?,
    };
    let group_number = source
        .group
        .parse::<usize>()
        .map_err(|_| ImportError::Validation {
            row: source_row,
            message: "Номер группы должен быть целым положительным числом".to_owned(),
        })?;
    let group = groups
        .get(group_number)
        .map_err(|source| ImportError::UnsupportedGroup {
            row: source_row,
            source,
        })?;
    let login = normalize_conflict_login(source_row, &field(5)?)?;
    let password = field(6)?;
    if password.is_empty() {
        return Err(ImportError::Validation {
            row: source_row,
            message: "Пароль пустой".to_owned(),
        });
    }
    Ok(PreparedStudent {
        identity: crate::entities::import::PreparedIdentity {
            source,
            login,
            group: group.clone(),
        },
        password: SecretString::new(password),
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
    use std::collections::HashMap;

    use super::*;

    fn groups() -> Groups {
        Groups::new(HashMap::from([(
            151,
            crate::entities::import::Group::new(151, "ПИ".to_owned()),
        )]))
    }

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

    #[test]
    fn parses_stored_result_for_ldap_creation() {
        let csv = concat!(
            "First,Last,Patronymic,Email,Group,Login,Pass\n",
            "Иван,Иванов,Иванович,ivan@example.com,151,ivanovii,temporary-password\n",
        );

        let students = parse_result_csv(csv.as_bytes(), &groups())
            .expect("сохранённый результат должен быть пригоден для LDAP-создания");

        assert_eq!(students.len(), 1);
        assert_eq!(students[0].identity.login, "ivanovii");
        assert_eq!(students[0].identity.group.name(), "ПИ");
        assert_eq!(students[0].password.get(), "temporary-password");
    }
}
