import { create } from 'zustand';
import { fetchFiles } from '../api/client';

interface FilesState {
  files: string[];
  loading: boolean;
  fetch: () => Promise<void>;
}

export const useFiles = create<FilesState>((set) => ({
  files: [],
  loading: false,
  fetch: async () => {
    set({ loading: true });
    const { data } = await fetchFiles();
    set({ files: data, loading: false });
  },
}));
