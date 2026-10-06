import { api } from './client';

export interface Stats {
  clients: number;
  sites: number;
  today: {
    success: number;
    failed: number;
  };
  total_success: number;
}

export const fetchStats = () => api.get<Stats>('/stats');
