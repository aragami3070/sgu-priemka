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

type ResolutionError = {
  login?: string;
  fullName?: string;
};

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
    Record<number, ResolutionError>
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
            conflict.login_conflict &&
            !/^[A-Za-z0-9]+$/.test(
              (replacementLogins[conflict.row] ?? "").trim(),
            ),
        )
        .map((conflict) => [
          conflict.row,
          { login: "Используйте только латинские буквы и цифры." },
        ]),
    );
    const invalidNames = Object.fromEntries(
      conflicts
        .filter((conflict) => {
          if (!conflict.full_name_conflict) return false;
          const value = (replacementFullNames[conflict.row] ?? "").trim();
          return value.split(/\s+/).filter(Boolean).length !== 3;
        })
        .map((conflict) => [
          conflict.row,
          { fullName: "Введите фамилию, имя и отчество через пробел." },
        ]),
    );
    if (
      Object.keys(invalid).length > 0 ||
      Object.keys(invalidNames).length > 0
    ) {
      const merged = { ...invalid } as Record<number, ResolutionError>;
      for (const [row, error] of Object.entries(invalidNames)) {
        const rowNumber = Number(row);
        merged[rowNumber] = { ...merged[rowNumber], ...error };
      }
      setResolutionErrors(merged);
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
                    error={resolutionErrors[conflict.row]?.fullName !== undefined}
                    helperText={
                      resolutionErrors[conflict.row]?.fullName ??
                      (conflict.full_name_conflict ? conflict.message : undefined)
                    }
                    disabled={isSending || !conflict.full_name_conflict}
                    onChange={(event) => {
                      const fullName = event.target.value;
                      setReplacementFullNames((current) => ({
                        ...current,
                        [conflict.row]: fullName,
                      }));
                      setResolutionErrors((current) => {
                        const updated = { ...current };
                        if (updated[conflict.row]) {
                          delete updated[conflict.row].fullName;
                          if (!updated[conflict.row].login) {
                            delete updated[conflict.row];
                          }
                        }
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
                    error={resolutionErrors[conflict.row]?.login !== undefined}
                    helperText={
                      resolutionErrors[conflict.row]?.login ??
                      (conflict.login_conflict ? conflict.message : undefined)
                    }
                    disabled={isSending || !conflict.login_conflict}
                    onChange={(event) => {
                      const login = event.target.value;
                      setReplacementLogins((current) => ({
                        ...current,
                        [conflict.row]: login,
                      }));
                      setResolutionErrors((current) => {
                        const updated = { ...current };
                        if (updated[conflict.row]) {
                          delete updated[conflict.row].login;
                          if (!updated[conflict.row].fullName) {
                            delete updated[conflict.row];
                          }
                        }
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
