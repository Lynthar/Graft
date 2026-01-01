import { api } from './client';

export interface Site {
  id: string;
  name: string;
  base_url: string;
  template_type: string;
  has_passkey: boolean;
  has_cookie: boolean;
  enabled: boolean;
}

export interface AvailableSite {
  id: string;
  name: string;
  base_url: string;
  template_type: string;
  tracker_domains: string[];
}

export interface CreateSiteRequest {
  id: string;
  name: string;
  base_url: string;
  passkey?: string;
  cookie?: string;
}

export const fetchSites = () => api.get<Site[]>('/sites');

export const fetchAvailableSites = () => api.get<AvailableSite[]>('/sites/available');

export const createSite = (data: CreateSiteRequest) =>
  api.post<Site>('/sites', data);

export const deleteSite = (id: string) =>
  api.delete<{ deleted: boolean }>(`/sites/${id}`);
