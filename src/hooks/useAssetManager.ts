import { useCallback, useState } from 'react';
import { api, isApiError, type Connection, type RegexScript } from '../api/client';
import { useToast } from './useToast';

export type UseAssetManagerInput = {
  connection: Connection;
  cardId: string;
};

export function useAssetManager(input: UseAssetManagerInput) {
  const { addToast } = useToast();
  const [compat, setCompat] = useState({ worldbook: 0, regex: 0 });
  const [regexScripts, setRegexScripts] = useState<RegexScript[]>([]);

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
    addToast('success', kind === 'worldbook' ? `世界书已导入：${x.entries} 条目` : kind === 'preset' ? 'ST 预设已应用' : `Regex 已导入：${x.scripts?.length || 0} 条脚本`);
  }, [addToast]);

  const exportCard = useCallback(async (format: 'json' | 'png') => {
    if (!input.cardId) return;
    try {
      const x = await api.cards.export(input.cardId, format);
      if (isApiError(x)) {
        addToast('error', x.error);
        return;
      }
      const a = document.createElement('a');
      a.href = `data:${x.mime};base64,${x.data}`;
      a.download = x.filename;
      a.click();
      addToast('success', `已导出 ${x.filename}`);
    } catch (e) {
      addToast('error', String(e));
    }
  }, [input.cardId, addToast]);

  return {
    compat,
    setCompat,
    regexScripts,
    setRegexScripts,
    importAsset,
    exportCard,
  };
}
