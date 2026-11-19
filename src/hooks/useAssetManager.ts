import { useCallback, useEffect, useState, type Dispatch, type SetStateAction } from 'react';
import { api, isApiError, type Connection, type RegexScript } from '../api/client';
import { useToast } from './useToast';
import type { HookReturn } from './types';

export type UseAssetManagerInput = {
  connection: Connection;
  cardId: string;
};

export type UseAssetManagerData = {
  compat: { worldbook: number; regex: number };
  regexScripts: RegexScript[];
};

export type UseAssetManagerActions = {
  setCompat: Dispatch<SetStateAction<{ worldbook: number; regex: number }>>;
  setRegexScripts: Dispatch<SetStateAction<RegexScript[]>>;
  importAsset: (kind: 'worldbook' | 'preset' | 'regex', file?: File) => Promise<void>;
  exportCard: (format: 'json' | 'png') => Promise<void>;
};

export function useAssetManager(input: UseAssetManagerInput): HookReturn<UseAssetManagerData, UseAssetManagerActions> {
  const { addToast } = useToast();
  const [compat, setCompat] = useState({ worldbook: 0, regex: 0 });
  const [regexScripts, setRegexScripts] = useState<RegexScript[]>([]);
  const [loading, setLoading] = useState(false);
  const [assetError, setAssetError] = useState<string | null>(null);
  const [assetSuccess, setAssetSuccess] = useState<string | null>(null);

  // 启动时从后端加载已存的 regex_scripts，避免重启后 Regex 功能假死
  useEffect(() => {
    if (!input.cardId) return;
    void api.compatibility.get().then(result => {
      if (isApiError(result)) return;
      const scripts = result.regex_scripts;
      if (Array.isArray(scripts)) {
        setRegexScripts(scripts);
        setCompat(v => ({ ...v, regex: scripts.length }));
      }
    }).catch(() => {});
  }, [input.cardId]);

  const importAsset = useCallback(async (kind: 'worldbook' | 'preset' | 'regex', file?: File) => {
    if (!file) return;
    setLoading(true);
    setAssetError(null);
    setAssetSuccess(null);
    const data = await new Promise<string>((resolve, reject) => {
      const r = new FileReader();
      r.onload = () => resolve(String(r.result).split(',')[1] || '');
      r.onerror = () => reject(r.error);
      r.readAsDataURL(file);
    });
    const x = await api.compatibility.import(kind, file.name, data);
    if (isApiError(x)) {
      setAssetError(String(x.error));
      setLoading(false);
      addToast('error', String(x.error));
      return;
    }
    if (kind === 'preset' && x.connection) {
      // Preset connection applied by parent via useConnectionConfig.setConnection
    }
    if (kind === 'worldbook') {
      setCompat(v => ({ ...v, worldbook: x.entries || 0 }));
    }
    if (kind === 'regex') {
      setRegexScripts(x.scripts || []);
      setCompat(v => ({ ...v, regex: x.scripts?.length || 0 }));
    }
    setLoading(false);
    const msg = kind === 'worldbook' ? `世界书已导入：${x.entries} 条目` : kind === 'preset' ? 'ST 预设已应用' : `Regex 已导入：${x.scripts?.length || 0} 条脚本`;
    setAssetSuccess(msg);
    addToast('success', msg);
  }, [addToast]);

  const exportCard = useCallback(async (format: 'json' | 'png') => {
    if (!input.cardId) return;
    setLoading(true);
    setAssetError(null);
    setAssetSuccess(null);
    try {
      const x = await api.cards.export(input.cardId, format);
      if (isApiError(x)) {
        setAssetError(x.error);
        setLoading(false);
        addToast('error', x.error);
        return;
      }
      const a = document.createElement('a');
      a.href = `data:${x.mime};base64,${x.data}`;
      a.download = x.filename;
      a.click();
      setLoading(false);
      setAssetSuccess(`已导出 ${x.filename}`);
      addToast('success', `已导出 ${x.filename}`);
    } catch (e) {
      const msg = String(e);
      setAssetError(msg);
      setLoading(false);
      addToast('error', msg);
    }
  }, [input.cardId, addToast]);

  return {
    data: { compat, regexScripts },
    actions: { setCompat, setRegexScripts, importAsset, exportCard },
    status: {
      isLoading: loading,
      error: assetError,
      success: assetSuccess,
    },
  };
}
