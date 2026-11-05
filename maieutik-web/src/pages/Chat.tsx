import { useState, useRef, FormEvent, useCallback } from 'react';
import { useChat } from '../hooks/useChat';

interface Message {
  id: number;
  role: 'user' | 'assistant';
  content: string;
}

let messageId = 0;

export function Chat() {
  const [messages, setMessages] = useState<Message[]>([]);
  const [inputValue, setInputValue] = useState('');
  const { sendMessage, isLoading, error } = useChat();

  const handleSubmit = useCallback(async (e: FormEvent) => {
    e.preventDefault();
    if (!inputValue.trim() || isLoading) return;

    const userMessage: Message = {
      id: ++messageId,
      role: 'user',
      content: inputValue
    };

    setMessages(prev => [...prev, userMessage]);
    setInputValue('');

    try {
      const response = await sendMessage(inputValue);
      setMessages(prev => [...prev, {
        id: ++messageId,
        role: 'assistant',
        content: response?.message?.content || 'No response'
      }]);
    } catch (err) {
      console.error('Chat error:', err);
    }
  }, [inputValue, isLoading, sendMessage]);

  return (
    <div className="min-h-screen bg-gray-50 flex flex-col">
      <div className="flex-1 overflow-y-auto p-4">
        <h1 className="text-2xl font-bold mb-4 text-center">Maieutik Chat</h1>
        <div className="space-y-4">
          {messages.map(msg => (
            <div key={msg.id} className={`flex ${msg.role === 'user' ? 'justify-end' : 'justify-start'}`}>
              <div className={`max-w-xs px-4 py-2 rounded-lg ${msg.role === 'user' ? 'bg-blue-500 text-white rounded-br-lg' : 'bg-gray-200 text-gray-800 rounded-bl-lg'}`}>
                {msg.content}
              </div>
            </div>
          ))}
        </div>
      </div>
      <form onSubmit={handleSubmit} className="p-4 bg-white border-t">
        <div className="flex gap-2">
          <input
            type="text"
            value={inputValue}
            onChange={(e) => setInputValue(e.target.value)}
            placeholder="Posez votre question..."
            className="flex-1 px-4 py-2 border rounded-lg focus:ring-2 focus:ring-blue-500"
            disabled={isLoading}
          />
          <button type="submit" disabled={isLoading || !inputValue.trim()} className="px-6 py-2 bg-blue-500 text-white rounded-lg hover:bg-blue-600 disabled:opacity-50">
            {isLoading ? '...' : 'Envoyer'}
          </button>
        </div>
      </form>
    </div>
  );
}
