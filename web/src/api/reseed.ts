import { api } from './client';

export interface Reason {
  reason: string;
  count: number;
}

export interface Candidate {
  id: number;
  source_hash: string;
  source_name: string;
  source_site?: string;
  save_path: string;
  size: number;
  pieces_hash: string;
  target_site: string;
  target_torrent_id: string;
  evidence: string;
  needs_confirmation: boolean;
  note: string;
}

export interface Preview {
  source_client_id: string;
  read: {
    total: number;
    incomplete: number;
    recognized: Record<string, number>;
    unrecognized: Reason[];
    without_pieces: Reason[];
  };
  sites: { site_id: string; queried: number; found: number; already_seeding: number; error?: string }[];
  imports: ImportReport[];
  candidates: Candidate[];
}

export interface ImportReport {
  file: string;
  site?: string;
  outcome: 'candidate' | 'seeding' | 'partial' | 'none' | 'invalid';
  detail: string;
}

export interface ItemResult {
  candidate_id: number;
  target_site: string;
  source_name: string;
  status: 'success' | 'skipped' | 'failed';
  step: string;
  message: string;
}

export interface ExecuteSummary {
  run_id: string;
  success: number;
  skipped: number;
  failed: number;
  not_attempted: number;
  items: ItemResult[];
}

export interface Task<T> {
  id: string;
  kind: string;
  status: 'running' | 'done' | 'failed' | 'cancelled';
  progress: { phase: string; done: number; total: number };
  error?: string;
  result?: T;
}

export interface HistoryEntry {
  id: number;
  run_id: string;
  source_name: string;
  source_site?: string;
  target_site: string;
  target_torrent_id: string;
  target_client: string;
  status: 'success' | 'failed' | 'skipped';
  step: string;
  message: string;
  created_at: string;
}

export const startPreview = (source_client_id: string, target_site_ids: string[]) =>
  api.post<{ task_id: string }>('/reseed/preview', { source_client_id, target_site_ids });

/** `files` carry each `.torrent` base64-encoded. */
export const startImport = (source_client_id: string, files: { name: string; data: string }[]) =>
  api.post<{ task_id: string }>('/reseed/import', { source_client_id, files });

export const startExecute = (data: {
  preview_id: string;
  target_client_id: string;
  candidate_ids: number[];
  confirmed_risky_ids: number[];
}) => api.post<{ task_id: string }>('/reseed/execute', data);

export const fetchTask = <T>(id: string) => api.get<Task<T>>(`/tasks/${id}`);

export const cancelTask = (id: string) => api.post<{ cancelling: boolean }>(`/tasks/${id}/cancel`);

export const fetchHistory = (limit = 100) => api.get<HistoryEntry[]>(`/reseed/history?limit=${limit}`);
