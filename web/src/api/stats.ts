import { api } from './client';

export interface Stats {
  index: {
    total_entries: number;
    sites: Array<{ site_id: string; count: number }>;
  };
  clients: number;
  sites: number;
  today: {
    success: number;
    failed: number;
  };
}

export const fetchStats = () => api.get<Stats>('/stats');
