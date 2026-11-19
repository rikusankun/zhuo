import React from 'react';
import { BookOpen, Plus, Trash2 } from 'lucide-react';
import type { Lore } from '../../api/client';

export type ContextPanelProps = {
  persona: { name: string; description: string };
  setPersona: (p: { name: string; description: string }) => void;
  lore: Lore[];
  setLore: React.Dispatch<React.SetStateAction<Lore[]>>;
};

export function ContextPanel({ persona, setPersona, lore, setLore }: ContextPanelProps) {
  return (
    <div className="panel-scroll form-stack">
      <div style={{ fontWeight: 'bold', fontSize: '14px', marginBottom: '8px' }}>当前 Persona</div>
      <Field label="名称" value={persona.name} set={v => setPersona({ ...persona, name: v })} />
      <label>
        描述
        <textarea rows={5} value={persona.description} onChange={e => setPersona({ ...persona, description: e.target.value })} placeholder="身份、外观、说话方式和背景…" />
      </label>
      <div className="panel-section-title split">
        <span><BookOpen size={14} />World Info</span>
        <button onClick={() => setLore((a: Lore[]) => [...a, { id: crypto.randomUUID(), keys: '', content: '', enabled: true }])}><Plus size={15} />条目</button>
      </div>
      {lore.map(x => (
        <div className="lore-card" key={x.id}>
          <div className="lore-top">
            <button className={`switch ${x.enabled ? 'on' : ''}`} onClick={() => setLore((a: Lore[]) => a.map((v: Lore) => v.id === x.id ? { ...v, enabled: !v.enabled } : v))}><i /></button>
            <span>{x.enabled ? '已启用' : '已停用'}</span>
            <button className="danger-icon" onClick={() => setLore((a: Lore[]) => a.filter((v: Lore) => v.id !== x.id))}><Trash2 size={14} /></button>
          </div>
          <input value={x.keys} onChange={e => setLore((a: Lore[]) => a.map((v: Lore) => v.id === x.id ? { ...v, keys: e.target.value } : v))} placeholder="触发词，用逗号分隔" />
          <textarea rows={4} value={x.content} onChange={e => setLore((a: Lore[]) => a.map((v: Lore) => v.id === x.id ? { ...v, content: e.target.value } : v))} placeholder="命中后注入的世界设定…" />
        </div>
      ))}
      {!lore.length && <Empty icon={<BookOpen size={18} />} text="暂无 World Info 条目" />}
    </div>
  );
}

function Field({ label, value, set }: { label: string; value: string; set: (v: string) => void }) {
  return <label>{label}<input type="text" value={value} onChange={e => set(e.target.value)} /></label>;
}

function Empty({ icon, text }: { icon: React.ReactNode; text: string }) {
  return <div className="panel-empty">{icon}<p>{text}</p></div>;
}
