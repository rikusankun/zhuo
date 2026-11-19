import { createContext, useContext, useState, useEffect, type ReactNode } from 'react';
import { readPersona, saveLocal } from '../lib/storage';
import type { Connection } from '../api/client';

// 1. 空白默认值：首启不预填任何厂商、端点或模型，由用户在设置面板手动连接
const DEFAULT_PERSONA = { name: '用户', description: '' };
const DEFAULT_CONNECTION: Connection = { 
  provider: 'openai_compatible',
  base_url: '', 
  api_key: '', 
  model: '', 
  summary_model: '', 
  num_gpu: 0,
  temperature: 0.8, 
  max_tokens: 1024, 
  top_p: 1.0, 
  frequency_penalty: 0.0, 
  presence_penalty: 0.0, 
  repetition_penalty: 1.0
};

// 2. 创建 Context（注意：这里必须定义，否则 Provider 无法工作）
const PersonaContext = createContext<{ 
  persona: { name: string; description: string }; 
  setPersona: (p: { name: string; description: string }) => void; 
} | null>(null);

const ConnectionContext = createContext<{ 
  connection: Connection; 
  setConnection: (c: Connection) => void; 
} | null>(null);

// 3. 自包含型 PersonaProvider
export function PersonaProvider({ children }: { children: ReactNode }) {
  // 从 localStorage 读取初始值，如果没有则使用默认值
  const [persona, setPersona] = useState(() => {
    try {
      return readPersona() || DEFAULT_PERSONA;
    } catch {
      return DEFAULT_PERSONA;
    }
  });
  
  // 自动持久化到 localStorage
  useEffect(() => { 
    saveLocal('zhuo.persona', persona); 
  }, [persona]);

  return (
    <PersonaContext.Provider value={{ persona, setPersona }}>
      {children}
    </PersonaContext.Provider>
  );
}

// 4. 自包含型 ConnectionProvider
export function ConnectionProvider({ children }: { children: ReactNode }) {
  const [connection, setConnection] = useState<Connection>(DEFAULT_CONNECTION);
  
  // 可选：在这里添加从 API 或 localStorage 加载连接配置的逻辑
  // useEffect(() => { ... }, []);
  
  return (
    <ConnectionContext.Provider value={{ connection, setConnection }}>
      {children}
    </ConnectionContext.Provider>
  );
}

// 5. 配套 Consumer Hooks（这就是 UserMenu.tsx 报错缺失的部分）
export function usePersona() {
  const ctx = useContext(PersonaContext);
  if (!ctx) throw new Error('usePersona must be used within a PersonaProvider');
  return ctx;
}

export function useConnection() {
  const ctx = useContext(ConnectionContext);
  if (!ctx) throw new Error('useConnection must be used within a ConnectionProvider');
  return ctx;
}