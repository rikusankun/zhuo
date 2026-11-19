import React from 'react';
import { Bot, Copy, Plus, RefreshCw } from 'lucide-react';
import type { Card, Message } from '../api/client';
import { usePersona } from '../hooks/useConnectionContext';

export type ChatViewProps = {
  messages: Message[];
  card: Card | undefined;
  busy: boolean;
  loading: boolean;
  cardId: string;
  newChat: () => void;
  regenerate: () => void;
};

export function ChatView({
  messages,
  card,
  busy,
  loading,
  cardId,
  newChat,
  regenerate,
}: ChatViewProps) {
  const { persona } = usePersona();
  const log = React.useRef<HTMLDivElement>(null);
  const initials = (name: string) => name.trim().slice(0, 2).toUpperCase() || 'AI';

  React.useEffect(() => {
    log.current?.scrollTo({ top: log.current.scrollHeight, behavior: 'smooth' });
  }, [messages]);

  return (
    <div className="conversation" ref={log}>
      {!messages.length && (
        <div className="empty-chat">
          <div className="empty-orbit"><Bot size={34} /></div>
          <h1>和 {card?.name || '角色'} 开始故事</h1>
          <p>开场白、Persona 与世界信息会自动进入上下文。</p>
          <button disabled={busy || loading || !cardId} onClick={newChat}>
            <Plus size={17} />新建对话
          </button>
        </div>
      )}
      {messages.map((m, i) => (
        <article key={i} className={`message ${m.role}`}>
          <span className={`avatar ${m.role === 'user' ? 'user' : ''}`}>
            {m.role === 'user' ? initials(persona.name) : initials(card?.name || '')}
          </span>
          <div className="message-body">
            <div className="message-meta">
              <strong>{m.role === 'user' ? persona.name : card?.name}</strong>
              <span>{m.role === 'user' ? 'Persona' : '角色'}</span>
            </div>
            <div className="message-content">
              {m.content || (busy && i === messages.length - 1 ? <span className="typing"><i /><i /><i /></span> : '')}
            </div>
            {m.content && (
              <div className="message-actions">
                <button onClick={() => navigator.clipboard.writeText(m.content)}>
                  <Copy size={14} />复制
                </button>
                {m.role === 'assistant' && i === messages.length - 1 && (
                  <button disabled={busy || loading} onClick={regenerate}>
                    <RefreshCw size={14} />重生成
                  </button>
                )}
              </div>
            )}
          </div>
        </article>
      ))}
    </div>
  );
}
