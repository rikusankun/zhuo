import React from 'react';
import { Brain, RefreshCw } from 'lucide-react';
import type { Memory } from '../../api/client';

export type MemoryPanelProps = {
  memories: Memory[];
  memoryTotal: number;
  messagesLength: number;
  loadMemory: () => void;
  stateText: string;
  setStateText: React.Dispatch<React.SetStateAction<string>>;
  saveState: () => void;
};

export function MemoryPanel({ memories, memoryTotal, messagesLength, loadMemory, stateText, setStateText, saveState }: MemoryPanelProps) {
  return (
    <div className="panel-scroll form-stack">
      <div className="metric-row">
        <div><span>长期记忆</span><strong>{memoryTotal}</strong></div>
        <div><span>当前消息</span><strong>{messagesLength}</strong></div>
      </div>
      <div className="panel-section-title split">
        <span><Brain size={14} />记忆库</span>
        <button onClick={loadMemory}><RefreshCw size={14} />刷新</button>
      </div>
      <div className="memory-list">
        {memories.map(x => (
          <div key={x.id}>
            <p>{x.summary}</p>
            <small>{x.source_round ? `自动提取 #${x.source_round}` : '手动添加'} · {x.created_at}</small>
          </div>
        ))}
        {!memories.length && <Empty icon={<Brain size={18} />} text="还没有长期记忆" />}
      </div>
      <div style={{ fontWeight: 'bold', fontSize: '14px', marginTop: '12px', marginBottom: '8px' }}>角色状态 JSON</div>
      <textarea className="code-editor" rows={10} value={stateText} onChange={e => setStateText(e.target.value)} />
      <button className="secondary wide" onClick={saveState}>保存角色状态</button>
    </div>
  );
}

function Empty({ icon, text }: { icon: React.ReactNode; text: string }) {
  return <div className="panel-empty">{icon}<p>{text}</p></div>;
}
