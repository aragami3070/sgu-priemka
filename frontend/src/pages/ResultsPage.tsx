import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Download,
  FileClock,
  Mail,
  RefreshCw,
  RotateCcw,
  Trash2,
  UserPlus,
  UserX,
} from "lucide-react";
import {
  Alert,
  AlertTitle,
  Backdrop,
  Box,
  Button,
  CircularProgress,
  Dialog,
  DialogActions,
  DialogContent,
  DialogContentText,
  DialogTitle,
  IconButton,
  LinearProgress,
  Snackbar,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  TextField,
  Tooltip,
  Typography,
} from "@mui/material";
import {
  createAccountsFromResult,
  deleteAccountsFromResult,
  deleteResult,
  downloadResult,
  getResultErrorMessage,
  listResults,
  sendCredentialsFromResult,
} from "../api/results";
import type { ResultItem } from "../api/results";
import { openImportEvents } from "../api/imports";
import type { JobStatus } from "../api/imports";
import { LoginConflictResolver } from "../components/LoginConflictResolver";

function resultKey(result: ResultItem): string {
  return `${result.owner}/${result.filename}`;
}

function formatCreatedAt(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat("ru-RU", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} Б`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} КБ`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} МБ`;
}

export function ResultsPage() {
  const [results, setResults] = useState<ResultItem[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [dateFrom, setDateFrom] = useState("");
  const [dateTo, setDateTo] = useState("");
  const [downloadingKey, setDownloadingKey] = useState<string | null>(null);
  const [deletingKey, setDeletingKey] = useState<string | null>(null);
  const [deleteCandidate, setDeleteCandidate] = useState<ResultItem | null>(
    null,
  );
  const [deleteAccountsCandidate, setDeleteAccountsCandidate] =
    useState<ResultItem | null>(null);
  const [creatingKey, setCreatingKey] = useState<string | null>(null);
  const [deletingAccountsKey, setDeletingAccountsKey] = useState<string | null>(
    null,
  );
  const [creationStatus, setCreationStatus] = useState<JobStatus | null>(null);
  const [creationSuccess, setCreationSuccess] = useState<string | null>(null);
  const [deletionSuccess, setDeletionSuccess] = useState<string | null>(null);
  const [mailingKey, setMailingKey] = useState<string | null>(null);
  const [mailStatus, setMailStatus] = useState<JobStatus | null>(null);
  const [mailSuccess, setMailSuccess] = useState<string | null>(null);
  const creationSocketRef = useRef<WebSocket | null>(null);
  const deletionSocketRef = useRef<WebSocket | null>(null);
  const mailSocketRef = useRef<WebSocket | null>(null);

  const loadResults = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    try {
      setResults(await listResults());
    } catch (requestError) {
      setError(
        getResultErrorMessage(
          requestError,
          "Не удалось получить список готовых CSV.",
        ),
      );
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadResults();
  }, [loadResults]);

  useEffect(() => {
    return () => {
      creationSocketRef.current?.close();
      creationSocketRef.current = null;
      deletionSocketRef.current?.close();
      deletionSocketRef.current = null;
      mailSocketRef.current?.close();
      mailSocketRef.current = null;
    };
  }, []);

  const filteredResults = useMemo(() => {
    const from = dateFrom ? new Date(`${dateFrom}T00:00:00`) : null;
    const to = dateTo ? new Date(`${dateTo}T23:59:59.999`) : null;
    return results.filter((result) => {
      const createdAt = new Date(result.created_at);
      if (Number.isNaN(createdAt.getTime())) return true;
      return (!from || createdAt >= from) && (!to || createdAt <= to);
    });
  }, [dateFrom, dateTo, results]);

  const handleDownload = async (result: ResultItem) => {
    const key = resultKey(result);
    setDownloadingKey(key);
    setError(null);
    try {
      await downloadResult(result);
    } catch (requestError) {
      setError(
        getResultErrorMessage(
          requestError,
          "Не удалось скачать выбранный CSV.",
        ),
      );
    } finally {
      setDownloadingKey(null);
    }
  };

  const handleDelete = async () => {
    if (!deleteCandidate) return;
    const candidate = deleteCandidate;
    const key = resultKey(candidate);
    setDeletingKey(key);
    setError(null);
    try {
      await deleteResult(candidate);
      setResults((current) =>
        current.filter((result) => resultKey(result) !== key),
      );
      setDeleteCandidate(null);
    } catch (requestError) {
      setError(
        getResultErrorMessage(
          requestError,
          "Не удалось удалить выбранный CSV.",
        ),
      );
    } finally {
      setDeletingKey(null);
    }
  };

  const handleCreateAccounts = async (result: ResultItem) => {
    const key = resultKey(result);
    if (creatingKey) return;
    setCreatingKey(key);
    setCreationStatus(null);
    setCreationSuccess(null);
    setError(null);
    const previousCreationSocket = creationSocketRef.current;
    creationSocketRef.current = null;
    previousCreationSocket?.close();
    try {
      const { job_id: jobId } = await createAccountsFromResult(result);
      const socket = openImportEvents(jobId);
      creationSocketRef.current = socket;
      let terminalReceived = false;
      socket.onmessage = (event) => {
        try {
          const nextStatus = JSON.parse(String(event.data)) as JobStatus;
          setCreationStatus(nextStatus);
          if (
            nextStatus.type === "completed" ||
            nextStatus.type === "failed" ||
            nextStatus.type === "partial_failure"
          ) {
            terminalReceived = true;
            setCreatingKey(null);
            if (nextStatus.type === "completed") {
              const count = nextStatus.created;
              setCreationSuccess(
                `Пользователи успешно созданы: ${count} ${
                  count === 1
                    ? "учётная запись"
                    : count >= 2 && count <= 4
                      ? "учётные записи"
                      : "учётных записей"
                }.`,
              );
            }
            if (nextStatus.type === "failed") setError(nextStatus.message);
            if (nextStatus.type === "partial_failure") {
              setError(
                `Создание остановлено на строке ${nextStatus.failed_row}.`,
              );
            }
          }
        } catch {
          setError("Backend прислал некорректный статус задания.");
          terminalReceived = true;
          socket.close();
        }
      };
      socket.onclose = (event) => {
        // Backend отправляет терминальный статус и сразу после него Close.
        // Откладываем проверку на следующий tick, чтобы последний message
        // успел обработаться даже если браузер доставит close-событие первым.
        window.setTimeout(() => {
          if (creationSocketRef.current !== socket) return;
          if (!terminalReceived && event.code !== 1000) {
            setError("Связь с задачей создания пользователей потеряна.");
          }
          if (!terminalReceived && event.code === 1000) {
            setCreationSuccess("Пользователи успешно созданы.");
          }
          setCreatingKey(null);
          creationSocketRef.current = null;
        }, 0);
      };
    } catch (requestError) {
      setError(
        getResultErrorMessage(
          requestError,
          "Не удалось запустить создание пользователей.",
        ),
      );
      setCreatingKey(null);
    }
  };

  const handleDeleteAccounts = async () => {
    if (!deleteAccountsCandidate || creatingKey || deletingAccountsKey) return;
    const candidate = deleteAccountsCandidate;
    const key = resultKey(candidate);
    setDeletingAccountsKey(key);
    setDeletionSuccess(null);
    setError(null);
    setDeleteAccountsCandidate(null);
    const previousDeletionSocket = deletionSocketRef.current;
    deletionSocketRef.current = null;
    previousDeletionSocket?.close();
    try {
      const { job_id: jobId } = await deleteAccountsFromResult(candidate);
      const socket = openImportEvents(jobId);
      deletionSocketRef.current = socket;
      let terminalReceived = false;
      socket.onmessage = (event) => {
        try {
          const nextStatus = JSON.parse(String(event.data)) as JobStatus;
          if (nextStatus.type === "progress") {
            setCreationStatus(nextStatus);
          }
          if (
            nextStatus.type === "deleted" ||
            // Совместимость с backend, собранным до появления отдельного
            // статуса `deleted`: старый сервер возвращал `completed`.
            nextStatus.type === "completed"
          ) {
            terminalReceived = true;
            setDeletingAccountsKey(null);
            setCreationStatus(null);
            const count =
              nextStatus.type === "deleted"
                ? nextStatus.deleted
                : nextStatus.created;
            setDeletionSuccess(
              `Пользователи успешно удалены: ${count} ${
                count === 1
                  ? "учётная запись"
                  : count >= 2 && count <= 4
                    ? "учётные записи"
                    : "учётных записей"
              }.`,
            );
          } else if (nextStatus.type === "failed") {
            terminalReceived = true;
            setDeletingAccountsKey(null);
            setCreationStatus(null);
            setError(nextStatus.message);
          } else if (nextStatus.type === "partial_failure") {
            terminalReceived = true;
            setDeletingAccountsKey(null);
            setCreationStatus(null);
            setError(
              `Удаление остановлено на строке ${nextStatus.failed_row}.`,
            );
          }
        } catch {
          setError("Backend прислал некорректный статус задания удаления.");
          socket.close();
        }
      };
      socket.onclose = () => {
        if (deletionSocketRef.current !== socket) return;
        if (!terminalReceived) {
          setError("Связь с задачей удаления пользователей потеряна.");
          setDeletingAccountsKey(null);
          setCreationStatus(null);
        }
        deletionSocketRef.current = null;
      };
    } catch (requestError) {
      setError(
        getResultErrorMessage(
          requestError,
          "Не удалось запустить удаление пользователей.",
        ),
      );
      setDeletingAccountsKey(null);
    }
  };

  const handleSendCredentials = async (result: ResultItem) => {
    const key = resultKey(result);
    if (mailingKey || creatingKey || deletingAccountsKey) return;
    setMailingKey(key);
    setMailStatus(null);
    setMailSuccess(null);
    setError(null);
    const previousSocket = mailSocketRef.current;
    mailSocketRef.current = null;
    previousSocket?.close();
    try {
      const { job_id: jobId } = await sendCredentialsFromResult(result);
      const socket = openImportEvents(jobId);
      mailSocketRef.current = socket;
      let terminalReceived = false;
      socket.onmessage = (event) => {
        try {
          const nextStatus = JSON.parse(String(event.data)) as JobStatus;
          if (nextStatus.type === "mail_progress") {
            setMailStatus(nextStatus);
          } else if (nextStatus.type === "mail_completed") {
            terminalReceived = true;
            setMailingKey(null);
            setMailStatus(null);
            setMailSuccess(
              `Рассылка завершена: принято ${nextStatus.accepted}, ошибок ${nextStatus.failed}.`,
            );
          } else if (nextStatus.type === "failed") {
            terminalReceived = true;
            setMailingKey(null);
            setMailStatus(null);
            setError(nextStatus.message);
          }
        } catch {
          terminalReceived = true;
          setMailingKey(null);
          setMailStatus(null);
          setError("Backend прислал некорректный статус рассылки.");
          socket.close();
        }
      };
      socket.onclose = (event) => {
        window.setTimeout(() => {
          if (mailSocketRef.current !== socket) return;
          if (!terminalReceived && event.code !== 1000) {
            setError("Связь с задачей рассылки потеряна.");
          }
          setMailingKey(null);
          setMailStatus(null);
          mailSocketRef.current = null;
        }, 0);
      };
    } catch (requestError) {
      setError(
        getResultErrorMessage(
          requestError,
          "Не удалось запустить рассылку учётных данных.",
        ),
      );
      setMailingKey(null);
    }
  };

  const creationConflictStatus =
    creationStatus?.type === "awaiting_login_resolutions"
      ? creationStatus
      : null;

  // --- Вычисление прогресса создания/удаления аккаунтов ---
  const accountProgress =
    creationStatus?.type === "progress" ? creationStatus : null;
  const accountProgressValue =
    accountProgress && accountProgress.total > 0
      ? Math.min(100, (accountProgress.current / accountProgress.total) * 100)
      : undefined;
  const isAccountOperationActive =
    (creatingKey !== null || deletingAccountsKey !== null) &&
    creationConflictStatus === null;

  // --- Вычисление прогресса рассылки ---
  const mailProgressData =
    mailStatus?.type === "mail_progress" ? mailStatus : null;
  const mailProgressValue =
    mailProgressData && mailProgressData.total > 0
      ? Math.min(
          100,
          (mailProgressData.current / mailProgressData.total) * 100,
        )
      : undefined;

  return (
    <Box className="page-section">
      <Box className="page-heading">
        <Box>
          <Typography component="h1" variant="h1">
            Готовые CSV
          </Typography>
          <Typography color="text.secondary">
            Результаты, доступные для повторного скачивания
          </Typography>
        </Box>
        <Tooltip title="Обновить список">
          <IconButton
            aria-label="Обновить список"
            disabled={isLoading}
            onClick={() => void loadResults()}
          >
            <RefreshCw size={19} />
          </IconButton>
        </Tooltip>
      </Box>

      {error && (
        <Alert severity="error">
          <AlertTitle>Ошибка работы с результатами</AlertTitle>
          {error}
        </Alert>
      )}

      <Box className="results-filters">
        <TextField
          label="Дата с"
          type="date"
          value={dateFrom}
          onChange={(event) => setDateFrom(event.target.value)}
          slotProps={{ inputLabel: { shrink: true } }}
        />
        <TextField
          label="Дата по"
          type="date"
          value={dateTo}
          onChange={(event) => setDateTo(event.target.value)}
          slotProps={{ inputLabel: { shrink: true } }}
        />
        <Button
          color="inherit"
          disabled={!dateFrom && !dateTo}
          startIcon={<RotateCcw size={17} />}
          onClick={() => {
            setDateFrom("");
            setDateTo("");
          }}
        >
          Сбросить
        </Button>
      </Box>

      <TableContainer className="results-table">
        <Table aria-label="Список готовых CSV">
          <TableHead>
            <TableRow>
              <TableCell>Администратор</TableCell>
              <TableCell>Файл</TableCell>
              <TableCell>Создан</TableCell>
              <TableCell align="right">Размер</TableCell>
              <TableCell align="right" aria-label="Действия" />
            </TableRow>
          </TableHead>
          <TableBody>
            {isLoading ? (
              <TableRow>
                <TableCell colSpan={5} className="results-empty">
                  <Box className="results-empty__content">
                    <CircularProgress size={30} />
                    <Typography color="text.secondary" variant="body2">
                      Загружаем список…
                    </Typography>
                  </Box>
                </TableCell>
              </TableRow>
            ) : filteredResults.length === 0 ? (
              <TableRow>
                <TableCell colSpan={5} className="results-empty">
                  <Box className="results-empty__content">
                    <FileClock size={30} strokeWidth={1.6} aria-hidden="true" />
                    <Typography sx={{ fontWeight: 650 }}>
                      {results.length === 0
                        ? "Готовых файлов пока нет"
                        : "За выбранный период файлов нет"}
                    </Typography>
                    <Typography color="text.secondary" variant="body2">
                      {results.length === 0
                        ? "Они появятся здесь после обработки импорта"
                        : "Измените даты или сбросьте фильтр"}
                    </Typography>
                  </Box>
                </TableCell>
              </TableRow>
            ) : (
              filteredResults.map((result) => {
                const key = resultKey(result);
                const isDownloading = downloadingKey === key;
                const isDeleting = deletingKey === key;
                const isCreating = creatingKey === key;
                const isDeletingAccounts = deletingAccountsKey === key;
                const isMailing = mailingKey === key;
                return (
                  <TableRow key={key} hover>
                    <TableCell>{result.owner}</TableCell>
                    <TableCell>{result.filename}</TableCell>
                    <TableCell>{formatCreatedAt(result.created_at)}</TableCell>
                    <TableCell align="right">
                      {formatSize(result.size)}
                    </TableCell>
                    <TableCell align="right">
                      <Box className="results-actions">
                        <Tooltip title="Скачать CSV">
                          <span>
                            <IconButton
                              aria-label={`Скачать ${result.filename}`}
                              disabled={isDownloading || isDeleting}
                              onClick={() => void handleDownload(result)}
                            >
                              {isDownloading ? (
                                <CircularProgress size={19} />
                              ) : (
                                <Download size={19} />
                              )}
                            </IconButton>
                          </span>
                        </Tooltip>
                        <Tooltip title="Создать пользователей в LDAP">
                          <span>
                            <IconButton
                              color="primary"
                              aria-label={`Создать пользователей из ${result.filename}`}
                              disabled={
                                isDownloading ||
                                isDeleting ||
                                creatingKey !== null ||
                                deletingAccountsKey !== null ||
                                mailingKey !== null
                              }
                              onClick={() => void handleCreateAccounts(result)}
                            >
                              {isCreating ? (
                                <CircularProgress size={19} />
                              ) : (
                                <UserPlus size={19} />
                              )}
                            </IconButton>
                          </span>
                        </Tooltip>
                        <Tooltip title="Удалить пользователей из LDAP">
                          <span>
                            <IconButton
                              color="warning"
                              aria-label={`Удалить пользователей из ${result.filename}`}
                              disabled={
                                isDownloading ||
                                isDeleting ||
                                creatingKey !== null ||
                                deletingAccountsKey !== null ||
                                mailingKey !== null
                              }
                              onClick={() =>
                                setDeleteAccountsCandidate(result)
                              }
                            >
                              {isDeletingAccounts ? (
                                <CircularProgress size={19} />
                              ) : (
                                <UserX size={19} />
                              )}
                            </IconButton>
                          </span>
                        </Tooltip>
                        <Tooltip title="Отправить учётные данные на почты">
                          <span>
                            <IconButton
                              color="secondary"
                              aria-label={`Отправить учётные данные из ${result.filename}`}
                              disabled={
                                isDownloading ||
                                isDeleting ||
                                creatingKey !== null ||
                                deletingAccountsKey !== null ||
                                mailingKey !== null
                              }
                              onClick={() => void handleSendCredentials(result)}
                            >
                              {isMailing ? (
                                <CircularProgress size={19} />
                              ) : (
                                <Mail size={19} />
                              )}
                            </IconButton>
                          </span>
                        </Tooltip>
                        <Tooltip title="Удалить CSV">
                          <span>
                            <IconButton
                              color="error"
                              aria-label={`Удалить ${result.filename}`}
                              disabled={isDownloading || isDeleting}
                              onClick={() => setDeleteCandidate(result)}
                            >
                              <Trash2 size={19} />
                            </IconButton>
                          </span>
                        </Tooltip>
                      </Box>
                    </TableCell>
                  </TableRow>
                );
              })
            )}
          </TableBody>
        </Table>
      </TableContainer>

      {creationConflictStatus && (
        <LoginConflictResolver
          conflicts={creationConflictStatus.conflicts}
          socket={creationSocketRef.current}
          onError={setError}
        />
      )}

      <Dialog
        open={deleteCandidate !== null}
        onClose={() => {
          if (!deletingKey) setDeleteCandidate(null);
        }}
      >
        <DialogTitle>Удалить готовый CSV?</DialogTitle>
        <DialogContent>
          <DialogContentText>
            Файл {deleteCandidate?.filename} администратора{" "}
            {deleteCandidate?.owner} будет удалён без возможности
            восстановления.
          </DialogContentText>
        </DialogContent>
        <DialogActions>
          <Button
            color="inherit"
            disabled={deletingKey !== null}
            onClick={() => setDeleteCandidate(null)}
          >
            Отмена
          </Button>
          <Button
            color="error"
            variant="contained"
            disabled={deletingKey !== null}
            onClick={() => void handleDelete()}
          >
            {deletingKey ? "Удаляем…" : "Удалить"}
          </Button>
        </DialogActions>
      </Dialog>

      <Dialog
        open={deleteAccountsCandidate !== null}
        onClose={() => {
          if (!deletingAccountsKey) setDeleteAccountsCandidate(null);
        }}
      >
        <DialogTitle>Удалить пользователей из LDAP?</DialogTitle>
        <DialogContent>
          <DialogContentText>
            Все пользователи из файла {deleteAccountsCandidate?.filename} будут
            удалены из LDAP. Это действие нельзя отменить.
          </DialogContentText>
        </DialogContent>
        <DialogActions>
          <Button
            color="inherit"
            disabled={deletingAccountsKey !== null}
            onClick={() => setDeleteAccountsCandidate(null)}
          >
            Отмена
          </Button>
          <Button
            color="warning"
            variant="contained"
            disabled={deletingAccountsKey !== null}
            onClick={() => void handleDeleteAccounts()}
          >
            Удалить пользователей
          </Button>
        </DialogActions>
      </Dialog>

      {/* Модалка прогресса создания/удаления аккаунтов */}
      <Backdrop
        className="import-backdrop"
        open={isAccountOperationActive}
      >
        <Box className="import-progress" role="status" aria-live="polite">
          <CircularProgress size={42} />
          <Typography component="h2" variant="h2">
            {deletingAccountsKey
              ? "Удаление учётных записей"
              : accountProgress
                ? accountProgress.stage === "creating_accounts"
                  ? "Создание учётных записей"
                  : accountProgress.stage === "deleting_accounts"
                    ? "Удаление учётных записей"
                    : accountProgress.stage === "checking_ldap"
                      ? "Проверка LDAP"
                      : accountProgress.stage === "generating_passwords"
                        ? "Генерация паролей"
                        : accountProgress.stage === "saving_result"
                          ? "Сохранение результата"
                          : "Обработка"
                : "Подключение к задаче"}
          </Typography>
          {accountProgressValue === undefined ? (
            <LinearProgress className="import-progress__bar" />
          ) : (
            <LinearProgress
              className="import-progress__bar"
              variant="determinate"
              value={accountProgressValue}
            />
          )}
          {accountProgress && accountProgress.total > 0 && (
            <Typography color="text.secondary" variant="body2">
              {accountProgress.current} из {accountProgress.total}
            </Typography>
          )}
        </Box>
      </Backdrop>

      {/* Модалка прогресса рассылки */}
      <Backdrop
        className="import-backdrop"
        open={mailingKey !== null}
      >
        <Box className="import-progress" role="status" aria-live="polite">
          <CircularProgress size={42} />
          <Typography component="h2" variant="h2">
            Отправка учётных данных
          </Typography>
          {mailProgressValue === undefined ? (
            <LinearProgress className="import-progress__bar" />
          ) : (
            <LinearProgress
              className="import-progress__bar"
              variant="determinate"
              value={mailProgressValue}
            />
          )}
          {mailProgressData ? (
            <Box sx={{ textAlign: "center" }}>
              <Typography color="text.secondary" variant="body2">
                {mailProgressData.current} из {mailProgressData.total}
              </Typography>
              <Typography
                color="text.secondary"
                variant="caption"
                sx={{ display: "block", mt: 0.5 }}
              >
                Принято: {mailProgressData.accepted}
                {mailProgressData.failed > 0 && (
                  <Typography
                    component="span"
                    variant="caption"
                    color="error"
                    sx={{ ml: 1 }}
                  >
                    Ошибки: {mailProgressData.failed}
                  </Typography>
                )}
              </Typography>
            </Box>
          ) : (
            <Typography color="text.secondary" variant="body2">
              Подготовка к отправке…
            </Typography>
          )}
        </Box>
      </Backdrop>

      <Snackbar
        open={creationSuccess !== null}
        autoHideDuration={6000}
        onClose={() => setCreationSuccess(null)}
        anchorOrigin={{ vertical: "bottom", horizontal: "center" }}
      >
        <Alert
          severity="success"
          variant="filled"
          onClose={() => setCreationSuccess(null)}
        >
          {creationSuccess}
        </Alert>
      </Snackbar>

      <Snackbar
        open={deletionSuccess !== null}
        autoHideDuration={6000}
        onClose={() => setDeletionSuccess(null)}
        anchorOrigin={{ vertical: "bottom", horizontal: "center" }}
      >
        <Alert
          severity="success"
          variant="filled"
          onClose={() => setDeletionSuccess(null)}
        >
          {deletionSuccess}
        </Alert>
      </Snackbar>

      <Snackbar
        open={mailSuccess !== null}
        autoHideDuration={6000}
        onClose={() => setMailSuccess(null)}
        anchorOrigin={{ vertical: "bottom", horizontal: "center" }}
      >
        <Alert
          severity="success"
          variant="filled"
          onClose={() => setMailSuccess(null)}
        >
          {mailSuccess}
        </Alert>
      </Snackbar>
    </Box>
  );
}
