import { useState } from 'react'
import { AppShell } from './components/AppShell'
import { ImportPage } from './pages/ImportPage'
import { LoginPage } from './pages/LoginPage'
import { ResultsPage } from './pages/ResultsPage'
import type { AppView, AuthUser } from './types/app'
import './App.css'

function App() {
  const [user, setUser] = useState<AuthUser | null>(null)
  const [view, setView] = useState<AppView>('import')

  if (!user) {
    return (
      <LoginPage
        onLogin={(identifier) => setUser({ identifier })}
      />
    )
  }

  return (
    <AppShell
      user={user}
      view={view}
      onViewChange={setView}
      onLogout={() => {
        setUser(null)
        setView('import')
      }}
    >
      {view === 'import' ? <ImportPage /> : <ResultsPage />}
    </AppShell>
  )
}

export default App
