import { useState } from "react";
import type { FormEvent } from "react";
import { Eye, EyeOff, LogIn, Wrench } from "lucide-react";
import {
  Alert,
  Box,
  Button,
  CircularProgress,
  Divider,
  IconButton,
  InputAdornment,
  Paper,
  TextField,
  Tooltip,
  Typography,
} from "@mui/material";
import { getLoginErrorMessage } from "../api/auth";
import { Brand } from "../components/Brand";

interface LoginPageProps {
  onLogin: (identifier: string, password: string) => Promise<void>;
  onSkip: () => void;
}

const authSkipEnabled = import.meta.env.DEV;

export function LoginPage({ onLogin, onSkip }: LoginPageProps) {
  const [identifier, setIdentifier] = useState("");
  const [password, setPassword] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const handleSubmit = async (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault();
    if (!identifier.trim() || !password || isSubmitting) {
      return;
    }

    setErrorMessage(null);
    setIsSubmitting(true);

    try {
      await onLogin(identifier.trim(), password);
      setPassword("");
    } catch (error) {
      setErrorMessage(getLoginErrorMessage(error));
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <Box className="login-page">
      <Box className="login-page__brand">
        <Brand />
      </Box>
      <Paper
        component="section"
        className="login-panel"
        aria-labelledby="login-title"
      >
        <Box className="login-panel__heading">
          <Typography component="h1" id="login-title" variant="h1">
            Вход в систему
          </Typography>
          <Typography color="text.secondary">
            Используйте учётную запись сотрудника
          </Typography>
        </Box>
        <Box component="form" className="login-form" onSubmit={handleSubmit}>
          {errorMessage && <Alert severity="error">{errorMessage}</Alert>}
          <TextField
            fullWidth
            required
            autoFocus
            autoComplete="username"
            label="Идентификатор"
            value={identifier}
            disabled={isSubmitting}
            onChange={(event) => setIdentifier(event.target.value)}
          />
          <TextField
            fullWidth
            required
            autoComplete="current-password"
            label="Пароль"
            type={showPassword ? "text" : "password"}
            value={password}
            disabled={isSubmitting}
            onChange={(event) => setPassword(event.target.value)}
            slotProps={{
              input: {
                endAdornment: (
                  <InputAdornment position="end">
                    <Tooltip
                      title={showPassword ? "Скрыть пароль" : "Показать пароль"}
                    >
                      <IconButton
                        aria-label={
                          showPassword ? "Скрыть пароль" : "Показать пароль"
                        }
                        edge="end"
                        onClick={() => setShowPassword((visible) => !visible)}
                      >
                        {showPassword ? (
                          <EyeOff size={19} />
                        ) : (
                          <Eye size={19} />
                        )}
                      </IconButton>
                    </Tooltip>
                  </InputAdornment>
                ),
              },
            }}
          />
          <Button
            fullWidth
            type="submit"
            variant="contained"
            disabled={isSubmitting}
            startIcon={
              isSubmitting ? (
                <CircularProgress color="inherit" size={18} />
              ) : (
                <LogIn size={18} />
              )
            }
          >
            {isSubmitting ? "Вход…" : "Войти"}
          </Button>
          {authSkipEnabled && (
            <>
              <Divider>для разработки</Divider>
              <Button
                fullWidth
                type="button"
                color="inherit"
                variant="outlined"
                disabled={isSubmitting}
                startIcon={<Wrench size={18} />}
                onClick={onSkip}
              >
                Пропустить вход
              </Button>
            </>
          )}
        </Box>
      </Paper>
      <Typography className="login-page__footer" variant="caption">
        Саратовский государственный университет
      </Typography>
    </Box>
  );
}
