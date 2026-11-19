import { useCallback, useEffect, useRef, useState } from 'react';
import { api, isApiError, type Connection, type Model, type RegexScript } from '../api/client';

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

export function useConnection() {
  const [connection, setConnection] = useState<Connection>(defaults);
  const [saved, setSaved] = useState(false);
  const [models, setModels] = useState<Model[]>([]);
  const [modelsLoading, setModelsLoading] = useState(false);
  const [modelsError, setModelsError] = useState('');
  const [connectionStatus, setConnectionStatus] = useState<'idle' | 'testing' | 'ok' | 'error'>('idle');
  const [modelFilter, setModelFilter] = useState('');
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [compat, setCompat] = useState({ worldbook: 0, regex: 0 });
  const [regexScripts, setRegexScripts] = useState<RegexScript[]>([]);

  const connectionRef = useRef(connection);
  connectionRef.current = connection;

  const gpuSaveTimer = useRef<number | null>(null);
  const saveQueue = useRef<Promise<void>>(Promise.resolve());

  const worldInput = useRef<HTMLInputElement>(null);
  const presetInput = useRef<HTMLInputElement>(null);
  const regexInput = useRef<HTMLInputElement>(null);

  const visibleModels = models.filter(m => m.displayName.toLowerCase().includes(modelFilter.toLowerCase()));

  const normalizeBaseUrl = useCallback((url: string) => {
    return url && !/^https?:\/\//i.test(url) ? `http://${url}` : url;
  }, []);

  const loadModels = useCallback(async () => {
    setModelsLoading(true);
    setModelsError('');
    try {
      const payload = await api.connection.models({
        provider: connection.provider,
        base_url: normalizeBaseUrl(connection.base_url),
        api_key: connection.api_key,
      });
      if (isApiError(payload)) throw new Error(payload.error || '未知错误');
      const ms = Array.isArray(payload.models) ? payload.models : [];
      setModels(ms.map(m => ({ id: m, displayName: m })));
    } catch (e) {
      setModelsError(`无法读取模型列表：${String(e)}`);
    } finally {
      setModelsLoading(false);
    }
  }, [connection, normalizeBaseUrl]);

  const testConnection = useCallback(async () => {
    setConnectionStatus('testing');
    setModelsError('');
    try {
      const payload = await api.connection.models({
        provider: connection.provider,
        base_url: normalizeBaseUrl(connection.base_url),
        api_key: connection.api_key,
      });
      if (isApiError(payload)) throw new Error(payload.error || '未知错误');
      const ms = Array.isArray(payload.models) ? payload.models : [];
      setConnectionStatus(ms.length ? 'ok' : 'idle');
      setNotice(ms.length ? `连接正常，发现 ${ms.length} 个模型` : '连接正常，未发现模型');
    } catch (e) {
      setConnectionStatus('error');
      setModelsError(`连接测试失败：${String(e)}`);
    }
  }, [connection, normalizeBaseUrl, setNotice]);

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
      if (announce) setNotice('连接设置已保存');
      return true;
    } catch (e) {
      setError(`保存连接设置失败：${String(e)}`);
      return false;
    }
  }, [setError, setNotice]);

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

  const importAsset = useCallback(async (kind: 'worldbook' | 'preset' | 'regex', file?: File) => {
    if (!file) return;
    const data = await new Promise<string>((resolve, reject) => {
      const r = new FileReader();
      r.onload = () => resolve(String(r.result).split(',')[1] || '');
      r.onerror = () => reject(r.error);
      r.readAsDataURL(file);
    });
    const x = await api.compatibility.import(kind, file.name, data);
    if (isApiError(x)) {
      setError(String(x.error));
      return;
    }
    if (kind === 'preset' && x.connection) {
      setConnection({ ...defaults, ...x.connection });
    }
    if (kind === 'worldbook') {
      setCompat(v => ({ ...v, worldbook: x.entries || 0 }));
    }
    if (kind === 'regex') {
      setRegexScripts(x.scripts || []);
      setCompat(v => ({ ...v, regex: x.scripts?.length || 0 }));
    }
    setNotice(kind === 'worldbook' ? `世界书已导入：${x.entries} 条目` : kind === 'preset' ? 'ST 预设已应用' : `Regex 已导入：${x.scripts?.length || 0} 条脚本`);
  }, [defaults, setCompat, setConnection, setError, setNotice, setRegexScripts]);

  const exportCard = useCallback(async (cardId: string, format: 'json' | 'png') => {
    if (!cardId) return;
    try {
      const x = await api.cards.export(cardId, format);
      if (isApiError(x)) {
        setError(x.error);
        return;
      }
      const a = document.createElement('a');
      a.href = `data:${x.mime};base64,${x.data}`;
      a.download = x.filename;
      a.click();
      setNotice(`已导出 ${x.filename}`);
    } catch (e) {
      setError(String(e));
    }
  }, [setError, setNotice]);

  const refreshModels = useCallback(async (saveFirst = true) => {
    if (saveFirst) {
      await saveConnection(connection, false);
    }
    await loadModels();
  }, [connection, loadModels, saveConnection]);

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
    setSaved,
    models,
    modelsLoading,
    modelsError,
    connectionStatus,
    modelFilter,
    setModelFilter,
    visibleModels,
    compat,
    setCompat,
    regexScripts,
    setRegexScripts,
    worldInput,
    presetInput,
    regexInput,
    error,
    setError,
    notice,
    setNotice,
    normalizeBaseUrl,
    loadModels,
    testConnection,
    refreshModels,
    saveConnection,
    changeNumGpu,
    importAsset,
    exportCard,
  };
}
