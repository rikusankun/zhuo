import { useCallback, useEffect, useState } from 'react';
import type { Lore } from '../api/client';
import { readPersona, readLore, saveLocal } from '../lib/storage';
import type { HookReturn } from './types';

export type UseContextData = {
  persona: { name: string; description: string };
  lore: Lore[];
  loreHits: (text: string) => Lore[];
};

export type UseContextActions = {
  setPersona: (next: { name: string; description: string }) => void;
  setLore: (next: Lore[] | ((prev: Lore[]) => Lore[])) => void;
};

export function useContext(): HookReturn<UseContextData, UseContextActions> {
  const [persona, setPersona] = useState<{ name: string; description: string }>(readPersona);
  const [lore, setLore] = useState<Lore[]>(readLore);

  useEffect(() => {
    saveLocal('zhuo.persona', persona);
  }, [persona]);

  useEffect(() => {
    saveLocal('zhuo.lore', lore);
  }, [lore]);

  const loreHits = useCallback((text: string): Lore[] => {
    const hay = text.toLowerCase();
    return lore.filter(x => x.enabled && x.keys.split(',').some(k => k.trim() && hay.includes(k.trim().toLowerCase())));
  }, [lore]);

  return {
    data: { persona, lore, loreHits },
    actions: { setPersona, setLore },
    status: { isLoading: false, error: null, success: null },
  };
}
