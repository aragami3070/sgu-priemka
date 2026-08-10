import axios from 'axios'
import { apiClient } from './client'

export type JobStage =
  | 'uploading'
  | 'parsing'
  | 'validating'
  | 'transliterating'
  | 'checking_ldap'
  | 'generating_passwords'
  | 'creating_accounts'
  | 'deleting_accounts'
  | 'sending_mail'
  | 'saving_result'

export interface ResultReference {
  owner: string
  filename: string
}

export interface LoginConflict {
  row: number
  full_name: string
  login: string
  message: string
  login_conflict: boolean
  full_name_conflict: boolean
}

export interface LoginResolution {
  row: number
  login: string
  full_name: string
}

export type JobStatus =
  | {
      type: 'progress'
      stage: JobStage
      current: number
      total: number
    }
  | {
      type: 'awaiting_login_resolutions'
      conflicts: LoginConflict[]
    }
  | {
      type: 'completed'
      created: number
      total: number
      result: ResultReference
    }
  | {
      type: 'deleted'
      deleted: number
      total: number
      result: ResultReference
    }
  | {
      type: 'mail_progress'
      current: number
      total: number
      accepted: number
      failed: number
    }
  | {
      type: 'mail_completed'
      accepted: number
      failed: number
      total: number
      result: ResultReference
    }
  | {
      type: 'failed'
      stage: JobStage
      code: string
      message: string
      row: number | null
    }
  | {
      type: 'partial_failure'
      created: number
      total: number
      failed_row: number
      failed_fio: string
      ldap_phase: string
      possibly_created: boolean
      result: ResultReference
    }

interface CreateImportResponse {
  job_id: string
}

interface ActiveImportResponse {
  job_id: string
}

export async function createImport(file: File): Promise<CreateImportResponse> {
  const form = new FormData()
  form.append('file', file)

  const response = await apiClient.post<CreateImportResponse>('/imports', form, {
    timeout: 30_000,
  })
  return response.data
}

export async function getActiveImport(): Promise<string | null> {
  const response = await apiClient.get<ActiveImportResponse>('/imports/active')
  return response.status === 204 ? null : response.data.job_id
}

export function openImportEvents(jobId: string): WebSocket {
  const configuredBase = apiClient.defaults.baseURL || '/api'
  const base = new URL(
    configuredBase.endsWith('/') ? configuredBase : `${configuredBase}/`,
    window.location.origin,
  )
  base.protocol = base.protocol === 'https:' ? 'wss:' : 'ws:'

  return new WebSocket(
    new URL(`imports/${encodeURIComponent(jobId)}/events`, base).toString(),
  )
}

export function resolveLoginConflicts(
  socket: WebSocket,
  resolutions: LoginResolution[],
): void {
  socket.send(
    JSON.stringify({
      type: 'resolve_logins',
      resolutions,
    }),
  )
}

export function getImportErrorMessage(error: unknown): string {
  if (!axios.isAxiosError(error)) {
    return 'Не удалось запустить импорт.'
  }

  if (error.response?.status === 409) {
    return 'У вас уже есть активная задача импорта. Обновите страницу, чтобы вернуться к ней.'
  }
  const responseMessage = error.response?.data
  if (typeof responseMessage === 'string' && responseMessage.length > 0) {
    return responseMessage
  }
  if (error.response?.status === 401) {
    return 'Сессия отсутствует или истекла. Войдите снова.'
  }
  if (error.response?.status === 413) {
    return 'Размер файла превышает 10 МБ.'
  }
  return error.response
    ? 'Backend отклонил загруженный файл.'
    : 'Не удалось подключиться к backend.'
}
