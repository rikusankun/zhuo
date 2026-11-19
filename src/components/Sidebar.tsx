import React from 'react';
import { X, Sparkles } from 'lucide-react';
import type { Card, Chat } from '../api/client';
import { CharacterList } from './CharacterList';
import { ChatList } from './ChatList';
import { UserMenu } from './UserMenu';

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
  selectChat: (id: string) => void;
  deleteChat: (id: string) => void;
  renameChat: () => void;
  setUserMenuOpen: React.Dispatch<React.SetStateAction<boolean>>;
  userMenuOpen: boolean;
  setPanel: (panel: 'context' | 'settings' | 'memory' | null) => void;
};

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
  selectChat,
  deleteChat,
  renameChat,
  setUserMenuOpen,
  userMenuOpen,
  setPanel,
}: SidebarProps) {
  return (
    <aside className={`left-sidebar ${mobile ? 'mobile-open' : ''}`}>
      <div className="brand">
        <div className="brand-mark"><Sparkles size={18} /></div>
        <div>
          <strong>Zhuo</strong><span>角色叙事工作台</span>
        </div>
        <button className="mobile-close" onClick={() => setMobile(false)}><X /></button>
      </div>
      <CharacterList
        cards={cards}
        cardId={cardId}
        setCardId={setCardId}
        search={search}
        setSearch={setSearch}
        busy={busy}
        setCreateDialog={setCreateDialog}
        fileInput={fileInput}
        importCard={importCard}
        deleteCard={deleteCard}
      />
      <ChatList
        chats={chats}
        chatId={chatId}
        selectChat={selectChat}
        busy={busy}
        cardId={cardId}
        newChat={newChat}
        deleteChat={deleteChat}
        renameChat={renameChat}
      />
      <UserMenu
        userMenuOpen={userMenuOpen}
        setUserMenuOpen={setUserMenuOpen}
        setPanel={setPanel}
      />
    </aside>
  );
}
