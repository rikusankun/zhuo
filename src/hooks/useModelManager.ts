import { useCallback, useRef, useState } from 'react';
import { api, isApiError, type Model } from '../api/client';
import { useToast } from './useToast';
import type { HookReturn } from './types';

export type UseModelManagerInput = {
  provider: string;
  base_url: string;
  api_key: string;
};

export type UseModelManagerData = {
  models: Model[];
  visibleModels: Model[];
  connectionStatus: 'idle' | 'testing' | 'ok' | 'error';
  modelFilter: string;
  selectedModel: string;
};

export type UseModelManagerActions = {
  setModelFilter: (v: string) => void;
  setSelectedModel: (v: string) => void;
  loadModels: () => Promise<void>;
  testConnection: () => Promise<void>;
  refreshModels: (saveFirst?: boolean) => Promise<void>;
};

export function useModelManager(input: UseModelManagerInput): HookReturn<UseModelManagerData, UseModelManagerActions> {
  const { addToast } = useToast();
  const [models, setModels] = useState<Model[]>([]);
  const [loading, setLoading] = useState(false);
  const [connectionStatus, setConnectionStatus] = useState<'idle' | 'testing' | 'ok' | 'error'>('idle');
  const [modelFilter, setModelFilter] = useState('');
  const [selectedModel, setSelectedModel] = useState('');
  const [statusError, setStatusError] = useState<string | null>(null);
  const [statusSuccess, setStatusSuccess] = useState<string | null>(null);

  const versionRef = useRef(0);

  const visibleModels = models.filter(m => m.displayName.toLowerCase().includes(modelFilter.toLowerCase()));

  const normalizeBaseUrl = useCallback((url: string) => {
    return url && !/^https?:\/\//i.test(url) ? `http://${url}` : url;
  }, []);

  const loadModels = useCallback(async () => {
    const version = ++versionRef.current;
    setLoading(true);
    setConnectionStatus('testing');
    setStatusError(null);
    try {
      const payload = await api.connection.models({
        provider: input.provider,
        base_url: normalizeBaseUrl(input.base_url),
        api_key: input.api_key,
      });
      if (version !== versionRef.current) return;
      if (isApiError(payload)) throw new Error(payload.error || '未知错误');
      const ms = Array.isArray(payload.models) ? payload.models : [];
      setModels(ms.map(m => ({ id: m, displayName: m })));
      setConnectionStatus(ms.length ? 'ok' : 'idle');
      setStatusSuccess(ms.length ? `发现 ${ms.length} 个模型` : '未发现模型');
    } catch (e) {
      if (version !== versionRef.current) return;
      setConnectionStatus('error');
      const msg = `无法读取模型列表：${String(e)}`;
      setStatusError(msg);
      addToast('error', msg);
    } finally {
      if (version === versionRef.current) setLoading(false);
    }
  }, [input, normalizeBaseUrl, addToast]);

  const testConnection = useCallback(async () => {
    const version = ++versionRef.current;
    setLoading(true);
    setConnectionStatus('testing');
    setStatusError(null);
    setStatusSuccess(null);
    try {
      const payload = await api.connection.models({
        provider: input.provider,
        base_url: normalizeBaseUrl(input.base_url),
        api_key: input.api_key,
      });
      if (version !== versionRef.current) return;
      if (isApiError(payload)) throw new Error(payload.error || '未知错误');
      const ms = Array.isArray(payload.models) ? payload.models : [];
      setConnectionStatus(ms.length ? 'ok' : 'idle');
      setStatusSuccess(ms.length ? `连接正常，发现 ${ms.length} 个模型` : '连接正常，未发现模型');
      addToast('success', ms.length ? `连接正常，发现 ${ms.length} 个模型` : '连接正常，未发现模型');
    } catch (e) {
      if (version !== versionRef.current) return;
      setConnectionStatus('error');
      const msg = `连接测试失败：${String(e)}`;
      setStatusError(msg);
      addToast('error', msg);
    } finally {
      if (version === versionRef.current) setLoading(false);
    }
  }, [input, normalizeBaseUrl, addToast]);

  const refreshModels = useCallback(async (saveFirst = true) => {
    if (saveFirst) {
      // save is handled by parent via useConnectionConfig.saveConnection
    }
    await loadModels();
  }, [loadModels]);
  return {
    data: { models, visibleModels, connectionStatus, modelFilter, selectedModel },
    actions: { setModelFilter, setSelectedModel, loadModels, testConnection, refreshModels },
    status: {
      isLoading: loading,
      error: statusError,
      success: statusSuccess,
    },
  };
}
