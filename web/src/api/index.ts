import { api } from './client';

export interface IndexStats {
  total_entries: number;
  sites: Array<{ site_id: string; count: number }>;
}

export interface ImportResult {
  total: number;
  imported: number;
  skipped: number;
  unrecognized: number;
}

export const fetchIndexStats = () => api.get<IndexStats>('/index/stats');

export const importFromClient = (clientId: string) =>
  api.post<ImportResult>(`/index/import/${clientId}`);

export const clearIndex = () =>
  api.delete<{ cleared: boolean }>('/index');

export const clearSiteIndex = (siteId: string) =>
  api.delete<{ cleared: boolean; site_id: string }>(`/index/${siteId}`);
