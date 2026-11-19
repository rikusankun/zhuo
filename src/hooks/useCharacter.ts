import { useCallback, useState } from 'react';
import { api, isApiError, type Card } from '../api/client';

export function useCharacter() {
  const [cards, setCards] = useState<Card[]>([]);
  const [cardId, setCardId] = useState('');
  const [newRole, setNewRole] = useState({ name: '', description: '', first_mes: '', system_prompt: '' });
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');

  const createCard = useCallback(async () => {
    try {
      const result = await api.cards.create(newRole);
      if (isApiError(result)) {
        setError(result.error);
        return;
      }
      setCards(a => [result.card, ...a]);
      setCardId(result.card.id);
      setNewRole({ name: '', description: '', first_mes: '', system_prompt: '' });
      setNotice(`已创建角色：${result.card.name}`);
    } catch (e) {
      setError(`创建角色失败：${String(e)}`);
    }
  }, [newRole, setError, setNotice]);

  const deleteCard = useCallback(async (item: Card) => {
    if (!confirm(`删除角色“${item.name}”？\n\n这会永久删除角色卡文件，以及该角色的全部对话、消息、长期记忆、世界设定索引和角色状态。此操作不可撤销。`)) return;
    try {
      const result = await api.cards.delete(item.id);
      if (isApiError(result)) {
        setError(result.error);
        return;
      }
      setCards(next => next.filter(x => x.id !== item.id));
      setNotice(`已删除角色：${item.name}`);
    } catch (e) {
      setError(`删除角色失败：${String(e)}`);
    }
  }, [setError, setNotice]);

  const importCard = useCallback(async (file?: File) => {
    if (!file) return;
    const data = await new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result).split(',')[1] || '');
      reader.onerror = () => reject(reader.error);
      reader.readAsDataURL(file);
    });
    const x = await api.cards.import(file.name, data);
    if (isApiError(x)) {
      setError(String(x.error));
      return;
    }
    setCards(a => (a.some(v => v.id === x.card.id) ? a : [...a, x.card]));
    setCardId(x.card.id);
    setNotice(`已导入角色：${x.card.name}`);
  }, [setError, setNotice]);

  return {
    cards,
    setCards,
    cardId,
    setCardId,
    newRole,
    setNewRole,
    error,
    setError,
    notice,
    setNotice,
    createCard,
    deleteCard,
    importCard,
  };
}
