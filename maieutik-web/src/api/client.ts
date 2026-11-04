import axios from 'axios';

const API_URL = import.meta.env.VITE_API_URL ?? 'http://localhost:3000';

export const api = axios.create({
  baseURL: API_URL,
});

export const fetchFiles = () => api.get<string[]>('/api/files');
export const searchFiles = (q: string) => api.post('/api/search', { query: q });
