export type AppView = 'import' | 'results'

export interface AuthUser {
  username: string
  expiresAt?: string
  isSkipped?: boolean
}
