import React from 'react';
import { Plus, Search, Trash2, Upload } from 'lucide-react';
import type { Card } from '../api/client';

const initials = (name: string) => name.trim().slice(0, 2).toUpperCase() || 'AI';

export type CharacterListProps = {
  cards: Card[];
  cardId: string;
  setCardId: (id: string) => void;
  search: string;
  setSearch: (v: string) => void;
  busy: boolean;
  setCreateDialog: React.Dispatch<React.SetStateAction<boolean>>;
  fileInput: React.RefObject<HTMLInputElement>;
  importCard: (file?: File) => Promise<void>;
  deleteCard: (item: Card) => Promise<void>;
};

export function CharacterList({
  cards,
  cardId,
  setCardId,
  search,
  setSearch,
  busy,
  setCreateDialog,
  fileInput,
  importCard,
  deleteCard,
}: CharacterListProps) {
  const filtered = cards.filter(x => `${x.name} ${x.description}`.toLowerCase().includes(search.toLowerCase()));
  return (
    <>
      <div className="search-box">
        <Search size={16} />
        <input value={search} onChange={e => setSearch(e.target.value)} placeholder="搜索角色" />
      </div>
      <input ref={fileInput} hidden type="file" accept=".json,.png,application/json,image/png" onChange={e => { importCard(e.target.files?.[0]); e.currentTarget.value = ''; }} />
      <div className="section-label">
        <span>角色</span>
        <div className="section-actions">
          <button title="新建角色" disabled={busy} onClick={(_e) => setCreateDialog(true)}><Plus size={15} /></button>
          <button title="导入角色卡" disabled={busy} onClick={() => fileInput.current?.click()}><Upload size={15} /></button>
        </div>
      </div>
      <div className="character-list">
        {filtered.map(x => (
          <div key={x.id} className="character-item">
            <button className={`character-row ${cardId === x.id ? 'selected' : ''}`} disabled={busy} onClick={() => { setCardId(x.id); }}>
              <span className="avatar">{initials(x.name)}</span>
              <span className="character-copy"><strong>{x.name}</strong><small>{x.description || '未填写角色描述'}</small></span>
            </button>
            <button className="character-delete" title={`删除角色 ${x.name}`} aria-label={`删除角色 ${x.name}`} disabled={busy && cardId === x.id} onClick={() => void deleteCard(x)}><Trash2 size={14} /></button>
          </div>
        ))}
        {!filtered.length && <div className="empty-small">{cards.length ? '没有匹配的角色' : '暂无角色，点击上方 + 导入角色卡'}</div>}
      </div>
    </>
  );
}
