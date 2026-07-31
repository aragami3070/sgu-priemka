import axios from "axios";

const configuredBaseUrl = import.meta.env.VITE_API_BASE_URL?.trim();

export const apiClient = axios.create({
  baseURL: configuredBaseUrl || "/api",
  timeout: 15_000,
  withCredentials: true,
  headers: {
    Accept: "application/json",
  },
});
