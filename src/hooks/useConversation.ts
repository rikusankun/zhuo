import { useCallback, useEffect, useRef, useState } from 'react';
import { api, isApiError, type Chat, type ChatInput, type Message, type RegexScript } from '../api/client';
import { consumeChatStream } from '../api/sse';
import { applyRegex } from '../lib/regex';
import { useToast } from './useToast';
import type { HookReturn } from './types';

export type UseConversationData = {
  chats: Chat[];
  chatId: string;
  messages: Message[];
  input: string;
  busy: boolean;
  loading: boolean;
  usage: number | null;
  promptChars: number | null;
  promptTokens: number | null;
  abort: React.MutableRefObject<AbortController | null>;
};

export type UseConversationActions = {
  setInput: (v: string) => void;
  newChat: () => Promise<void>;
  deleteChat: (id: string) => Promise<void>;
  renameChat: () => Promise<void>;
  selectChat: (id: string) => void;
  send: (override?: string, regenerate?: boolean) => Promise<void>;
  regenerate: () => void;
};

export function useConversation(
  cardId: string,
  context: Pick<ChatInput, 'persona' | 'lore_entries'>,
  regexScripts: RegexScript[],
): HookReturn<UseConversationData, UseConversationActions> {
  const { addToast } = useToast();
  
  // 状态定义
  const [chats, setChats] = useState<Chat[]>([]);
  const [chatId, setChatId] = useState<string>('');
  const [messages, setMessages] = useState<Message[]>([]);
  const [input, setInput] = useState<string>('');
  const [busy, setBusy] = useState<boolean>(false);
  const [loading, setLoading] = useState<boolean>(false);
  const [usage, setUsage] = useState<number | null>(null);
  const [promptChars, setPromptChars] = useState<number | null>(null);
  const [promptTokens, setPromptTokens] = useState<number | null>(null);
  
  // Refs 用于解决闭包陷阱和引用不稳定问题
  const abort = useRef<AbortController | null>(null);
  const locked = useRef(false);
  const selection = useRef({ cardId, chatId });
  const historyVersion = useRef(0);
  
  // ✅ 核心修复：将易变的 regexScripts 存入 Ref，避免触发 Effect 死循环
  const regexRef = useRef(regexScripts);
  regexRef.current = regexScripts;

  // ✅ context ref 兜底：context 对象每次渲染新建，存入 ref 避免 send 重建
  const contextRef = useRef(context);
  contextRef.current = context;
  
  // 同步当前选中的卡片和对话 ID
  selection.current = { cardId, chatId };

  // ✅ 核心修复：稳定化的 display 函数，不依赖外部变量
  const display = useCallback((rows: Message[]) => 
    rows.map(m => m.role === 'assistant'
      ? { ...m, content: applyRegex(m.content, regexRef.current) } 
      : m), 
  []);

  // 初始化：加载角色卡下的对话列表
  useEffect(() => {
    let active = true;
    abort.current?.abort();
    setChats([]); 
    setChatId(''); 
    setMessages([]); 
    setInput('');
    setUsage(null); 
    setPromptChars(null); 
    setPromptTokens(null);
    setLoading(Boolean(cardId));
    
    if (cardId) {
      void api.chats.list(cardId).then(result => {
        if (!active) return;
        if (isApiError(result)) { addToast('error', result.error); return; }
        setChats(result); 
        setChatId(result[0]?.id || '');
      }).catch(e => { addToast('error', String(e)); })
        .finally(() => { if (active) setLoading(false); });
    } else {
      setLoading(false);
    }
    
    return () => { active = false; };
  }, [cardId, addToast]);

  // 加载历史消息：✅ 修复了因 regexScripts 引用变化导致的无限循环
  useEffect(() => {
    const version = ++historyVersion.current;
    if (locked.current) return;
    
    setMessages([]); 
    setUsage(null); 
    setPromptChars(null); 
    setPromptTokens(null);
    
    if (!chatId) {
      setLoading(false);
      return;
    }
    
    setLoading(true);
    void api.history.get(chatId).then(result => {
      if (version !== historyVersion.current) return;
      if (isApiError(result)) { addToast('error', result.error); return; }
      setMessages(display(result));
    }).catch(e => { 
      if (version === historyVersion.current) addToast('error', String(e)); 
    }).finally(() => { 
      if (version === historyVersion.current) setLoading(false); 
    });
    
    return () => { historyVersion.current++; };
  }, [chatId, addToast, display]);

  // 组件卸载时中止所有请求
  useEffect(() => () => { abort.current?.abort(); }, []);

  // 新建对话
  const newChat = useCallback(async () => {
    if (!cardId || locked.current || loading) return;
    const current = cardId;
    locked.current = true;
    setBusy(true);
    
    try {
      const result = await api.chats.create(current);
      if (isApiError(result)) { 
        addToast('error', result.error); 
        return; 
      }
      if (selection.current.cardId !== current) return;
      
      setChats(rows => [result, ...rows]); 
      setChatId(result.id);
      addToast('success', '已创建新对话');
    } catch (e) { 
      addToast('error', String(e)); 
    } finally { 
      locked.current = false; 
      setBusy(false); 
    }
  }, [cardId, loading, addToast]);

  // 删除对话
  const deleteChat = useCallback(async (id: string) => {
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
    } catch (e) { 
      addToast('error', String(e)); 
    }
  }, [cardId, chats, addToast]);

  // 重命名对话
  const renameChat = useCallback(async () => {
    const chat = chats.find(row => row.id === chatId);
    if (!chat) return;
    
    const title = prompt('对话名称', chat.title)?.trim();
    if (!title) return;
    
    try {
      const result = await api.chats.rename(chat.id, title);
      if (isApiError(result)) { addToast('error', result.error); return; }
      if (!result.ok) { addToast('error', '对话不存在，请刷新'); return; }
      
      setChats(rows => rows.map(row => row.id === chat.id ? { ...row, title } : row));
    } catch (e) { 
      addToast('error', String(e)); 
    }
  }, [chats, chatId, addToast]);

  // ✅ 语义化的会话切换：生成中禁止切换，防止脏数据
  const selectChat = useCallback((id: string) => {
    if (locked.current) return;
    setChatId(id);
  }, []);

  // 发送消息 / 流式生成
  const send = useCallback(async (override?: string, regenerate = false, model?: string) => {
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
    
    const isCurrent = () => 
      selection.current.cardId === currentCard && 
      selection.current.chatId === active;
    
    try {
      // 如果没有当前对话，自动创建一个
      if (!active) {
        const result = await api.chats.create(currentCard);
        if (isApiError(result)) { addToast('error', result.error); return; }
        if (selection.current.cardId !== currentCard || controller.signal.aborted) return;
        
        active = result.id;
        selection.current.chatId = active;
        setChats(rows => [result, ...rows]); 
        setChatId(active);
        
        const history = await api.history.get(active);
        if (isApiError(history)) { addToast('error', history.error); return; }
        previous = display(history);
      }
      
      if (!isCurrent() || controller.signal.aborted) return;
      if (!regenerate) setInput('');
      
      setUsage(null); 
      setPromptChars(null); 
      setPromptTokens(null);
      
      // 乐观更新 UI
      setMessages(regenerate 
        ? [...previous.slice(0, -1), { role: 'assistant', content: '' }]
        : [...previous, { role: 'user', content: text }, { role: 'assistant', content: '' }]
      );
      
      // 发起 SSE 流式请求
      const stream = await api.chat.stream({ 
        card: currentCard, 
        chat_id: active,
        message: text, 
        display_message: text, 
        regenerate, 
        model,
        ...contextRef.current 
      }, controller.signal);
      
      let streamError = '';
      await consumeChatStream(stream, event => {
        if (!isCurrent()) return;
        
        if (typeof event.prompt_chars === 'number') setPromptChars(event.prompt_chars);
        if (typeof event.estimated_tokens === 'number') setPromptTokens(event.estimated_tokens);
        
        if (typeof event.token === 'string') {
          received = true;
          setMessages(rows => rows.map((m, i) => 
            i === rows.length - 1 ? { ...m, content: m.content + event.token } : m
          ));
        }
        
        // 应用正则替换
        if (typeof event.raw_content === 'string' || typeof event.processed === 'string') {
          setMessages(rows => rows.map((m, i) => i === rows.length - 1
            ? { ...m, content: applyRegex(event.processed ?? event.raw_content ?? '', regexRef.current) } 
            : m
          ));
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
        
        // 如果完全没收到内容，回滚 UI
        if (!received) { 
          setMessages(previous); 
          if (!regenerate) setInput(text); 
        }
      }
    } finally {
      locked.current = false; 
      setBusy(false);
      if (abort.current === controller) abort.current = null;
    }
  }, [cardId, chatId, input, loading, messages, addToast, display]);

  // 重新生成最后一条回复
  const regenerate = useCallback(() => {
    const user = messages.at(-2);
    if (user?.role === 'user' && messages.at(-1)?.role === 'assistant') {
      void send(user.content, true);
    }
  }, [messages, send]);

  return {
    data: { 
      chats, chatId, messages, input, busy, loading, 
      usage, promptChars, promptTokens, abort 
    },
    actions: { 
      setInput, newChat, deleteChat, renameChat, selectChat, send, regenerate 
    },
    status: {
      isLoading: busy || loading,
      error: null,
      success: null,
    },
  };
}