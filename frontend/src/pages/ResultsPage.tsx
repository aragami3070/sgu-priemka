import { Download, FileClock, RefreshCw, RotateCcw } from 'lucide-react'
import {
  Box,
  Button,
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
} from '@mui/material'

export function ResultsPage() {
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
          <IconButton aria-label="Обновить список">
            <RefreshCw size={19} />
          </IconButton>
        </Tooltip>
      </Box>

      <Box className="results-filters">
        <TextField
          label="Дата с"
          type="date"
          slotProps={{ inputLabel: { shrink: true } }}
        />
        <TextField
          label="Дата по"
          type="date"
          slotProps={{ inputLabel: { shrink: true } }}
        />
        <Button color="inherit" startIcon={<RotateCcw size={17} />}>
          Сбросить
        </Button>
      </Box>

      <TableContainer className="results-table">
        <Table aria-label="Список готовых CSV">
          <TableHead>
            <TableRow>
              <TableCell>Файл</TableCell>
              <TableCell>Создан</TableCell>
              <TableCell align="right">Размер</TableCell>
              <TableCell align="right" aria-label="Действия" />
            </TableRow>
          </TableHead>
          <TableBody>
            <TableRow>
              <TableCell colSpan={4} className="results-empty">
                <Box className="results-empty__content">
                  <FileClock size={30} strokeWidth={1.6} aria-hidden="true" />
                  <Typography sx={{ fontWeight: 650 }}>
                    Готовых файлов пока нет
                  </Typography>
                  <Typography color="text.secondary" variant="body2">
                    Они появятся здесь после обработки импорта
                  </Typography>
                </Box>
              </TableCell>
            </TableRow>
          </TableBody>
        </Table>
      </TableContainer>

      <Button className="results-download-placeholder" disabled startIcon={<Download size={17} />}>
        Скачать
      </Button>
    </Box>
  )
}
