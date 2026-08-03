import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Download,
  FileClock,
  RefreshCw,
  RotateCcw,
  Trash2,
} from "lucide-react";
import {
  Alert,
  AlertTitle,
  Box,
  Button,
  CircularProgress,
  Dialog,
  DialogActions,
  DialogContent,
  DialogContentText,
  DialogTitle,
  IconButton,
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
  deleteResult,
  downloadResult,
  getResultErrorMessage,
  listResults,
} from "../api/results";
import type { ResultItem } from "../api/results";

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
    </Box>
  );
}
