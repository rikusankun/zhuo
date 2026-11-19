import React from 'react';
import { Square, Send } from 'lucide-react';
import type { Model } from '../api/client';
import { usePersona } from '../hooks/useConnectionContext';

export type ChatComposerProps = {
  input: string;
  setInput: (v: string) => void;
  busy: boolean;
  loading: boolean;
  cardId: string;
  chatId: string;
  send: (override?: string, regenerate?: boolean, model?: string) => void;
  abort: React.RefObject<AbortController | null>;
  promptChars: number | null;
  promptTokens: number | null;
  usage: number | null;
  loreHits: (text: string) => import('../api/client').Lore[];
  selectedModel: string;
  setSelectedModel: (v: string) => void;
  availableModels: Model[];
};

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
  selectedModel,
  setSelectedModel,
  availableModels,
}: ChatComposerProps) {
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
        <select
          className="model-select"
          value={selectedModel}
          onChange={e => setSelectedModel(e.target.value)}
          disabled={busy || loading || !cardId}
        >
          <option value="">使用默认模型</option>
          {availableModels.map(m => (
            <option key={m.id} value={m.id}>{m.displayName}</option>
          ))}
        </select>
        <textarea
          rows={1}
          value={input}
          disabled={busy || loading || !cardId}
          onChange={e => setInput(e.target.value)}
          onKeyDown={e => {
            if (e.key === 'Enter' && !e.shiftKey && !e.nativeEvent.isComposing) {
              e.preventDefault();
              send(undefined, false, selectedModel || undefined);
            }
          }}
          placeholder={chatId ? `以 ${persona.name} 的身份发送消息…` : '新建对话并发送消息…'}
        />
        <button
          className={`send ${busy ? 'stop' : ''}`}
          onClick={() => (busy ? abort.current?.abort() : send(undefined, false, selectedModel || undefined))}
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
