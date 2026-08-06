import { useRef, useState } from 'react'
import type { ChangeEvent, DragEvent } from 'react'
import { FileCog, Send, UploadCloud, X } from 'lucide-react'
import {
  Alert,
  AlertTitle,
  Box,
  Button,
  Chip,
  CircularProgress,
  Divider,
  IconButton,
  Stack,
  Typography,
} from '@mui/material'
import { getGroupsErrorMessage, replaceGroups } from '../api/groups'

const MAX_GROUPS_FILE_SIZE = 1024 * 1024

/** Страница загрузки и замены TOML-конфигурации учебных групп. */
export function GroupsPage() {
  const inputRef = useRef<HTMLInputElement>(null)
  const [file, setFile] = useState<File | null>(null)
  const [isDragging, setIsDragging] = useState(false)
  const [isUploading, setIsUploading] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const [success, setSuccess] = useState<number | null>(null)

  const selectFile = (selectedFile?: File) => {
    if (!selectedFile) return
    if (!selectedFile.name.toLowerCase().endsWith('.toml')) {
      setError('Можно загрузить только файл с расширением .toml.')
      return
    }
    if (selectedFile.size > MAX_GROUPS_FILE_SIZE) {
      setError('Размер TOML-файла превышает 1 МБ.')
      return
    }
    setError(null)
    setSuccess(null)
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

  const handleUpload = async () => {
    if (!file || isUploading) return
    setIsUploading(true)
    setError(null)
    setSuccess(null)
    try {
      const response = await replaceGroups(file)
      setSuccess(response.groups)
      setFile(null)
      if (inputRef.current) inputRef.current.value = ''
    } catch (requestError) {
      setError(getGroupsErrorMessage(requestError))
    } finally {
      setIsUploading(false)
    }
  }

  return (
    <Box className="page-section">
      <Box className="page-heading">
        <Box>
          <Typography component="h1" variant="h1">Учебные группы</Typography>
          <Typography color="text.secondary">
            Загрузите TOML с соответствиями номеров и названий LDAP-групп
          </Typography>
        </Box>
        <Chip label="TOML" size="small" color="primary" variant="outlined" />
      </Box>

      <Box className="import-layout">
        <Box className="import-workspace">
          {error && <Alert severity="error" className="import-message"><AlertTitle>Файл групп не заменён</AlertTitle>{error}</Alert>}
          {success !== null && <Alert severity="success" className="import-message"><AlertTitle>Файл групп успешно заменён</AlertTitle>Загружено групп: {success}.</Alert>}

          <input ref={inputRef} hidden type="file" accept=".toml,application/toml" onChange={handleFileChange} />
          <Box
            className={`dropzone${isDragging ? ' dropzone--active' : ''}`}
            role="button"
            tabIndex={0}
            onClick={() => inputRef.current?.click()}
            onKeyDown={(event) => {
              if (event.key === 'Enter' || event.key === ' ') inputRef.current?.click()
            }}
            onDragEnter={(event) => { event.preventDefault(); setIsDragging(true) }}
            onDragOver={(event) => event.preventDefault()}
            onDragLeave={() => setIsDragging(false)}
            onDrop={handleDrop}
          >
            <Box className="dropzone__icon" aria-hidden="true"><UploadCloud size={30} strokeWidth={1.7} /></Box>
            <Typography component="h2" variant="h2">Перетащите TOML-файл сюда</Typography>
            <Typography color="text.secondary" variant="body2">или выберите его на компьютере</Typography>
            <Typography color="text.secondary" variant="caption">Максимальный размер — 1 МБ</Typography>
          </Box>

          {file && (
            <Box className="selected-file">
              <FileCog size={22} aria-hidden="true" />
              <Box className="selected-file__meta"><Typography variant="body2" sx={{ fontWeight: 650 }}>{file.name}</Typography><Typography color="text.secondary" variant="caption">{file.size} Б</Typography></Box>
              <IconButton aria-label="Убрать файл" disabled={isUploading} onClick={() => { setFile(null); if (inputRef.current) inputRef.current.value = '' }}><X size={18} /></IconButton>
            </Box>
          )}

          <Divider />
          <Stack direction="row" sx={{ justifyContent: 'flex-end' }} spacing={1.5}>
            <Button variant="outlined" disabled={!file || isUploading} onClick={() => void handleUpload()} startIcon={isUploading ? <CircularProgress size={17} /> : <Send size={17} />}>
              {isUploading ? 'Заменяем…' : 'Заменить файл групп'}
            </Button>
          </Stack>
        </Box>
        <Box className="import-aside"><Typography variant="h3">Формат файла</Typography><Typography color="text.secondary" variant="body2">Используйте секцию <code>[groups]</code> и числовые ключи:</Typography><Box component="pre" sx={{ mt: 1, mb: 0 }}>{'[groups]\n151 = "ПИ"\n161 = "ПО"'}</Box></Box>
      </Box>
    </Box>
  )
}
