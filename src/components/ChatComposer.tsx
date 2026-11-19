import React from 'react';
import { Square, Send } from 'lucide-react';
import type { Lore } from '../api/client';
import { usePersona } from '../hooks/useConnectionContext';

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
