import axios from 'axios'
import { apiClient } from './client'

export interface ReplaceGroupsResponse {
  groups: number
}

export async function replaceGroups(file: File): Promise<ReplaceGroupsResponse> {
  const form = new FormData()
  form.append('file', file)
  const response = await apiClient.post<ReplaceGroupsResponse>('/groups', form, {
    timeout: 30_000,
  })
  return response.data
}

export function getGroupsErrorMessage(error: unknown): string {
  if (!axios.isAxiosError(error)) return 'Не удалось заменить файл групп.'
  if (error.response?.status === 401) return 'Сессия отсутствует или истекла. Войдите снова.'
  if (error.response?.status === 400) {
    const message = error.response.data
    return typeof message === 'string' ? message : 'Некорректный TOML-файл групп.'
  }
  return error.response ? 'Backend не смог заменить файл групп.' : 'Не удалось подключиться к backend.'
}
