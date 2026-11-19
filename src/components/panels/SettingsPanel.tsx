import React from 'react';
import { BookOpen, Check, Database, RefreshCw, Settings, SlidersHorizontal, Sparkles } from 'lucide-react';
import type { Connection, Model } from '../../api/client';

export type SettingsPanelProps = {
  connection: Connection;
  setConnection: (c: Connection) => void;
  visibleModels: Model[];
  modelFilter: string;
  setModelFilter: (v: string) => void;
  connectionStatus: 'idle' | 'testing' | 'ok' | 'error';
  modelsLoading: boolean;
  saved: boolean;
  normalizeBaseUrl: (url: string) => string;
  testConnection: () => Promise<void>;
  refreshModels: () => Promise<void>;
  saveConnection: (next?: Connection, announce?: boolean) => Promise<boolean>;
  changeNumGpu: (v: number) => void;
  importAsset: (kind: 'worldbook' | 'preset' | 'regex', file?: File) => Promise<void>;
  exportCard: (format: 'json' | 'png') => Promise<void>;
  compat: { worldbook: number; regex: number };
};

function Field({ label, value, set, password = false, onBlur }: { label: string; value: string; set: (v: string) => void; password?: boolean; onBlur?: (v: string) => void }) {
  const [visible, setVisible] = React.useState(false);
  return (
    <label style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
      {label}
      <input
        type={password && !visible ? 'password' : 'text'}
        value={value}
        onChange={e => set(e.target.value)}
        onBlur={e => onBlur && onBlur(e.target.value)}
      />
      <button
        type="button"
        onClick={() => setVisible(!visible)}
        className="toggle"
        aria-label={visible ? '隐藏' : '显示'}
      >
        {visible ? '🙈' : '👁️'}
      </button>
    </label>
  );
}

