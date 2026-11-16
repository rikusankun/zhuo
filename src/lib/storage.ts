import type { Lore } from '../api/client';

function read(key: string): unknown {
  try { return JSON.parse(localStorage.getItem(key) || 'null'); }
  catch { return null; }
}

export function readPersona(): { name: string; description: string } {
  const value = read('zhuo.persona');
  if (value && typeof value === 'object' && 'name' in value && 'description' in value &&
      typeof value.name === 'string' && typeof value.description === 'string') return { name: value.name, description: value.description };
  return { name: '旅人', description: '' };
}

export function readLore(): Lore[] {
  const value = read('zhuo.lore');
  if (!Array.isArray(value)) return [];
  return value.filter((entry): entry is Lore => entry && typeof entry === 'object' &&
    typeof entry.id === 'string' && typeof entry.keys === 'string' &&
    typeof entry.content === 'string' && typeof entry.enabled === 'boolean');
}

export function saveLocal(key: string, value: unknown): void {
  try { localStorage.setItem(key, JSON.stringify(value)); }
  catch { /* Restricted storage must not crash the conversation UI. */ }
}
