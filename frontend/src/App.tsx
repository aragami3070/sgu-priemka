import { useEffect, useState } from 'react'
import { Box, CircularProgress, Typography } from '@mui/material'
import { login, logout, whoami } from './api/auth'
import { AppShell } from './components/AppShell'
import { ImportPage } from './pages/ImportPage'
import { LoginPage } from './pages/LoginPage'
import { ResultsPage } from './pages/ResultsPage'
import type { AppView, AuthUser } from './types/app'
import './App.css'

function App() {
  const [user, setUser] = useState<AuthUser | null>(null)
  const [isCheckingSession, setIsCheckingSession] = useState(true)
  const [view, setView] = useState<AppView>('import')

  useEffect(() => {
    let cancelled = false

    void whoami()
      .then((response) => {
        if (!cancelled) setUser({ username: response.username })
      })
      .catch(() => {
        if (!cancelled) setUser(null)
      })
      .finally(() => {
        if (!cancelled) setIsCheckingSession(false)
      })

    return () => {
      cancelled = true
    }
  }, [])

  const handleLogin = async (identifier: string, password: string) => {
    const response = await login({ identifier, password })
    setUser({
      username: response.username,
      expiresAt: response.expires_at,
    })
  }

  const handleSkipLogin = () => {
    setUser({ username: 'test-user', isSkipped: true })
  }

  const handleLogout = async () => {
    if (!user?.isSkipped) {
      try {
        await logout()
      } catch {
        // Локально завершаем вход даже при недоступном backend.
      }
    }

    setUser(null)
    setView('import')
  }

  if (isCheckingSession) {
    return (
      <Box className="auth-loading" role="status" aria-live="polite">
        <CircularProgress size={42} />
        <Typography color="text.secondary">Проверка сессии…</Typography>
      </Box>
    )
  }

  if (!user) {
    return <LoginPage onLogin={handleLogin} onSkip={handleSkipLogin} />
  }

  return (
    <AppShell
      user={user}
      view={view}
      onViewChange={setView}
      onLogout={handleLogout}
    >
      {view === 'import' ? <ImportPage /> : <ResultsPage />}
    </AppShell>
  )
}

export default App
