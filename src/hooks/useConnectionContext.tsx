import { createContext, useContext, type ReactNode } from 'react';
import type { Connection } from '../api/client';

export type PersonaContextValue = {
  persona: { name: string; description: string };
  setPersona: (p: { name: string; description: string }) => void;
};

const PersonaContext = createContext<PersonaContextValue | null>(null);

export function PersonaProvider({ children, persona, setPersona }: { children: ReactNode; persona: { name: string; description: string }; setPersona: (p: { name: string; description: string }) => void }) {
  return (
    <PersonaContext.Provider value={{ persona, setPersona }}>
      {children}
    </PersonaContext.Provider>
  );
}

export function usePersona() {
  const ctx = useContext(PersonaContext);
  if (!ctx) throw new Error('usePersona must be used within a PersonaProvider');
  return ctx;
}

export type ConnectionContextValue = {
  connection: Connection;
  setConnection: (c: Connection) => void;
};

const ConnectionContext = createContext<ConnectionContextValue | null>(null);

export function ConnectionProvider({ children, connection, setConnection }: { children: ReactNode; connection: Connection; setConnection: (c: Connection) => void }) {
  return (
    <ConnectionContext.Provider value={{ connection, setConnection }}>
      {children}
    </ConnectionContext.Provider>
  );
}

export function useConnection() {
  const ctx = useContext(ConnectionContext);
  if (!ctx) throw new Error('useConnection must be used within a ConnectionProvider');
  return ctx;
}
