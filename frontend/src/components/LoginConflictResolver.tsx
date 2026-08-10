import { useEffect, useState } from "react";
import {
  Alert,
  AlertTitle,
  Box,
  Button,
  Paper,
  Table,
  TableBody,
  TableCell,
  TableContainer,
  TableHead,
  TableRow,
  TextField,
} from "@mui/material";
import { resolveLoginConflicts } from "../api/imports";
import type { LoginConflict } from "../api/imports";

interface LoginConflictResolverProps {
  conflicts: LoginConflict[];
  socket: WebSocket | null;
  onError: (message: string | null) => void;
}

/** Общая таблица исправления конфликтующих логинов для import-job WebSocket. */
export function LoginConflictResolver({
  conflicts,
  socket,
  onError,
}: LoginConflictResolverProps) {
  const [replacementLogins, setReplacementLogins] = useState<
    Record<number, string>
  >({});
  const [replacementFullNames, setReplacementFullNames] = useState<
    Record<number, string>
  >({});
  const [resolutionErrors, setResolutionErrors] = useState<
    Record<number, string>
  >({});
  const [isSending, setIsSending] = useState(false);

  useEffect(() => {
    setReplacementLogins(
      Object.fromEntries(
        conflicts.map((conflict) => [conflict.row, conflict.login]),
      ),
    );
    setReplacementFullNames(
      Object.fromEntries(
        conflicts.map((conflict) => [conflict.row, conflict.full_name]),
      ),
    );
    setResolutionErrors({});
    setIsSending(false);
  }, [conflicts]);

  const submit = () => {
    const invalid = Object.fromEntries(
      conflicts
        .filter(
          (conflict) =>
            !/^[A-Za-z0-9]+$/.test(
              (replacementLogins[conflict.row] ?? "").trim(),
            ),
        )
        .map((conflict) => [
          conflict.row,
          "Используйте только латинские буквы и цифры.",
        ]),
    );
    const invalidNames = Object.fromEntries(
      conflicts
        .filter((conflict) => {
          const value = (replacementFullNames[conflict.row] ?? "").trim();
          return value.split(/\s+/).filter(Boolean).length !== 3;
        })
        .map((conflict) => [
          conflict.row,
          'Введите фамилию, имя и отчество через пробел.',
        ]),
    );
    if (
      Object.keys(invalid).length > 0 ||
      Object.keys(invalidNames).length > 0
    ) {
      setResolutionErrors({ ...invalid, ...invalidNames });
      return;
    }
    if (!socket || socket.readyState !== WebSocket.OPEN) {
      onError("Канал задания недоступен. Переподключитесь к задаче.");
      return;
    }

    onError(null);
    setResolutionErrors({});
    setIsSending(true);
    resolveLoginConflicts(
      socket,
      conflicts.map((conflict) => ({
        row: conflict.row,
        login: replacementLogins[conflict.row].trim(),
        full_name: replacementFullNames[conflict.row].trim(),
      })),
    );
  };

  return (
    <Box className="login-conflicts">
      <Alert severity="warning">
        <AlertTitle>Найдены конфликты данных ({conflicts.length})</AlertTitle>
        Измените необходимые ФИО и логины и отправьте всю таблицу на повторную
        проверку.
      </Alert>
      <TableContainer component={Paper} variant="outlined">
        <Table className="login-conflicts__table" size="small">
          <TableHead>
            <TableRow>
              <TableCell>Строка</TableCell>
              <TableCell>ФИО</TableCell>
              <TableCell>Login</TableCell>
            </TableRow>
          </TableHead>
          <TableBody>
            {conflicts.map((conflict) => (
              <TableRow key={conflict.row}>
                <TableCell>{conflict.row}</TableCell>
                <TableCell>
                  <TextField
                    fullWidth
                    size="small"
                    value={replacementFullNames[conflict.row] ?? ""}
                    error={resolutionErrors[conflict.row] !== undefined}
                    helperText={
                      resolutionErrors[conflict.row] ?? conflict.message
                    }
                    disabled={isSending}
                    onChange={(event) => {
                      const fullName = event.target.value;
                      setReplacementFullNames((current) => ({
                        ...current,
                        [conflict.row]: fullName,
                      }));
                      setResolutionErrors((current) => {
                        const updated = { ...current };
                        delete updated[conflict.row];
                        return updated;
                      });
                    }}
                  />
                </TableCell>
                <TableCell>
                  <TextField
                    fullWidth
                    size="small"
                    value={replacementLogins[conflict.row] ?? ""}
                    error={resolutionErrors[conflict.row] !== undefined}
                    helperText={
                      resolutionErrors[conflict.row] ?? conflict.message
                    }
                    disabled={isSending}
                    onChange={(event) => {
                      const login = event.target.value;
                      setReplacementLogins((current) => ({
                        ...current,
                        [conflict.row]: login,
                      }));
                      setResolutionErrors((current) => {
                        const updated = { ...current };
                        delete updated[conflict.row];
                        return updated;
                      });
                    }}
                  />
                </TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </TableContainer>
      <Box className="login-conflicts__actions">
        <Button variant="contained" disabled={isSending} onClick={submit}>
          {isSending ? "Проверяем…" : "Проверить все значения"}
        </Button>
      </Box>
    </Box>
  );
}
