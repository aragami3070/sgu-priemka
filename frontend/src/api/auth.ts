import axios from "axios";
import { apiClient } from "./client";

interface LoginRequest {
  identifier: string;
  password: string;
}

export interface LoginResponse {
  username: string;
  expires_at: string;
}

export interface WhoAmIResponse {
  username: string;
}

interface ApiErrorEnvelope {
  message?: string;
  error?: {
    message?: string;
  };
}

export async function login(request: LoginRequest): Promise<LoginResponse> {
  const response = await apiClient.post<LoginResponse>("/auth/login", request);
  return response.data;
}

export async function logout(): Promise<void> {
  await apiClient.post("/auth/logout");
}

export async function whoami(): Promise<WhoAmIResponse> {
  const response = await apiClient.get<WhoAmIResponse>("/auth/whoami");
  return response.data;
}

export function getLoginErrorMessage(error: unknown): string {
  if (!axios.isAxiosError<ApiErrorEnvelope>(error)) {
    return "Не удалось выполнить вход. Попробуйте ещё раз.";
  }

  switch (error.response?.status) {
    case 401:
      return "Неверный логин или пароль.";
    case 403:
      return "У этой учётной записи нет доступа к сервису.";
    case 503:
      return "LDAP временно недоступен. Попробуйте позже.";
  }

  const backendMessage =
    error.response?.data?.error?.message ?? error.response?.data?.message;
  if (backendMessage) {
    return backendMessage;
  }

  return error.response
    ? "Backend вернул ошибку. Попробуйте ещё раз."
    : "Не удалось подключиться к backend.";
}
