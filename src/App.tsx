import { useCallback, useEffect, useRef, useState } from 'react';
import { BookOpen, Brain, Menu, X } from 'lucide-react';
import { api, isApiError, type Card } from './api/client';
import { useConversation } from './hooks/useConversation';
import { useConnectionConfig } from './hooks/useConnectionConfig';
import { useModelManager } from './hooks/useModelManager';
import { useAssetManager } from './hooks/useAssetManager';
import { useCharacter } from './hooks/useCharacter';
import { useContext } from './hooks/useContext';
import { useMemory } from './hooks/useMemory';
import { ToastProvider, useToast } from './hooks/useToast';
import { PersonaProvider, ConnectionProvider } from './hooks/useConnectionContext';
import { Sidebar } from './components/Sidebar';
import { ChatView, ChatComposer } from './components/ChatView';
import { SettingsPanel } from './components/panels/SettingsPanel';
import { ContextPanel } from './components/panels/ContextPanel';
import { MemoryPanel } from './components/panels/MemoryPanel';

const initials = (name: string) => name.trim().slice(0, 2).toUpperCase() || 'AI';

export default function App() {
  const { addToast, removeToast, toasts } = useToast();
  const [search, setSearch] = useState('');
  const [mobile, setMobile] = useState(false);
  const [panel, setPanel] = useState<'context' | 'settings' | 'memory' | null>(null);
  const [userMenuOpen, setUserMenuOpen] = useState(false);
  const [createDialog, setCreateDialog] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);

  const { data: { connection, saved }, actions: { setConnection, normalizeBaseUrl, saveConnection, changeNumGpu } } = useConnectionConfig();
  const { data: { connectionStatus, modelFilter, visibleModels }, actions: { setModelFilter, loadModels, testConnection, refreshModels }, status: { isLoading: modelsLoading } } = useModelManager(connection);

  const { data: { cards, cardId, newRole }, actions: { setCards, setCardId, setNewRole, createCard, deleteCard: characterDeleteCard, importCard } } = useCharacter();
  const { data: { persona, lore, loreHits: contextLoreHits }, actions: { setPersona, setLore } } = useContext();
  const { data: { memories, memoryTotal, stateText }, actions: { setStateText, loadMemory, saveState } } = useMemory(cardId);
  const { compat, importAsset, exportCard } = useAssetManager({ connection, cardId });

  const { chats, chatId, setChatId, messages, input, setInput, busy, loading, usage, promptChars, promptTokens, abort, newChat, deleteChat, renameChat, send, regenerate } = useConversation(cardId, { persona, lore_entries: lore }, []);

  const handleDeleteCard = useCallback(async (item: Card) => {
    await characterDeleteCard(item);
    if (cardId === item.id) {
      const next = cards.find(x => x.id !== item.id);
      setCardId(next?.id || '');
      setPanel(null);
    }
  }, [cardId, setCardId, cards, setPanel, characterDeleteCard]);

  const card = cards.find(x => x.id === cardId);

  useEffect(() => {
    Promise.all([api.cards.list(), api.connection.get(), api.compatibility.get()])
      .then(([list, cfg]) => {
        setCards(Array.isArray(list) ? list : []);
        if (Array.isArray(list) && list[0]) setCardId(list[0].id);
        if (!isApiError(cfg) && cfg.connection) setConnection({ ...cfg.connection });
      })
      .catch(() => addToast('error', '无法连接后端，请先运行 npm run start。'));
  }, []);

  useEffect(() => {
    if (panel === 'settings') void loadModels();
  }, [panel, loadModels]);

  useEffect(() => {
    if (!userMenuOpen) return;
    const closeMenu = (event: PointerEvent) => {
      if (!(event.target instanceof Element) || !event.target.closest('.user-menu-anchor')) setUserMenuOpen(false);
    };
    const handleKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') setUserMenuOpen(false);
    };
    document.addEventListener('pointerdown', closeMenu);
    document.addEventListener('keydown', handleKey);
    return () => {
      document.removeEventListener('pointerdown', closeMenu);
      document.removeEventListener('keydown', handleKey);
    };
  }, [userMenuOpen]);

  const panelButton = (kind: typeof panel, icon: React.ReactNode, label: string) => <button className={`icon-button ${panel === kind ? 'active' : ''}`} onClick={() => setPanel(panel === kind ? null : kind)} title={label}>{icon}</button>;

  const closeToast = (id: string) => removeToast(id);

  return (
    <ToastProvider>
      <PersonaProvider persona={persona} setPersona={setPersona}>
        <ConnectionProvider connection={connection} setConnection={setConnection}>
          <div className="app-shell">
        <Sidebar cards={cards} cardId={cardId} setCardId={setCardId} search={search} setSearch={setSearch} mobile={mobile} setMobile={setMobile} busy={busy} newChat={newChat} setCreateDialog={setCreateDialog} fileInput={fileInput} importCard={importCard} deleteCard={handleDeleteCard} chats={chats} chatId={chatId} setChatId={setChatId} deleteChat={deleteChat} renameChat={renameChat} setUserMenuOpen={setUserMenuOpen} userMenuOpen={userMenuOpen} setPanel={setPanel} />
        <main className="main-stage">
          <header className="topbar"><button className="icon-button mobile-menu" onClick={() => setMobile(true)}><Menu /></button><div className="active-character"><span className="avatar">{initials(card?.name || '')}</span><div><strong>{card?.name || '选择角色'}</strong><span><i /> {connection.model || '未配置模型'}</span></div></div><div className="top-actions">{panelButton('memory', <Brain size={18} />, '记忆')}{panelButton('context', <BookOpen size={18} />, '上下文')}</div></header>
          {toasts.length > 0 && <div className="toast-list">{toasts.map(t => <div key={t.id} className={`status ${t.type === 'error' ? 'error' : t.type === 'success' ? '' : 'info'}`}>{t.message}<button onClick={() => closeToast(t.id)}><X size={14} /></button></div>)}</div>}
          <ChatView messages={messages} card={card} busy={busy} loading={loading} cardId={cardId} newChat={newChat} regenerate={regenerate} />
          <ChatComposer input={input} setInput={setInput} busy={busy} loading={loading} cardId={cardId} chatId={chatId} send={send} abort={abort} promptChars={promptChars} promptTokens={promptTokens} usage={usage} loreHits={contextLoreHits} />
        </main>
        {panel && <aside className="right-panel"><div className="panel-head"><div><strong>{panel === 'settings' ? '连接与生成' : panel === 'memory' ? '记忆与状态' : '上下文工作台'}</strong><span>{card?.name}</span></div><button onClick={() => setPanel(null)}><X size={18} /></button></div>
          {panel === 'settings' && <SettingsPanel visibleModels={visibleModels} modelFilter={modelFilter} setModelFilter={setModelFilter} connectionStatus={connectionStatus} modelsLoading={modelsLoading} saved={saved} normalizeBaseUrl={normalizeBaseUrl} testConnection={testConnection} refreshModels={refreshModels} saveConnection={saveConnection} changeNumGpu={changeNumGpu} importAsset={importAsset} exportCard={(format: 'json' | 'png') => exportCard(format)} compat={compat} />}
          {panel === 'context' && <ContextPanel lore={lore} setLore={setLore} />}
          {panel === 'memory' && <MemoryPanel memories={memories} memoryTotal={memoryTotal} messagesLength={messages.length} loadMemory={loadMemory} stateText={stateText} setStateText={setStateText} saveState={() => saveState(stateText).then(r => { if (!r.ok) addToast('error', r.error || ''); else addToast('success', '角色状态已保存'); }).catch(e => addToast('error', String(e)))} />}
        </aside>}
        {createDialog && <div className="create-modal-backdrop"><form className="create-modal form-stack" onSubmit={e => { e.preventDefault(); void createCard(); }}><div className="create-modal-head"><div><strong>新建角色</strong><span>创建一个本地角色卡</span></div><button type="button" onClick={() => setCreateDialog(false)} aria-label="关闭"><X size={18} /></button></div><label>角色名称<input autoFocus required maxLength={100} value={newRole.name} onChange={e => setNewRole({ ...newRole, name: e.target.value })} placeholder="例如：林间旅人" /></label><label>角色背景<textarea rows={3} value={newRole.description} onChange={e => setNewRole({ ...newRole, description: e.target.value })} placeholder="简单介绍角色的身份与背景" /></label><label>开场白<textarea rows={3} value={newRole.first_mes} onChange={e => setNewRole({ ...newRole, first_mes: e.target.value })} placeholder="角色第一次对用户说的话" /></label><label>角色指令<textarea rows={4} value={newRole.system_prompt} onChange={e => setNewRole({ ...newRole, system_prompt: e.target.value })} placeholder="角色的性格、行为和回复要求" /></label><div className="create-modal-actions"><button type="button" className="secondary" onClick={() => setCreateDialog(false)}>取消</button><button type="submit" className="primary" disabled={!newRole.name.trim()}>创建角色</button></div></form></div>}
        {mobile && <button className="backdrop" onClick={() => setMobile(false)} aria-label="关闭导航" />}
      </div>
        </ConnectionProvider>
      </PersonaProvider>
    </ToastProvider>
  );
}
