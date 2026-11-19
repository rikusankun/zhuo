import { MessageCircle, Plus, Trash2 } from 'lucide-react';
import type { Chat } from '../api/client';

export type ChatListProps = {
  chats: Chat[];
  chatId: string;
  setChatId: (id: string) => void;
  busy: boolean;
  cardId: string;
  newChat: () => void;
  deleteChat: (id: string) => void;
  renameChat: () => void;
};

export function ChatList({
  chats,
  chatId,
  setChatId,
  busy,
  cardId,
  newChat,
  deleteChat,
  renameChat,
}: ChatListProps) {
  return (
    <>
      <div className="section-label">
        <span>对话</span>
        <button disabled={busy || !cardId} onClick={() => newChat()}><Plus size={15} /></button>
      </div>
      <div className="chat-list">
        {chats.map(x => (
          <div key={x.id} className={`chat-row ${chatId === x.id ? 'selected' : ''}`}>
            <button disabled={busy} onDoubleClick={() => renameChat()} onClick={() => { setChatId(x.id); }}>
              <MessageCircle size={15} /><span>{x.title}</span>
            </button>
            <button className="row-delete" disabled={busy} onClick={() => deleteChat(x.id)}><Trash2 size={14} /></button>
          </div>
        ))}
        {!chats.length && <button className="empty-cta" disabled={busy || !cardId} onClick={() => newChat()}><Plus size={16} />开始第一段对话</button>}
      </div>
    </>
  );
}
