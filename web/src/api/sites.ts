import { api } from './client';

export interface Site {
  id: string;
  name: string;
  base_url: string;
  template_type: 'nexusphp' | 'unit3d' | 'gazelle';
  download_pattern: string;
  has_passkey: boolean;
  has_cookie: boolean;
  has_authkey: boolean;
  enabled: boolean;
  rate_limit_rpm: number;
  daily_limit: number;
  builtin: boolean;
  domains: string[];
}

export interface CreateSiteRequest {
  id: string;
  name: string;
  base_url: string;
  template_type?: string;
  download_pattern?: string;
  passkey?: string;
  cookie?: string;
  authkey?: string;
  domains?: string[];
  rate_limit_rpm?: number;
  daily_limit?: number;
  enabled?: boolean;
}

/** Absent fields stay unchanged; an empty string clears a credential. */
export type UpdateSiteRequest = Partial<Omit<CreateSiteRequest, 'id' | 'template_type'>>;

export const fetchSites = () => api.get<Site[]>('/sites');

export const createSite = (data: CreateSiteRequest) => api.post<Site>('/sites', data);

export const updateSite = (id: string, data: UpdateSiteRequest) => api.put<Site>(`/sites/${id}`, data);

export const deleteSite = (id: string) => api.delete<{ deleted: boolean }>(`/sites/${id}`);
