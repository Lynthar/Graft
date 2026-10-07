import { api } from './client';

export interface AuthStatus {
  required: boolean;
  authenticated: boolean;
}

export const fetchAuth = () => api.get<AuthStatus>('/auth');
export const login = (password: string) => api.post<{ ok: boolean }>('/login', { password });
export const logout = () => api.post<{ ok: boolean }>('/logout');
