import { useState } from 'react'
import { login, logout } from './api/auth'
import { AppShell } from './components/AppShell'
import { ImportPage } from './pages/ImportPage'
import { LoginPage } from './pages/LoginPage'
import { ResultsPage } from './pages/ResultsPage'
import type { AppView, AuthUser } from './types/app'
import './App.css'

function App() {
  const [user, setUser] = useState<AuthUser | null>(null)
  const [view, setView] = useState<AppView>('import')

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
