import { useCallback, useEffect, useRef, useState } from 'react';
import { api, isApiError, type Memory } from '../api/client';

export function useMemory(cardId: string) {
  const [memories, setMemories] = useState<Memory[]>([]);
  const [memoryTotal, setMemoryTotal] = useState(0);
  const [stateText, setStateText] = useState('{}');
  const memoryRequest = useRef(0);

  const clearMemory = useCallback(() => {
    memoryRequest.current++;
    setMemories([]);
    setMemoryTotal(0);
    setStateText('{}');
  }, []);

  const loadMemory = useCallback(async () => {
    const version = ++memoryRequest.current;
    if (!cardId) return;
    void Promise.all([api.memory.list(cardId), api.state.get(cardId)]).then(([m, s]) => {
      if (version !== memoryRequest.current) return;
      if (isApiError(m)) throw new Error(m.error);
      if (isApiError(s)) throw new Error(s.error);
      setMemories(m.memories || []);
      setMemoryTotal(m.total || 0);
      setStateText(JSON.stringify(s.state || {}, null, 2));
    }).catch(e => { if (version === memoryRequest.current) throw e; });
  }, [cardId]);

  const saveState = useCallback(async (text: string): Promise<{ ok: boolean; error?: string }> => {
    try {
      const state = JSON.parse(text);
      if (!state || Array.isArray(state) || typeof state !== 'object') throw new Error('state must be an object');
      const x = await api.state.save(cardId, state);
      if (isApiError(x)) return { ok: false, error: x.error };
      return { ok: true };
    } catch {
      return { ok: false, error: '角色状态必须是合法 JSON 对象' };
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
    memories,
    memoryTotal,
    stateText,
    setStateText,
    loadMemory,
    saveState,
  };
}
