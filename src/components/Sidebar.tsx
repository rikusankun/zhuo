import React from 'react';
import { Brain, MoreHorizontal, Plus, Search, Settings, Trash2, Upload, UserRound, X, Sparkles, MessageCircle } from 'lucide-react';
import type { Card, Chat } from '../api/client';
import { usePersona } from '../hooks/useConnectionContext';

export type SidebarProps = {
  cards: Card[];
  cardId: string;
  setCardId: (id: string) => void;
  search: string;
  setSearch: (v: string) => void;
  mobile: boolean;
  setMobile: (m: boolean) => void;
  busy: boolean;
  newChat: () => void;
  setCreateDialog: React.Dispatch<React.SetStateAction<boolean>>;
  fileInput: React.RefObject<HTMLInputElement>;
  importCard: (file?: File) => Promise<void>;
  deleteCard: (item: Card) => Promise<void>;
  chats: Chat[];
  chatId: string;
  setChatId: (id: string) => void;
  deleteChat: (id: string) => void;
  renameChat: () => void;
  setUserMenuOpen: React.Dispatch<React.SetStateAction<boolean>>;
  userMenuOpen: boolean;
  setPanel: (panel: 'context' | 'settings' | 'memory' | null) => void;
};

const initials = (name: string) => name.trim().slice(0, 2).toUpperCase() || 'AI';

export function Sidebar({
  cards,
  cardId,
  setCardId,
  search,
  setSearch,
  mobile,
  setMobile,
  busy,
  newChat,
  setCreateDialog,
  fileInput,
  importCard,
  deleteCard,
  chats,
  chatId,
  setChatId,
  deleteChat,
  renameChat,
  setUserMenuOpen,
  userMenuOpen,
  setPanel,
}: SidebarProps) {
  const { persona } = usePersona();
  const filtered = cards.filter(x => `${x.name} ${x.description}`.toLowerCase().includes(search.toLowerCase()));

  return (
    <aside className={`left-sidebar ${mobile ? 'mobile-open' : ''}`}>
      <div className="brand">
        <div className="brand-mark"><Sparkles size={18} /></div>
        <div>
          <strong>Zhuo</strong><span>角色叙事工作台</span>
        </div>
        <button className="mobile-close" onClick={() => setMobile(false)}><X /></button>
      </div>
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
            <button className={`character-row ${cardId === x.id ? 'selected' : ''}`} disabled={busy} onClick={() => { setCardId(x.id); setMobile(false); }}>
              <span className="avatar">{initials(x.name)}</span>
              <span className="character-copy"><strong>{x.name}</strong><small>{x.description || '未填写角色描述'}</small></span>
            </button>
            <button className="character-delete" title={`删除角色 ${x.name}`} aria-label={`删除角色 ${x.name}`} disabled={busy && cardId === x.id} onClick={() => void deleteCard(x)}><Trash2 size={14} /></button>
          </div>
        ))}
        {!filtered.length && <div className="empty-small">{cards.length ? '没有匹配的角色' : '暂无角色，点击上方 + 导入角色卡'}</div>}
      </div>
      <div className="section-label">
        <span>对话</span>
        <button disabled={busy || !cardId} onClick={() => newChat()}><Plus size={15} /></button>
      </div>
      <div className="chat-list">
        {chats.map(x => (
          <div key={x.id} className={`chat-row ${chatId === x.id ? 'selected' : ''}`}>
            <button disabled={busy} onDoubleClick={() => renameChat()} onClick={() => { setChatId(x.id); setMobile(false); }}>
              <MessageCircle size={15} /><span>{x.title}</span>
            </button>
            <button className="row-delete" disabled={busy} onClick={() => deleteChat(x.id)}><Trash2 size={14} /></button>
          </div>
        ))}
        {!chats.length && <button className="empty-cta" disabled={busy || !cardId} onClick={() => newChat()}><Plus size={16} />开始第一段对话</button>}
      </div>
      <div className="user-menu-anchor">
        <button className="persona-card" aria-expanded={userMenuOpen} aria-haspopup="menu" onClick={() => setUserMenuOpen(open => !open)}>
          <span className="avatar user">{initials(persona.name)}</span>
          <span>
            <strong>{persona.name}</strong><small>个人设置与偏好</small>
          </span>
          <MoreHorizontal size={18} />
        </button>
        {userMenuOpen && (
          <div className="user-menu-popover" role="menu">
            <button role="menuitem" onClick={() => { setPanel('settings'); setUserMenuOpen(false); }}><Settings size={16} />设置</button>
            <button role="menuitem" onClick={() => { setPanel('context'); setUserMenuOpen(false); }}><UserRound size={16} />个性与上下文</button>
            <button role="menuitem" onClick={() => { setPanel('memory'); setUserMenuOpen(false); }}><Brain size={16} />记忆与状态</button>
          </div>
        )}
      </div>
    </aside>
  );
}
