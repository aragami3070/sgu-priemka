import axios from 'axios'
import { apiClient } from './client'

export interface ResultItem {
  owner: string
  filename: string
  created_at: string
  size: number
}

interface ResultListResponse {
  items: ResultItem[]
}

function resultPath(result: Pick<ResultItem, 'owner' | 'filename'>): string {
  return `/results/${encodeURIComponent(result.owner)}/${encodeURIComponent(result.filename)}`
}

export async function listResults(): Promise<ResultItem[]> {
  const response = await apiClient.get<ResultListResponse>('/results')
  return response.data.items
}

export async function downloadResult(result: ResultItem): Promise<void> {
  const response = await apiClient.get<Blob>(resultPath(result), {
    responseType: 'blob',
  })
  const objectUrl = URL.createObjectURL(response.data)
  const link = document.createElement('a')
  link.href = objectUrl
  link.download = result.filename
  document.body.append(link)
  link.click()
  link.remove()
  window.setTimeout(() => URL.revokeObjectURL(objectUrl), 0)
}

export async function deleteResult(result: ResultItem): Promise<void> {
  await apiClient.delete(resultPath(result))
}

export function getResultErrorMessage(
  error: unknown,
  fallback: string,
): string {
  if (!axios.isAxiosError(error)) return fallback
  if (error.response?.status === 401) {
    return 'Сессия отсутствует или истекла. Войдите снова.'
  }
  if (error.response?.status === 404) {
    return 'Файл уже удалён или отсутствует на сервере.'
  }
  return error.response ? fallback : 'Не удалось подключиться к backend.'
}
