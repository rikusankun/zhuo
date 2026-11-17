import { useCallback, useEffect, useState } from 'react';
import { Brain, Edit3, Plus, RefreshCw, Trash2, X } from 'lucide-react';
import { api, isApiError, type Memory } from '../api/client';

const HINT =
  'AI 自动提取的记忆可能有误，请定期检查修正。错误记忆会影响后续对话质量。';

export function MemoryManagement({
  cards,
  cardId,
}: {
  cards: { id: string; name: string }[];
  cardId: string;
}) {
  const [selectedCard, setSelectedCard] = useState(
    cardId || cards[0]?.id || '',
  );
  const [memories, setMemories] = useState<Memory[]>([]);
  const [loading, setLoading] = useState(false);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [editContent, setEditContent] = useState('');
  const [addOpen, setAddOpen] = useState(false);
  const [addTitle, setAddTitle] = useState('');
  const [addContent, setAddContent] = useState('');
  const [confirmDeleteId, setConfirmDeleteId] = useState<number | null>(null);
  const [notice, setNotice] = useState('');
  const [error, setError] = useState('');

  const load = useCallback(async () => {
    if (!selectedCard) return;
    setLoading(true);
    setError('');
    try {
      const result = await api.memory.list(selectedCard);
      if (isApiError(result)) throw new Error(result.error);
      setMemories(Array.isArray(result.memories) ? result.memories : []);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, [selectedCard]);

  useEffect(() => {
    void load();
  }, [load]);

  const saveEdit = async (id: number) => {
    const content = editContent.trim();
    if (!content) {
      setError('内容不能为空');
      return;
    }
    try {
      const result = await api.memory.update(id, selectedCard, content);
      if (isApiError(result)) throw new Error(result.error);
      setNotice('记忆已更新');
      setEditingId(null);
      setEditContent('');
      void load();
    } catch (e) {
      setError(`保存失败：${String(e)}`);
    }
  };

  const removeMemory = async (id: number) => {
    try {
      const result = await api.memory.remove(id, selectedCard);
      if (isApiError(result)) throw new Error(result.error);
      setNotice('记忆已删除');
      setConfirmDeleteId(null);
      void load();
    } catch (e) {
      setError(`删除失败：${String(e)}`);
    }
  };

  const addMemory = async () => {
    if (!addContent.trim()) {
      setError('内容不能为空');
      return;
    }
    try {
      const result = await api.memory.add(
        selectedCard,
        addContent.trim(),
        addTitle.trim() || undefined,
      );
      if (isApiError(result)) throw new Error(result.error);
      setNotice('记忆已添加');
      setAddOpen(false);
      setAddTitle('');
      setAddContent('');
      void load();
    } catch (e) {
      setError(`添加失败：${String(e)}`);
    }
  };

  if (!selectedCard || !cards.length) {
    return (
      <div className="panel-section-title split">
        <span><Brain size={14} />记忆管理</span>
      </div>
    );
  }

  const currentCard = cards.find((c) => c.id === selectedCard);

  return (
    <div className="form-stack">
      <div className="panel-section-title split">
        <span><Brain size={14} />记忆管理</span>
        <button
          title="刷新"
          onClick={() => void load()}
          disabled={loading}
        >
          <RefreshCw size={13} />
        </button>
      </div>

      <label>角色</label>
      <select
        value={selectedCard}
        onChange={(e) => {
          setSelectedCard(e.target.value);
          setEditingId(null);
          setConfirmDeleteId(null);
        }}
      >
        {cards.map((c) => (
          <option key={c.id} value={c.id}>
            {c.name}
          </option>
        ))}
      </select>
      {currentCard && (
        <small className="settings-help">
          当前查看：{currentCard.name}（记忆按角色隔离，切换角色不共享）
        </small>
      )}

      <div className="settings-help hint">{HINT}</div>

      <div className="memory-table">
        {loading ? (
          <div className="memory-row">加载中…</div>
        ) : error ? (
          <div className="memory-row error">{error}</div>
        ) : memories.length === 0 ? (
          <div className="memory-row empty">该角色尚无长期记忆</div>
        ) : (
          memories.map((m) => {
            const isEditing = editingId === m.id;
            const isConfirming = confirmDeleteId === m.id;
            const title = m.summary.slice(0, 24);
            const preview = m.summary.slice(0, 80);
            return (
              <div
                key={m.id}
                className={`memory-row${isEditing ? ' editing' : ''}`}
              >
                <div className="memory-body">
                  <div className="memory-title">{title}</div>
                  {isEditing ? (
                    <textarea
                      className="memory-edit"
                      value={editContent}
                      onChange={(e) => setEditContent(e.target.value)}
                      onBlur={() => void saveEdit(m.id)}
                      autoFocus
                    />
                  ) : (
                    <div className="memory-content" title={m.summary}>
                      {preview}{m.summary.length > 80 ? '…' : ''}
                    </div>
                  )}
                  <div className="memory-meta">
                    {m.source_round ? `自动提取 #${m.source_round}` : '手动添加'} ·
                    {m.created_at}
                  </div>
                </div>
                <div className="memory-actions">
                  {isConfirming ? (
                    <>
                      <button
                        className="danger-icon confirm"
                        title="确认删除"
                        onClick={() => void removeMemory(m.id)}
                      >
                        确认
                      </button>
                      <button
                        className="secondary danger-icon"
                        title="取消"
                        onClick={() => setConfirmDeleteId(null)}
                      >
                        取消
                      </button>
                    </>
                  ) : (
                    <>
                      <button
                        className="icon-button"
                        title={isEditing ? '取消编辑' : '编辑'}
                        onClick={() => {
                          setEditingId(m.id);
                          setEditContent(m.summary);
                          setConfirmDeleteId(null);
                        }}
                      >
                        <Edit3 size={13} />
                      </button>
                      <button
                        className="icon-button danger-icon"
                        title="删除"
                        onClick={() => {
                          setEditingId(null);
                          setConfirmDeleteId(m.id);
                        }}
                      >
                        <Trash2 size={13} />
                      </button>
                    </>
                  )}
                </div>
              </div>
            );
          })
        )}
      </div>

      {notice && (
        <div className="status">{notice}<button onClick={() => setNotice('')}>✕</button></div>
      )}
      {error && !notice && (
        <div className="status error">{error}<button onClick={() => setError('')}>✕</button></div>
      )}

      <div className="memory-add">
        <button
          className="secondary wide"
          onClick={() => {
            setAddOpen(true);
            setEditingId(null);
            setConfirmDeleteId(null);
          }}
        >
          <Plus size={14} /> + 手动添加
        </button>
      </div>

      {addOpen && (
        <div className="memory-dialog">
          <div className="memory-dialog-panel">
            <div className="memory-dialog-head">
              <strong>手动添加记忆</strong>
              <button className="close" onClick={() => setAddOpen(false)}>
                <X size={16} />
              </button>
            </div>
            <label>标题（可选）</label>
            <input
              value={addTitle}
              onChange={(e) => setAddTitle(e.target.value)}
              placeholder="给这条记忆起个标题"
            />
            <label>内容</label>
            <textarea
              rows={4}
              value={addContent}
              onChange={(e) => setAddContent(e.target.value)}
              placeholder="输入要持久化的记忆内容，如：角色的偏好、关系、关键事实…"
            />
            <div className="memory-dialog-actions">
              <button className="secondary" onClick={() => setAddOpen(false)}>
                取消
              </button>
              <button className="primary wide" onClick={addMemory} disabled={!addContent.trim()}>
                添加记忆
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
