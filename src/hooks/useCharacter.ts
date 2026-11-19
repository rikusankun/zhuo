import { useCallback, useState } from 'react';
import { api, isApiError, type Card } from '../api/client';
import { useToast } from './useToast';

export function useCharacter() {
  const { addToast } = useToast();
  const [cards, setCards] = useState<Card[]>([]);
  const [cardId, setCardId] = useState('');
  const [newRole, setNewRole] = useState({ name: '', description: '', first_mes: '', system_prompt: '' });

  const createCard = useCallback(async () => {
    try {
      const result = await api.cards.create(newRole);
      if (isApiError(result)) {
        addToast('error', result.error);
        return;
      }
      setCards(a => [result.card, ...a]);
      setCardId(result.card.id);
      setNewRole({ name: '', description: '', first_mes: '', system_prompt: '' });
      addToast('success', `已创建角色：${result.card.name}`);
    } catch (e) {
      addToast('error', `创建角色失败：${String(e)}`);
    }
  }, [newRole, addToast]);

  const deleteCard = useCallback(async (item: Card) => {
    if (!confirm(`删除角色“${item.name}”？\n\n这会永久删除角色卡文件，以及该角色的全部对话、消息、长期记忆、世界设定索引和角色状态。此操作不可撤销。`)) return;
    try {
      const result = await api.cards.delete(item.id);
      if (isApiError(result)) {
        addToast('error', result.error);
        return;
      }
      setCards(next => next.filter(x => x.id !== item.id));
      addToast('success', `已删除角色：${item.name}`);
    } catch (e) {
      addToast('error', `删除角色失败：${String(e)}`);
    }
  }, [addToast]);

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
      addToast('error', String(x.error));
      return;
    }
    setCards(a => (a.some(v => v.id === x.card.id) ? a : [...a, x.card]));
    setCardId(x.card.id);
    addToast('success', `已导入角色：${x.card.name}`);
  }, [addToast]);

  return {
    cards,
    setCards,
    cardId,
    setCardId,
    newRole,
    setNewRole,
    createCard,
    deleteCard,
    importCard,
  };
}
