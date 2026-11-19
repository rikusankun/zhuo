import { useEffect, useRef, useState } from 'react';
import { api, isApiError, type Chat, type ChatInput, type Message, type RegexScript } from '../api/client';
import { consumeChatStream } from '../api/sse';
import { applyRegex } from '../lib/regex';
import { useToast } from './useToast';

export function useConversation(
  cardId: string,
  context: Pick<ChatInput, 'persona' | 'lore_entries'>,
  regexScripts: RegexScript[],
) {
  const { addToast } = useToast();
  const [chats, setChats] = useState<Chat[]>([]);
  const [chatId, setChatId] = useState('');
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState('');
  const [busy, setBusy] = useState(false);
  const [loading, setLoading] = useState(false);
  const [usage, setUsage] = useState<number | null>(null);
  const [promptChars, setPromptChars] = useState<number | null>(null);
  const [promptTokens, setPromptTokens] = useState<number | null>(null);
  const abort = useRef<AbortController | null>(null);
  const locked = useRef(false);
  const selection = useRef({ cardId, chatId });
  const historyVersion = useRef(0);
  selection.current = { cardId, chatId };
  const display = (rows: Message[]) => rows.map(m => m.role === 'assistant'
    ? { ...m, content: applyRegex(m.content, regexScripts) } : m);

  useEffect(() => {
    let active = true;
    abort.current?.abort();
    setChats([]); setChatId(''); setMessages([]); setInput('');
    setUsage(null); setPromptChars(null); setPromptTokens(null);
    setLoading(Boolean(cardId));
    if (cardId) void api.chats.list(cardId).then(result => {
      if (!active) return;
      if (isApiError(result)) { addToast('error', result.error); return; }
      setChats(result); setChatId(result[0]?.id || '');
    }).catch(e => { addToast('error', String(e)); })
      .finally(() => { if (active) setLoading(false); });
    return () => { active = false; };
  }, [cardId, addToast]);

  useEffect(() => {
    const version = ++historyVersion.current;
    if (locked.current) return;
    setMessages([]); setUsage(null); setPromptChars(null); setPromptTokens(null);
    if (!chatId) return;
    setLoading(true);
    void api.history.get(chatId).then(result => {
      if (version !== historyVersion.current) return;
      if (isApiError(result)) { addToast('error', result.error); return; }
      setMessages(display(result));
    }).catch(e => { if (version === historyVersion.current) addToast('error', String(e)); })
      .finally(() => { if (version === historyVersion.current) setLoading(false); });
    return () => { historyVersion.current++; };
  }, [chatId, regexScripts, addToast]);

  useEffect(() => () => { abort.current?.abort(); }, []);

  async function newChat() {
    if (!cardId || locked.current || loading) return;
    const current = cardId;
    locked.current = true;
    setBusy(true);
    try {
      const result = await api.chats.create(current);
      if (isApiError(result)) { addToast('error', result.error); locked.current = false; setBusy(false); return; }
      if (selection.current.cardId !== current) return;
      locked.current = false;
      setChats(rows => [result, ...rows]); setChatId(result.id);
      addToast('success', '已创建新对话');
    } catch (e) { addToast('error', String(e)); }
    finally { locked.current = false; setBusy(false); }
  }

  async function deleteChat(id: string) {
    if (locked.current || !confirm('删除这个对话及全部消息？此操作不可撤销。')) return;
    const current = cardId;
    try {
      const result = await api.chats.delete(id);
      if (isApiError(result)) { addToast('error', result.error); return; }
      if (!result.ok) { addToast('error', '对话不存在，请刷新'); return; }
      if (selection.current.cardId !== current) return;
      const next = chats.filter(row => row.id !== id);
      setChats(next);
      if (selection.current.chatId === id) setChatId(next[0]?.id || '');
    } catch (e) { addToast('error', String(e)); }
  }

  async function renameChat() {
    const chat = chats.find(row => row.id === chatId);
    if (!chat) return;
    const title = prompt('对话名称', chat.title)?.trim();
    if (!title) return;
    try {
      const result = await api.chats.rename(chat.id, title);
      if (isApiError(result)) { addToast('error', result.error); return; }
      if (!result.ok) { addToast('error', '对话不存在，请刷新'); return; }
      setChats(rows => rows.map(row => row.id === chat.id ? { ...row, title } : row));
    } catch (e) { addToast('error', String(e)); }
  }

  async function send(override?: string, regenerate = false) {
    const text = (override ?? input).trim();
    if (!text || !cardId || locked.current || loading) return;
    locked.current = true;
    historyVersion.current++;
    setBusy(true);
    const controller = new AbortController();
    abort.current = controller;
    const currentCard = cardId;
    let active = chatId;
    let previous = messages;
    let received = false;
    const isCurrent = () => selection.current.cardId === currentCard && selection.current.chatId === active;
    try {
      if (!active) {
        const result = await api.chats.create(currentCard);
        if (isApiError(result)) { addToast('error', result.error); return; }
        if (selection.current.cardId !== currentCard || controller.signal.aborted) return;
        active = result.id;
        selection.current.chatId = active;
        setChats(rows => [result, ...rows]); setChatId(active);
        const history = await api.history.get(active);
        if (isApiError(history)) { addToast('error', history.error); return; }
        previous = display(history);
      }
      if (!isCurrent() || controller.signal.aborted) return;
      if (!regenerate) setInput('');
      setUsage(null); setPromptChars(null); setPromptTokens(null);
      setMessages(regenerate ? [...previous.slice(0, -1), { role: 'assistant', content: '' }]
        : [...previous, { role: 'user', content: text }, { role: 'assistant', content: '' }]);
      const stream = await api.chat.stream({ card: currentCard, chat_id: active,
        message: text, display_message: text, regenerate, ...context }, controller.signal);
      let streamError = '';
      await consumeChatStream(stream, event => {
        if (!isCurrent()) return;
        if (typeof event.prompt_chars === 'number') setPromptChars(event.prompt_chars);
        if (typeof event.estimated_tokens === 'number') setPromptTokens(event.estimated_tokens);
        if (typeof event.token === 'string') {
          received = true;
          setMessages(rows => rows.map((m, i) => i === rows.length - 1 ? { ...m, content: m.content + event.token } : m));
        }
        if (typeof event.raw_content === 'string' || typeof event.processed === 'string') {
          setMessages(rows => rows.map((m, i) => i === rows.length - 1
            ? { ...m, content: applyRegex(event.processed ?? event.raw_content ?? '', regexScripts) } : m));
        }
        if (typeof event.usage?.total_tokens === 'number') setUsage(event.usage.total_tokens);
        if (event.error) streamError = event.error;
      });
      if (streamError) throw new Error(streamError);
      if (isCurrent()) addToast('success', '回复已保存');
    } catch (e) {
      if (isCurrent()) {
        if ((e as Error).name === 'AbortError') addToast('info', '已停止生成');
        else addToast('error', String(e));
        if (!received) { setMessages(previous); if (!regenerate) setInput(text); }
      }
    } finally {
      locked.current = false; setBusy(false);
      if (abort.current === controller) abort.current = null;
    }
  }

  function regenerate() {
    const user = messages.at(-2);
    if (user?.role === 'user' && messages.at(-1)?.role === 'assistant') void send(user.content, true);
  }

  return { chats, setChats, chatId, setChatId, messages, setMessages, input, setInput,
    busy, loading, usage, promptChars, promptTokens, abort, newChat, deleteChat, renameChat, send, regenerate };
}
