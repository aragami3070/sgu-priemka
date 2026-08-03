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
  | 'saving_result'

export interface ResultReference {
  owner: string
  filename: string
}

export type JobStatus =
  | {
      type: 'progress'
      stage: JobStage
      current: number
      total: number
    }
  | {
      type: 'completed'
      created: number
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

export async function createImport(file: File): Promise<CreateImportResponse> {
  const form = new FormData()
  form.append('file', file)

  const response = await apiClient.post<CreateImportResponse>('/imports', form, {
    timeout: 30_000,
  })
  return response.data
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

export function getImportErrorMessage(error: unknown): string {
  if (!axios.isAxiosError(error)) {
    return 'Не удалось запустить импорт.'
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
