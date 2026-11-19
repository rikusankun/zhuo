import { useCallback, useRef, useState, type Dispatch, type SetStateAction } from 'react';
import { api, isApiError, type Card } from '../api/client';
import { useToast } from './useToast';
import type { HookReturn } from './types';

export type UseCharacterData = {
  cards: Card[];
  cardId: string;
  newRole: { name: string; description: string; first_mes: string; system_prompt: string };
};

export type UseCharacterActions = {
  setCards: (cards: Card[]) => void;
  setCardId: (id: string) => void;
  setNewRole: Dispatch<SetStateAction<UseCharacterData['newRole']>>;
  updateCardField: (field: keyof UseCharacterData['newRole'], value: string) => void;
  resetNewRole: () => void;
  createCard: () => Promise<void>;
  deleteCard: (item: Card) => Promise<void>;
  importCard: (file?: File) => Promise<void>;
};

export function useCharacter(): HookReturn<UseCharacterData, UseCharacterActions> {
  const { addToast } = useToast();
  const [cards, setCards] = useState<Card[]>([]);
  const [cardId, setCardId] = useState<string>('');
  const [newRole, setNewRole] = useState<{ name: string; description: string; first_mes: string; system_prompt: string }>({ name: '', description: '', first_mes: '', system_prompt: '' });
  const charRequest = useRef(0);

  const createCard = useCallback(async () => {
    const version = ++charRequest.current;
    try {
      const result = await api.cards.create(newRole);
      if (version !== charRequest.current) return;
      if (isApiError(result)) {
        addToast('error', result.error);
        return;
      }
      setCards(a => [result.card, ...a]);
      setCardId(result.card.id);
      setNewRole({ name: '', description: '', first_mes: '', system_prompt: '' });
      addToast('success', `已创建角色：${result.card.name}`);
    } catch (e) {
      if (version === charRequest.current) {
        addToast('error', `创建角色失败：${String(e)}`);
      }
    }
  }, [newRole, addToast]);

  const deleteCard = useCallback(async (item: Card) => {
    if (!confirm(`删除角色“${item.name}”？\n\n这会永久删除角色卡文件，以及该角色的全部对话、消息、长期记忆、世界设定索引和角色状态。此操作不可撤销。`)) return;
    const version = ++charRequest.current;
    try {
      const result = await api.cards.delete(item.id);
      if (version !== charRequest.current) return;
      if (isApiError(result)) {
        addToast('error', result.error);
        return;
      }
      setCards(next => next.filter(x => x.id !== item.id));
      addToast('success', `已删除角色：${item.name}`);
    } catch (e) {
      if (version === charRequest.current) {
        addToast('error', `删除角色失败：${String(e)}`);
      }
    }
  }, [addToast]);

  const importCard = useCallback(async (file?: File) => {
    if (!file) return;
    const version = ++charRequest.current;
    const data = await new Promise<string>((resolve, reject) => {
      const reader = new FileReader();
      reader.onload = () => resolve(String(reader.result).split(',')[1] || '');
      reader.onerror = () => reject(reader.error);
      reader.readAsDataURL(file);
    });
    const x = await api.cards.import(file.name, data);
    if (version !== charRequest.current) return;
    if (isApiError(x)) {
      addToast('error', String(x.error));
      return;
    }
    setCards(a => (a.some(v => v.id === x.card.id) ? a : [...a, x.card]));
    setCardId(x.card.id);
    addToast('success', `已导入角色：${x.card.name}`);
  }, [addToast]);

  const updateCardField = useCallback((field: keyof UseCharacterData['newRole'], value: string) => {
    setNewRole(prev => ({ ...prev, [field]: value }));
  }, []);

  const resetNewRole = useCallback(() => {
    setNewRole({ name: '', description: '', first_mes: '', system_prompt: '' });
  }, []);

  return {
    data: { cards, cardId, newRole },
    actions: { setCards, setCardId, setNewRole, updateCardField, resetNewRole, createCard, deleteCard, importCard },
    status: { isLoading: false, error: null, success: null },
  };
}
