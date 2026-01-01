import { api } from './client';

export interface Client {
  id: string;
  name: string;
  client_type: 'qbittorrent' | 'transmission';
  host: string;
  port: number;
  username?: string;
  use_https: boolean;
  enabled: boolean;
}

export interface CreateClientRequest {
  name: string;
  client_type: 'qbittorrent' | 'transmission';
  host: string;
  port: number;
  username?: string;
  password?: string;
  use_https: boolean;
}

export const fetchClients = () => api.get<Client[]>('/clients');

export const createClient = (data: CreateClientRequest) =>
  api.post<Client>('/clients', data);

export const testClient = (id: string) =>
  api.post<{ success: boolean; message: string }>(`/clients/${id}/test`);

export const deleteClient = (id: string) =>
  api.delete<{ deleted: boolean }>(`/clients/${id}`);
