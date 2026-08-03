import { useEffect, useRef, useState } from 'react'
import type { ChangeEvent, DragEvent } from 'react'
import { FileSpreadsheet, Send, UploadCloud, X } from 'lucide-react'
import {
  Alert,
  AlertTitle,
  Backdrop,
  Box,
  Button,
  Chip,
  CircularProgress,
  Divider,
  IconButton,
  LinearProgress,
  Stack,
  Tooltip,
  Typography,
} from '@mui/material'
import {
  createImport,
  getImportErrorMessage,
  openImportEvents,
} from '../api/imports'
import type { JobStage, JobStatus } from '../api/imports'

const MAX_CSV_SIZE = 10 * 1024 * 1024

const stageLabels: Record<JobStage, string> = {
  uploading: 'Файл загружен',
  parsing: 'Чтение CSV',
  validating: 'Проверка строк и дубликатов',
  transliterating: 'Генерация логинов',
  checking_ldap: 'Проверка LDAP',
  generating_passwords: 'Генерация паролей',
  creating_accounts: 'Создание учётных записей',
  saving_result: 'Сохранение результата',
}

export function ImportPage() {
  const inputRef = useRef<HTMLInputElement>(null)
  const [file, setFile] = useState<File | null>(null)
  const [isDragging, setIsDragging] = useState(false)
  const [isProcessing, setIsProcessing] = useState(false)
  const [status, setStatus] = useState<JobStatus | null>(null)
  const [error, setError] = useState<string | null>(null)
  const socketRef = useRef<WebSocket | null>(null)

  useEffect(
    () => () => {
      socketRef.current?.close()
    },
    [],
  )

  const selectFile = (selectedFile?: File) => {
    if (!selectedFile) return
    if (!selectedFile.name.toLowerCase().endsWith('.csv')) {
      setError('Можно загрузить только файл с расширением .csv.')
      return
    }
    if (selectedFile.size > MAX_CSV_SIZE) {
      setError('Размер файла превышает 10 МБ.')
      return
    }

    setError(null)
    setStatus(null)
    setFile(selectedFile)
  }

  const handleFileChange = (event: ChangeEvent<HTMLInputElement>) => {
    selectFile(event.target.files?.[0])
  }

  const handleDrop = (event: DragEvent<HTMLDivElement>) => {
    event.preventDefault()
    setIsDragging(false)
    selectFile(event.dataTransfer.files[0])
  }

  const handleStartImport = async () => {
    if (!file || isProcessing) return

    socketRef.current?.close()
    setError(null)
    setStatus({ type: 'progress', stage: 'uploading', current: 0, total: 1 })
    setIsProcessing(true)

    try {
      const { job_id: jobId } = await createImport(file)
      const socket = openImportEvents(jobId)
      socketRef.current = socket
      let terminalReceived = false

      socket.onmessage = (event) => {
        try {
          const nextStatus = JSON.parse(String(event.data)) as JobStatus
          setStatus(nextStatus)

          if (nextStatus.type === 'completed') {
            terminalReceived = true
            setIsProcessing(false)
          } else if (nextStatus.type === 'failed') {
            terminalReceived = true
            setError(nextStatus.message)
            setIsProcessing(false)
          } else if (nextStatus.type === 'partial_failure') {
            terminalReceived = true
            setError(`Обработка остановлена на строке ${nextStatus.failed_row}.`)
            setIsProcessing(false)
          }
        } catch {
          terminalReceived = true
          setError('Backend прислал некорректный статус задания.')
          setIsProcessing(false)
          socket.close()
        }
      }
      socket.onerror = () => {
        if (!terminalReceived) {
          setError('Не удалось подключиться к каналу статусов задания.')
          setIsProcessing(false)
        }
      }
      socket.onclose = () => {
        if (!terminalReceived) {
          setError('Канал статусов закрылся до завершения задания.')
          setIsProcessing(false)
        }
        if (socketRef.current === socket) socketRef.current = null
      }
    } catch (requestError) {
      setError(getImportErrorMessage(requestError))
      setIsProcessing(false)
    }
  }

  const progress = status?.type === 'progress' ? status : null
  const progressValue =
    progress && progress.total > 0
      ? Math.min(100, (progress.current / progress.total) * 100)
      : undefined

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
          {error && (
            <Alert severity="error" className="import-message">
              <AlertTitle>Импорт не выполнен</AlertTitle>
              {error}
            </Alert>
          )}
          {status?.type === 'completed' && (
            <Alert severity="success" className="import-message">
              <AlertTitle>CSV успешно подготовлен</AlertTitle>
              Обработано строк: {status.total}. Результат:{' '}
              {status.result.owner}/{status.result.filename}
            </Alert>
          )}
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
              disabled={!file || isProcessing}
              variant="contained"
              startIcon={<Send size={18} />}
              onClick={handleStartImport}
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
            <li>Колонки: First, Last, Patronymic, Email, Group</li>
            <li>Разделитель — запятая</li>
            <li>Кодировка UTF-8 или Windows-1251</li>
            <li>Максимальный размер — 10 МБ</li>
          </Stack>
        </Box>
      </Box>

      <Backdrop className="import-backdrop" open={isProcessing}>
        <Box className="import-progress" role="status" aria-live="polite">
          <CircularProgress size={42} />
          <Typography component="h2" variant="h2">
            {progress ? stageLabels[progress.stage] : 'Запуск импорта'}
          </Typography>
          {progressValue === undefined ? (
            <LinearProgress className="import-progress__bar" />
          ) : (
            <LinearProgress
              className="import-progress__bar"
              variant="determinate"
              value={progressValue}
            />
          )}
          {progress && progress.total > 0 && (
            <Typography color="text.secondary" variant="body2">
              {progress.current} из {progress.total}
            </Typography>
          )}
        </Box>
      </Backdrop>
    </Box>
  )
}
