import { useCallback, useEffect, useRef, useState } from 'react';
import { api, isApiError, type Connection } from '../api/client';
import { useToast } from './useToast';
import type { HookReturn } from './types';

const defaults: Connection = {
  provider: '',
  base_url: '',
  api_key: '',
  model: '',
  summary_model: '',
  num_gpu: null,
  temperature: 0.0,
  max_tokens: 0,
  top_p: 0.0,
  frequency_penalty: 0.0,
  presence_penalty: 0.0,
  repetition_penalty: 0.0,
};

export type UseConnectionConfigData = {
  connection: Connection;
  saved: boolean;
};

export type UseConnectionConfigActions = {
  setConnection: (c: Connection) => void;
  saveConnection: (next?: Connection, announce?: boolean) => Promise<boolean>;
  changeNumGpu: (value: number | null) => void;
  normalizeBaseUrl: (url: string) => string;
};

export function useConnectionConfig(): HookReturn<UseConnectionConfigData, UseConnectionConfigActions> {
  const { addToast } = useToast();
  const [connection, setConnection] = useState<Connection>(defaults);
  const [saved, setSaved] = useState(false);
  const [saving, setSaving] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [saveSuccess, setSaveSuccess] = useState<string | null>(null);

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
    setSaving(true);
    setSaveError(null);
    setSaveSuccess(null);

    const task = saveQueue.current.then(async () => {
      const result = await api.connection.save(target);
      if (isApiError(result)) throw new Error(result.error);
    });
    saveQueue.current = task.catch(() => {});

    try {
      await task;
      if (connectionRef.current === target) setSaved(true);
      if (announce) {
        setSaveSuccess('连接设置已保存');
        addToast('success', '连接设置已保存');
      }
      return true;
    } catch (e) {
      const msg = `保存连接设置失败：${String(e)}`;
      setSaveError(msg);
      addToast('error', msg);
      return false;
    } finally {
      setSaving(false);
    }
  }, [addToast]);

  const changeNumGpu = useCallback((value: number | null) => {
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
    data: { connection, saved },
    actions: { setConnection, saveConnection, changeNumGpu, normalizeBaseUrl },
    status: {
      isLoading: saving,
      error: saveError,
      success: saveSuccess,
    },
  };
}
