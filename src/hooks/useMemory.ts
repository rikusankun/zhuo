import { useCallback, useEffect, useRef, useState } from 'react';
import { api, isApiError, type Memory } from '../api/client';
import type { HookReturn } from './types';

export type UseMemoryData = {
  memories: Memory[];
  memoryTotal: number;
  stateText: string;
};

export type UseMemoryActions = {
  setStateText: (text: string) => void;
  loadMemory: () => Promise<void>;
  saveState: (text: string) => Promise<{ ok: boolean; error?: string }>;
  clearMemory: () => void;
};

export function useMemory(cardId: string): HookReturn<UseMemoryData, UseMemoryActions> {
  const [memories, setMemories] = useState<Memory[]>([]);
  const [memoryTotal, setMemoryTotal] = useState(0);
  const [stateText, setStateText] = useState('{}');
  const memoryRequest = useRef(0);
  const [loading, setLoading] = useState(false);
  const [memError, setMemError] = useState<string | null>(null);
  const [memSuccess, setMemSuccess] = useState<string | null>(null);

  const clearMemory = useCallback(() => {
    memoryRequest.current++;
    setMemories([]);
    setMemoryTotal(0);
    setStateText('{}');
  }, []);

  const loadMemory = useCallback(async () => {
    const version = ++memoryRequest.current;
    if (!cardId) return;
    setLoading(true);
    setMemError(null);
    void Promise.all([api.memory.list(cardId), api.state.get(cardId)]).then(([m, s]) => {
      if (version !== memoryRequest.current) return;
      if (isApiError(m)) throw new Error(m.error);
      if (isApiError(s)) throw new Error(s.error);
      setMemories(m.memories || []);
      setMemoryTotal(m.total || 0);
      setStateText(JSON.stringify(s.state || {}, null, 2));
    }).catch(e => {
      if (version === memoryRequest.current) {
        setMemError(String(e));
        throw e;
      }
    }).finally(() => {
      if (version === memoryRequest.current) setLoading(false);
    });
  }, [cardId]);

  const saveState = useCallback(async (text: string): Promise<{ ok: boolean; error?: string }> => {
    setLoading(true);
    setMemError(null);
    setMemSuccess(null);
    try {
      const state = JSON.parse(text);
      if (!state || Array.isArray(state) || typeof state !== 'object') throw new Error('state must be an object');
      const x = await api.state.save(cardId, state);
      if (isApiError(x)) {
        setMemError(x.error);
        setLoading(false);
        return { ok: false, error: x.error };
      }
      setMemSuccess('角色状态已保存');
      setLoading(false);
      return { ok: true };
    } catch {
      const msg = '角色状态必须是合法 JSON 对象';
      setMemError(msg);
      setLoading(false);
      return { ok: false, error: msg };
    }
  }, [cardId]);

  useEffect(() => {
    clearMemory();
    if (cardId) {
      void loadMemory();
    }
    return () => { memoryRequest.current++; };
  }, [cardId, clearMemory, loadMemory]);

  return {
    data: { memories, memoryTotal, stateText },
    actions: { setStateText, loadMemory, saveState, clearMemory },
    status: {
      isLoading: loading,
      error: memError,
      success: memSuccess,
    },
  };
}
