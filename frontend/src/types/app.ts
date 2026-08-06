export type AppView = 'import' | 'results' | 'groups'

export interface AuthUser {
  username: string
  expiresAt?: string
  isSkipped?: boolean
}
