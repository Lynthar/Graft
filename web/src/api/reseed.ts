import { api } from './client';

export interface ReseedMatch {
  source_hash: string;
  source_name: string;
  source_site?: string;
  target_site: string;
  target_torrent_id?: string;
  target_hash: string;
  save_path: string;
  size: number;
  confidence: number;
}

export interface PreviewResult {
  matches: ReseedMatch[];
  total_size: number;
}

export interface PreviewRequest {
  source_client_id: string;
  target_site_ids: string[];
}

export interface ExecuteRequest {
  source_client_id: string;
  target_client_id: string;
  target_site_ids: string[];
  add_paused?: boolean;
  skip_checking?: boolean;
}

export interface ExecuteResult {
  total: number;
  success: number;
  failed: number;
  skipped: number;
}

export interface HistoryEntry {
  id: number;
  info_hash: string;
  source_site?: string;
  target_site: string;
  status: 'success' | 'failed' | 'skipped';
  message?: string;
  created_at: string;
}

export interface HistoryQuery {
  limit?: number;
  offset?: number;
  status?: string;
}

export const previewReseed = (data: PreviewRequest) =>
  api.post<PreviewResult>('/reseed/preview', data);

export const executeReseed = (data: ExecuteRequest) =>
  api.post<ExecuteResult>('/reseed/execute', data);

export const fetchHistory = (query: HistoryQuery = {}) => {
  const params = new URLSearchParams();
  if (query.limit) params.set('limit', query.limit.toString());
  if (query.offset) params.set('offset', query.offset.toString());
  if (query.status) params.set('status', query.status);

  const queryString = params.toString();
  return api.get<HistoryEntry[]>(`/reseed/history${queryString ? `?${queryString}` : ''}`);
};