export function SettingsPanel({
  connection,
  setConnection,
  visibleModels,
  modelFilter,
  setModelFilter,
  connectionStatus,
  modelsLoading,
  saved,
  normalizeBaseUrl,
  testConnection,
  refreshModels,
  saveConnection,
  changeNumGpu,
  importAsset,
  exportCard,
  compat,
}: SettingsPanelProps) {
  const worldInput = React.useRef<HTMLInputElement>(null);
  const presetInput = React.useRef<HTMLInputElement>(null);
  const regexInput = React.useRef<HTMLInputElement>(null);
  return (
    <div className="panel-scroll form-stack">
      <div style={{ fontWeight: 'bold', fontSize: '14px', marginBottom: '8px' }}>连接与生成</div>
      <label>
        服务类型
        <select value={connection.provider} onChange={e => setConnection({ ...connection, provider: e.target.value })}>
          <option value="ollama">Ollama</option>
          <option value="openai">OpenAI Compatible</option>
        </select>
      </label>
      <Field label="Base URL" value={connection.base_url} set={v => setConnection({ ...connection, base_url: v })} onBlur={v => setConnection({ ...connection, base_url: normalizeBaseUrl(v) })} />
      <Field label="API Key" value={connection.api_key} set={v => setConnection({ ...connection, api_key: v })} password />
      <div className="connection-bar">
        <span className={`dot ${connectionStatus}`}>{connectionStatus === 'idle' ? '未测试' : connectionStatus === 'testing' ? '测试中…' : connectionStatus === 'ok' ? '连接正常' : '连接失败'}</span>
        <button type="button" className="ghost" onClick={testConnection} disabled={connectionStatus === 'testing'}><RefreshCw size={14} /></button>
      </div>
      <button className="primary wide" onClick={refreshModels} disabled={modelsLoading}><RefreshCw size={16} />{modelsLoading ? '正在读取模型…' : '刷新本地模型列表'}</button>
      <input value={modelFilter} onChange={e => setModelFilter(e.target.value)} placeholder="搜索模型…" className="model-filter" />
      {connection.provider === 'openai' ? (
        <>
          <Field label="主聊天模型" value={connection.model} set={v => setConnection({ ...connection, model: v })} />
          <Field label="后台记忆提取模型（留空关闭）" value={connection.summary_model} set={v => setConnection({ ...connection, summary_model: v })} />
        </>
      ) : (
        <>
          <label>
            主聊天模型
            <select value={connection.model} onChange={e => void saveConnection({ ...connection, model: e.target.value }, false)}>
              {!visibleModels.some(m => m.id === connection.model) && <option value={connection.model}>{connection.model}</option>}
              {visibleModels.map(model => <option key={model.id} value={model.id}>{model.displayName}</option>)}
            </select>
          </label>
          <label>
            后台记忆提取模型
            <select value={connection.summary_model} onChange={e => void saveConnection({ ...connection, summary_model: e.target.value }, false)}>
              <option value="">关闭自动记忆提取</option>
              {!visibleModels.some(m => m.id === connection.summary_model) && connection.summary_model && <option value={connection.summary_model}>{connection.summary_model}</option>}
              {visibleModels.map(model => <option key={model.id} value={model.id}>{model.displayName}</option>)}
            </select>
          </label>
        </>
      )}
      <label>
        显存分配层数（num_gpu）
        <b>{connection.num_gpu}</b>
        <input type="range" min="0" max="99" step="1" value={connection.num_gpu} onChange={e => changeNumGpu(Number(e.target.value))} />
      </label>
      <small className="settings-help">6GB 显卡建议 15 层；12GB 以上可尝试 99 层。后台提取模型固定使用 CPU。</small>
      <div style={{ fontWeight: 'bold', fontSize: '14px', marginTop: '12px', marginBottom: '8px' }}>生成参数</div>
      <label>
        Temperature
        <b>{connection.temperature.toFixed(1)}</b>
        <input type="range" min="0" max="2" step="0.1" value={connection.temperature} onChange={e => setConnection({ ...connection, temperature: Number(e.target.value) })} />
      </label>
      <label>
        Top P
        <input type="number" min="0" max="1" step="0.05" value={connection.top_p} onChange={e => setConnection({ ...connection, top_p: Number(e.target.value) })} />
      </label>
      <label>
        Frequency Penalty
        <input type="number" min="-2" max="2" step="0.1" value={connection.frequency_penalty} onChange={e => setConnection({ ...connection, frequency_penalty: Number(e.target.value) })} />
      </label>
      <label>
        Presence Penalty
        <input type="number" min="-2" max="2" step="0.1" value={connection.presence_penalty} onChange={e => setConnection({ ...connection, presence_penalty: Number(e.target.value) })} />
      </label>
      <label>
        Repetition Penalty
        <input type="number" min="0" max="2" step="0.05" value={connection.repetition_penalty} onChange={e => setConnection({ ...connection, repetition_penalty: Number(e.target.value) })} />
      </label>
      <label>
        最大输出 Tokens
        <input type="number" value={connection.max_tokens} onChange={e => setConnection({ ...connection, max_tokens: Number(e.target.value) })} />
      </label>
      <button className="primary wide" onClick={() => void saveConnection()}>{saved ? <Check size={16} /> : <Settings size={16} />}保存设置</button>
      <div style={{ fontWeight: 'bold', fontSize: '14px', marginTop: '12px', marginBottom: '8px' }}>SillyTavern 资产</div>
      <input ref={worldInput} hidden type="file" accept=".json" onChange={e => { importAsset('worldbook', e.target.files?.[0]); e.currentTarget.value = ''; }} />
      <input ref={presetInput} hidden type="file" accept=".json" onChange={e => { importAsset('preset', e.target.files?.[0]); e.currentTarget.value = ''; }} />
      <input ref={regexInput} hidden type="file" accept=".json" onChange={e => { importAsset('regex', e.target.files?.[0]); e.currentTarget.value = ''; }} />
      <div className="compat-grid">
        <button onClick={() => worldInput.current?.click()}><BookOpen size={16} />导入世界书<small>{compat.worldbook} 条</small></button>
        <button onClick={() => presetInput.current?.click()}><SlidersHorizontal size={16} />导入预设</button>
        <button onClick={() => regexInput.current?.click()}><RefreshCw size={16} />导入 Regex<small>{compat.regex} 条</small></button>
        <button onClick={() => exportCard('json')}><Database size={16} />导出 V2 JSON</button>
        <button onClick={() => exportCard('png')}><Sparkles size={16} />导出 V2 PNG</button>
      </div>
    </div>
  );
}
