import { createTheme } from '@mui/material/styles'

export const appTheme = createTheme({
  palette: {
    mode: 'light',
    primary: {
      main: '#176b5b',
      dark: '#105247',
      light: '#dceee9',
      contrastText: '#ffffff',
    },
    secondary: {
      main: '#315a8c',
    },
    background: {
      default: '#f4f6f7',
      paper: '#ffffff',
    },
    text: {
      primary: '#17211f',
      secondary: '#5d6966',
    },
    divider: '#dce2e0',
  },
  shape: {
    borderRadius: 6,
  },
  typography: {
    fontFamily: "Inter, 'Segoe UI', Arial, sans-serif",
    h1: {
      fontSize: '1.75rem',
      fontWeight: 700,
      lineHeight: 1.25,
      letterSpacing: 0,
    },
    h2: {
      fontSize: '1.125rem',
      fontWeight: 650,
      lineHeight: 1.35,
      letterSpacing: 0,
    },
    button: {
      fontWeight: 650,
      letterSpacing: 0,
      textTransform: 'none',
    },
    body1: {
      letterSpacing: 0,
    },
    body2: {
      letterSpacing: 0,
    },
  },
  components: {
    MuiButton: {
      defaultProps: {
        disableElevation: true,
      },
      styleOverrides: {
        root: {
          minHeight: 40,
          borderRadius: 6,
        },
      },
    },
    MuiPaper: {
      defaultProps: {
        elevation: 0,
      },
    },
    MuiTextField: {
      defaultProps: {
        size: 'small',
      },
    },
    MuiTooltip: {
      defaultProps: {
        arrow: true,
      },
    },
  },
})
