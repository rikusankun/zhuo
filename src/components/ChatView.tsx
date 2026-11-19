import React from 'react';
import { Bot, Copy, Plus, RefreshCw, Send, Square } from 'lucide-react';
import type { Card, Lore, Message } from '../api/client';
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

export function ChatComposer({
  input,
  setInput,
  busy,
  loading,
  cardId,
  chatId,
  send,
  abort,
  promptChars,
  promptTokens,
  usage,
  loreHits,
}: {
  input: string;
  setInput: (v: string) => void;
  busy: boolean;
  loading: boolean;
  cardId: string;
  chatId: string;
  send: (override?: string, regenerate?: boolean) => void;
  abort: React.RefObject<AbortController | null>;
  promptChars: number | null;
  promptTokens: number | null;
  usage: number | null;
  loreHits: (text: string) => Lore[];
}) {
  const { persona } = usePersona();
  return (
    <div className="composer-wrap">
      {(promptChars !== null || usage) && (
        <div className="usage">
          {promptChars !== null && (
            <>
              提示词 {promptChars}/1500 字
              {promptTokens !== null ? ` · 估算 ${promptTokens} tokens` : ''}
            </>
          )}
          {promptChars !== null && usage ? ' · ' : ''}
          {usage && `本轮 ${usage} tokens`}
        </div>
      )}
      <div className="composer">
        <textarea
          rows={1}
          value={input}
          disabled={busy || loading || !cardId}
          onChange={e => setInput(e.target.value)}
          onKeyDown={e => {
            if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault();
              send();
            }
          }}
          placeholder={chatId ? `以 ${persona.name} 的身份发送消息…` : '新建对话并发送消息…'}
        />
        <button
          className={`send ${busy ? 'stop' : ''}`}
          onClick={() => (busy ? abort.current?.abort() : send())}
          disabled={!busy && (!input.trim() || loading || !cardId)}
        >
          {busy ? <Square size={17} /> : <Send size={18} />}
        </button>
      </div>
      <div className="composer-hint">
        <span>Enter 发送 · Shift + Enter 换行</span>
        <span>
          {loreHits(input).length
            ? `${loreHits(input).length} 条 World Info 关键词匹配`
            : '上下文自动管理'}
        </span>
      </div>
    </div>
  );
}
