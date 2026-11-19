import { useCallback, useState } from 'react';
import { api, isApiError, type Model } from '../api/client';
import { useToast } from './useToast';

export type UseModelManagerInput = {
  provider: string;
  base_url: string;
  api_key: string;
};

export function useModelManager(input: UseModelManagerInput) {
  const { addToast } = useToast();
  const [models, setModels] = useState<Model[]>([]);
  const [modelsLoading, setModelsLoading] = useState(false);
  const [connectionStatus, setConnectionStatus] = useState<'idle' | 'testing' | 'ok' | 'error'>('idle');
  const [modelFilter, setModelFilter] = useState('');

  const visibleModels = models.filter(m => m.displayName.toLowerCase().includes(modelFilter.toLowerCase()));

  const normalizeBaseUrl = useCallback((url: string) => {
    return url && !/^https?:\/\//i.test(url) ? `http://${url}` : url;
  }, []);

  const loadModels = useCallback(async () => {
    setModelsLoading(true);
    try {
      const payload = await api.connection.models({
        provider: input.provider,
        base_url: normalizeBaseUrl(input.base_url),
        api_key: input.api_key,
      });
      if (isApiError(payload)) throw new Error(payload.error || '未知错误');
      const ms = Array.isArray(payload.models) ? payload.models : [];
      setModels(ms.map(m => ({ id: m, displayName: m })));
      setConnectionStatus(ms.length ? 'ok' : 'idle');
    } catch (e) {
      addToast('error', `无法读取模型列表：${String(e)}`);
      setConnectionStatus('error');
    } finally {
      setModelsLoading(false);
    }
  }, [input, normalizeBaseUrl, addToast]);

  const testConnection = useCallback(async () => {
    setConnectionStatus('testing');
    try {
      const payload = await api.connection.models({
        provider: input.provider,
        base_url: normalizeBaseUrl(input.base_url),
        api_key: input.api_key,
      });
      if (isApiError(payload)) throw new Error(payload.error || '未知错误');
      const ms = Array.isArray(payload.models) ? payload.models : [];
      setConnectionStatus(ms.length ? 'ok' : 'idle');
      addToast('success', ms.length ? `连接正常，发现 ${ms.length} 个模型` : '连接正常，未发现模型');
    } catch (e) {
      setConnectionStatus('error');
      addToast('error', `连接测试失败：${String(e)}`);
    }
  }, [input, normalizeBaseUrl, addToast]);

  const refreshModels = useCallback(async (saveFirst = true) => {
    if (saveFirst) {
      // save is handled by parent via useConnectionConfig.saveConnection
    }
    await loadModels();
  }, [loadModels]);

  return {
    models,
    modelsLoading,
    connectionStatus,
    modelFilter,
    setModelFilter,
    visibleModels,
    loadModels,
    testConnection,
    refreshModels,
  };
}
