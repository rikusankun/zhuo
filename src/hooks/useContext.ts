import { useCallback, useEffect, useState } from 'react';
import type { Lore } from '../api/client';
import { readPersona, readLore, saveLocal } from '../lib/storage';

export function useContext() {
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
    persona,
    setPersona,
    lore,
    setLore,
    loreHits,
  };
}
