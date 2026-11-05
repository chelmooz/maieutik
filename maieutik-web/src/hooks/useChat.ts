import { useState, useCallback } from 'react';
import axios, { AxiosError } from 'axios';

interface ChatMessage { role: 'user' | 'assistant'; content: string; }
interface ChatResponse { message: ChatMessage; choices?: Array<{message: ChatMessage}>; }

const API_BASE_URL = import.meta.env.VITE_API_URL || '';

export function useChat() {
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  const sendMessage = useCallback(async (message: string): Promise<ChatResponse | null> => {
    setIsLoading(true);
    setError(null);
    try {
      const response = await axios.post<ChatResponse>(`${API_BASE_URL}/chat`, { messages: [{ role: 'user', content: message }] });
      return response.data;
    } catch (err) {
      const error = err instanceof AxiosError ? new Error(err.message) : new Error('Network error');
      setError(error);
      throw error;
    } finally {
      setIsLoading(false);
    }
  }, []);

  return { sendMessage, isLoading, error };
}
