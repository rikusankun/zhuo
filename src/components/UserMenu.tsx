import React from 'react';
import { Brain, MoreHorizontal, Settings, UserRound } from 'lucide-react';
import { usePersona } from '../hooks/useConnectionContext';

const initials = (name: string) => name.trim().slice(0, 2).toUpperCase() || 'AI';

export type UserMenuProps = {
  userMenuOpen: boolean;
  setUserMenuOpen: React.Dispatch<React.SetStateAction<boolean>>;
  setPanel: (panel: 'context' | 'settings' | 'memory' | null) => void;
};

export function UserMenu({
  userMenuOpen,
  setUserMenuOpen,
  setPanel,
}: UserMenuProps) {
  const { persona } = usePersona();
  return (
    <div className="user-menu-anchor">
      <button className="persona-card" aria-expanded={userMenuOpen} aria-haspopup="menu" onClick={() => setUserMenuOpen(open => !open)}>
        <span className="avatar user">{initials(persona.name)}</span>
        <span>
          <strong>{persona.name}</strong><small>个人设置与偏好</small>
        </span>
        <MoreHorizontal size={18} />
      </button>
      {userMenuOpen && (
        <div className="user-menu-popover" role="menu">
          <button role="menuitem" onClick={() => { setPanel('settings'); setUserMenuOpen(false); }}><Settings size={16} />设置</button>
          <button role="menuitem" onClick={() => { setPanel('context'); setUserMenuOpen(false); }}><UserRound size={16} />个性与上下文</button>
          <button role="menuitem" onClick={() => { setPanel('memory'); setUserMenuOpen(false); }}><Brain size={16} />记忆与状态</button>
        </div>
      )}
    </div>
  );
}
