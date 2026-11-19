import { useCallback, useEffect, useRef, useState } from 'react';
import { api, isApiError, type Connection } from '../api/client';
import { useToast } from './useToast';

const defaults: Connection = {
  provider: 'ollama',
  base_url: 'http://127.0.0.1:11434',
  api_key: '',
  model: 'qwen2.5-coder:3b',
  summary_model: '',
  num_gpu: 15,
  temperature: 0.8,
  max_tokens: 1024,
  top_p: 1.0,
  frequency_penalty: 0.0,
  presence_penalty: 0.0,
  repetition_penalty: 1.0,
};

export function useConnectionConfig() {
  const { addToast } = useToast();
  const [connection, setConnection] = useState<Connection>(defaults);
  const [saved, setSaved] = useState(false);

  const connectionRef = useRef(connection);
  connectionRef.current = connection;

  const gpuSaveTimer = useRef<number | null>(null);
  const saveQueue = useRef<Promise<void>>(Promise.resolve());

  const normalizeBaseUrl = useCallback((url: string) => {
    return url && !/^https?:\/\//i.test(url) ? `http://${url}` : url;
  }, []);

  const saveConnection = useCallback(async (next?: Connection, announce = true): Promise<boolean> => {
    if (gpuSaveTimer.current !== null) {
      window.clearTimeout(gpuSaveTimer.current);
      gpuSaveTimer.current = null;
    }
    const target = next ?? connectionRef.current;
    connectionRef.current = target;
    setConnection(target);
    setSaved(false);

    const task = saveQueue.current.then(async () => {
      const result = await api.connection.save(target);
      if (isApiError(result)) throw new Error(result.error);
    });
    saveQueue.current = task.catch(() => {});

    try {
      await task;
      if (connectionRef.current === target) setSaved(true);
      if (announce) addToast('success', '连接设置已保存');
      return true;
    } catch (e) {
      addToast('error', `保存连接设置失败：${String(e)}`);
      return false;
    }
  }, [addToast]);

  const changeNumGpu = useCallback((value: number) => {
    const next = { ...connectionRef.current, num_gpu: value };
    connectionRef.current = next;
    setConnection(next);
    setSaved(false);
    if (gpuSaveTimer.current !== null) {
      window.clearTimeout(gpuSaveTimer.current);
    }
    gpuSaveTimer.current = window.setTimeout(() => {
      gpuSaveTimer.current = null;
      void saveConnection(connectionRef.current, false);
    }, 400);
  }, [saveConnection]);

  useEffect(() => {
    return () => {
      if (gpuSaveTimer.current !== null) {
        window.clearTimeout(gpuSaveTimer.current);
      }
    };
  }, []);

  return {
    connection,
    setConnection,
    saved,
    normalizeBaseUrl,
    saveConnection,
    changeNumGpu,
  };
}
