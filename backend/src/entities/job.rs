use serde::{Deserialize, Serialize};

/// Прогресс или конечное состояние одной задачи импорта для фронтенда.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub(crate) enum JobStatus {
    /// Pipeline сейчас выполняет один из этапов.
    Progress {
        /// Текущий этап pipeline.
        stage: JobStage,
        /// Количество строк, обработанных на текущем этапе.
        current: usize,
        /// Общее количество строк на текущем этапе.
        total: usize,
    },
    /// Все проверенные учётные записи студентов успешно созданы.
    Completed {
        /// Количество созданных учётных записей LDAP.
        created: usize,
        /// Общее количество подготовленных студентов.
        total: usize,
        /// Сформированный CSV с готовыми учётными данными.
        result: ResultReference,
    },
    /// Обработка остановлена до появления частичного результата записи в LDAP.
    Failed {
        /// Этап, на котором произошла ошибка.
        stage: JobStage,
        /// Стабильный машиночитаемый код ошибки.
        code: String,
        /// Понятное пользователю сообщение об ошибке.
        message: String,
        /// Номер исходной строки с единицы, если ошибка относится к строке.
        row: Option<usize>,
    },
    /// Ошибка LDAP после того, как одна или несколько учётных записей могли быть созданы.
    PartialFailure {
        /// Количество учётных записей, точно созданных до ошибки.
        created: usize,
        /// Общее количество подготовленных студентов.
        total: usize,
        /// Номер строки с единицы, на которой остановилось создание в LDAP.
        failed_row: usize,
        /// Полное ФИО студента, на котором произошла ошибка LDAP.
        failed_fio: String,
        /// Этап создания в LDAP, завершившийся ошибкой.
        ldap_phase: String,
        /// Может ли LDAP-объект проблемного студента уже существовать.
        possibly_created: bool,
        /// Полный итоговый CSV для проверки, повторного запуска или ручного отката.
        result: ResultReference,
    },
}

/// Именованный этап pipeline, передаваемый в событиях прогресса.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum JobStage {
    /// Получение и проверка multipart-загрузки.
    Uploading,
    /// Декодирование и чтение строк CSV.
    Parsing,
    /// Проверка исходных полей и уникальности внутри файла.
    Validating,
    /// Генерация транслитерированных логинов.
    Transliterating,
    /// Поиск существующих пользователей и логинов в LDAP.
    CheckingLdap,
    /// Вычисление временных паролей.
    GeneratingPasswords,
    /// Запись подготовленных учётных записей студентов в LDAP.
    CreatingAccounts,
}

/// Адрес сформированного файла результата для API и событий задачи.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ResultReference {
    /// `sAMAccountName` владельца каталога, содержащего результат.
    pub(crate) owner: String,
    /// Имя файла, сформированное из даты и времени создания.
    pub(crate) filename: String,
}
