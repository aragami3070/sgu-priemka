import { useState } from 'react'
import type { FormEvent } from 'react'
import { Eye, EyeOff, LogIn } from 'lucide-react'
import {
  Box,
  Button,
  IconButton,
  InputAdornment,
  Paper,
  TextField,
  Tooltip,
  Typography,
} from '@mui/material'
import { Brand } from '../components/Brand'

interface LoginPageProps {
  onLogin: (identifier: string) => void
}

export function LoginPage({ onLogin }: LoginPageProps) {
  const [identifier, setIdentifier] = useState('')
  const [password, setPassword] = useState('')
  const [showPassword, setShowPassword] = useState(false)

  const handleSubmit = (event: FormEvent<HTMLFormElement>) => {
    event.preventDefault()
    if (identifier.trim() && password) {
      onLogin(identifier.trim())
    }
  }

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
          <TextField
            fullWidth
            required
            autoFocus
            autoComplete="username"
            label="Логин или email"
            value={identifier}
            onChange={(event) => setIdentifier(event.target.value)}
          />
          <TextField
            fullWidth
            required
            autoComplete="current-password"
            label="Пароль"
            type={showPassword ? 'text' : 'password'}
            value={password}
            onChange={(event) => setPassword(event.target.value)}
            slotProps={{
              input: {
                endAdornment: (
                  <InputAdornment position="end">
                    <Tooltip title={showPassword ? 'Скрыть пароль' : 'Показать пароль'}>
                      <IconButton
                        aria-label={showPassword ? 'Скрыть пароль' : 'Показать пароль'}
                        edge="end"
                        onClick={() => setShowPassword((visible) => !visible)}
                      >
                        {showPassword ? <EyeOff size={19} /> : <Eye size={19} />}
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
            startIcon={<LogIn size={18} />}
          >
            Войти
          </Button>
        </Box>
      </Paper>
      <Typography className="login-page__footer" variant="caption">
        Саратовский государственный университет
      </Typography>
    </Box>
  )
}
