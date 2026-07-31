import type { ReactNode, SyntheticEvent } from 'react'
import { Download, LogOut, Upload } from 'lucide-react'
import {
  AppBar,
  Avatar,
  Box,
  Container,
  IconButton,
  Tab,
  Tabs,
  Toolbar,
  Tooltip,
  Typography,
} from '@mui/material'
import type { AppView, AuthUser } from '../types/app'
import { Brand } from './Brand'

interface AppShellProps {
  children: ReactNode
  user: AuthUser
  view: AppView
  onViewChange: (view: AppView) => void
  onLogout: () => void
}

export function AppShell({
  children,
  user,
  view,
  onViewChange,
  onLogout,
}: AppShellProps) {
  const handleTabChange = (_event: SyntheticEvent, nextView: AppView) => {
    onViewChange(nextView)
  }

  return (
    <Box className="app-layout">
      <AppBar className="app-header" position="static" color="inherit">
        <Container maxWidth="lg">
          <Toolbar disableGutters className="app-header__toolbar">
            <Brand compact />
            <Box className="app-header__account">
              <Avatar className="app-header__avatar">
                {user.username.slice(0, 1).toUpperCase()}
              </Avatar>
              <Typography className="app-header__user" variant="body2">
                {user.username}
              </Typography>
              <Tooltip title="Выйти">
                <IconButton aria-label="Выйти" onClick={onLogout}>
                  <LogOut size={19} />
                </IconButton>
              </Tooltip>
            </Box>
          </Toolbar>
        </Container>
        <Box className="app-nav">
          <Container maxWidth="lg">
            <Tabs
              value={view}
              onChange={handleTabChange}
              aria-label="Разделы сервиса"
            >
              <Tab
                icon={<Upload size={18} />}
                iconPosition="start"
                label="Новый импорт"
                value="import"
              />
              <Tab
                icon={<Download size={18} />}
                iconPosition="start"
                label="Готовые CSV"
                value="results"
              />
            </Tabs>
          </Container>
        </Box>
      </AppBar>
      <Container component="main" maxWidth="lg" className="app-main">
        {children}
      </Container>
    </Box>
  )
}
