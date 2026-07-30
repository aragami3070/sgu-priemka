import { useRef, useState } from 'react'
import type { ChangeEvent, DragEvent } from 'react'
import { FileSpreadsheet, Send, UploadCloud, X } from 'lucide-react'
import {
  Box,
  Button,
  Chip,
  Divider,
  IconButton,
  Stack,
  Tooltip,
  Typography,
} from '@mui/material'

export function ImportPage() {
  const inputRef = useRef<HTMLInputElement>(null)
  const [file, setFile] = useState<File | null>(null)
  const [isDragging, setIsDragging] = useState(false)

  const selectFile = (selectedFile?: File) => {
    if (selectedFile?.name.toLowerCase().endsWith('.csv')) {
      setFile(selectedFile)
    }
  }

  const handleFileChange = (event: ChangeEvent<HTMLInputElement>) => {
    selectFile(event.target.files?.[0])
  }

  const handleDrop = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault()
    setIsDragging(false)
    selectFile(event.dataTransfer.files[0])
  }

  return (
    <Box className="page-section">
      <Box className="page-heading">
        <Box>
          <Typography component="h1" variant="h1">
            Создание учётных записей
          </Typography>
          <Typography color="text.secondary">
            Загрузите подготовленный список студентов
          </Typography>
        </Box>
        <Chip label="CSV" size="small" color="primary" variant="outlined" />
      </Box>

      <Box className="import-layout">
        <Box className="import-workspace">
          <input
            ref={inputRef}
            hidden
            type="file"
            accept=".csv,text/csv"
            onChange={handleFileChange}
          />
          <Box
            className={`dropzone${isDragging ? ' dropzone--active' : ''}`}
            role="button"
            tabIndex={0}
            onClick={() => inputRef.current?.click()}
            onKeyDown={(event) => {
              if (event.key === 'Enter' || event.key === ' ') {
                inputRef.current?.click()
              }
            }}
            onDragEnter={(event) => {
              event.preventDefault()
              setIsDragging(true)
            }}
            onDragOver={(event) => event.preventDefault()}
            onDragLeave={() => setIsDragging(false)}
            onDrop={handleDrop}
          >
            <Box className="dropzone__icon" aria-hidden="true">
              <UploadCloud size={30} strokeWidth={1.7} />
            </Box>
            <Typography component="h2" variant="h2">
              Перетащите CSV-файл сюда
            </Typography>
            <Typography color="text.secondary" variant="body2">
              или выберите его на компьютере
            </Typography>
            <Button variant="outlined" component="span">
              Выбрать файл
            </Button>
          </Box>

          {file && (
            <Box className="selected-file">
              <FileSpreadsheet size={22} aria-hidden="true" />
              <Box className="selected-file__meta">
                <Typography noWrap sx={{ fontWeight: 650 }}>
                  {file.name}
                </Typography>
                <Typography color="text.secondary" variant="caption">
                  {(file.size / 1024).toFixed(1)} КБ
                </Typography>
              </Box>
              <Tooltip title="Убрать файл">
                <IconButton
                  aria-label="Убрать файл"
                  size="small"
                  onClick={() => {
                    setFile(null)
                    if (inputRef.current) inputRef.current.value = ''
                  }}
                >
                  <X size={18} />
                </IconButton>
              </Tooltip>
            </Box>
          )}

          <Box className="import-actions">
            <Button
              disabled={!file}
              variant="contained"
              startIcon={<Send size={18} />}
            >
              Запустить импорт
            </Button>
          </Box>
        </Box>

        <Box component="aside" className="requirements">
          <Typography component="h2" variant="h2">
            Требования к файлу
          </Typography>
          <Divider />
          <Stack component="ul" className="requirements__list" spacing={1.5}>
            <li>Колонки: First, Last, Fio</li>
            <li>Разделитель — запятая</li>
            <li>Кодировка UTF-8 или Windows-1251</li>
            <li>Не более 50 000 студентов</li>
            <li>Максимальный размер — 10 МБ</li>
          </Stack>
        </Box>
      </Box>
    </Box>
  )
}
