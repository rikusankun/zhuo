export type ApiError = { error: string };
export type ApiResult<T> = T | ApiError;

export type Card = { id: string; name: string; description: string; first_mes: string };
export type Chat = {
  id: string;
  card: string;
  title: string;
  created_at: string;
  updated_at: string;
};
export type Message = { role: 'user' | 'assistant' | 'system'; content: string };
export type Memory = { id: number; summary: string; created_at: string; source_round: number };
export type Connection = {
  provider: string;
  base_url: string;
  api_key: string;
  model: string;
  summary_model: string;
  num_gpu: number;
  temperature: number;
  max_tokens: number;
  top_p: number;
  frequency_penalty: number;
  presence_penalty: number;
  repetition_penalty: number;
};
export type Lore = { id: string; keys: string; content: string; enabled: boolean };
export type RegexScript = {
  script_name?: string;
  scriptName?: string;
  find_regex?: string;
  findRegex?: string;
  replace_string?: string;
  replaceString?: string;
  placement?: number[];
  disabled?: boolean;
  prompt_only?: boolean;
  promptOnly?: boolean;
};
export type CompatibilityAsset = 'worldbook' | 'preset' | 'regex';
export type ChatInput = {
  card: string;
  chat_id: string;
  message: string;
  display_message: string;
  regenerate?: boolean;
  persona: { name: string; description: string };
  lore_entries: Lore[];
};

async function requestJson<T>(path: string, init?: RequestInit): Promise<ApiResult<T>> {
  const response = await fetch(path, init);
  let payload: unknown;
  try {
    payload = await response.json();
  } catch {
    if (response.ok) throw new Error(`Invalid JSON response (HTTP ${response.status})`);
  }
  if (!response.ok) {
    const message =
      typeof payload === 'object' &&
      payload !== null &&
      'error' in payload &&
      typeof payload.error === 'string'
        ? payload.error
        : `HTTP ${response.status}`;
    throw new Error(message);
  }
  return payload as ApiResult<T>;
}

function jsonBody(method: string, body: unknown): RequestInit {
  return {
    method,
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  };
}

export function isApiError<T extends object>(result: ApiResult<T>): result is ApiError {
  return typeof result === 'object' && result !== null && 'error' in result;
}

export const api = {
  cards: {
    list: () => requestJson<Card[]>('/api/cards'),
    create: (card: {
      name: string;
      description: string;
      first_mes: string;
      system_prompt: string;
    }) => requestJson<{ ok: boolean; card: Card }>('/api/cards', jsonBody('POST', card)),
    delete: (id: string) =>
      requestJson<{ ok: boolean }>(`/api/cards/${encodeURIComponent(id)}`, {
        method: 'DELETE',
      }),
    import: (filename: string, data: string) =>
      requestJson<{ card: Card }>(
        '/api/cards/import',
        jsonBody('POST', { filename, data }),
      ),
    export: (id: string, format: 'json' | 'png') =>
      requestJson<{ filename: string; mime: string; data: string }>(
        `/api/cards/${encodeURIComponent(id)}/export?format=${format}`,
      ),
  },
  chats: {
    list: (card: string) =>
      requestJson<Chat[]>(`/api/chats?card=${encodeURIComponent(card)}`),
    create: (card: string) =>
      requestJson<Chat>('/api/chats', jsonBody('POST', { card })),
    delete: (id: string) =>
      requestJson<{ ok: boolean }>(`/api/chats/${encodeURIComponent(id)}`, {
        method: 'DELETE',
      }),
    rename: (id: string, title: string) =>
      requestJson<{ ok: boolean }>(
        `/api/chats/${encodeURIComponent(id)}`,
        jsonBody('PATCH', { title }),
      ),
  },
  history: {
    get: (chat: string) =>
      requestJson<Message[]>(`/api/history?chat=${encodeURIComponent(chat)}`),
  },
  ollama: {
    models: () => requestJson<{ models?: string[] }>('/api/ollama/tags'),
  },
  compatibility: {
    get: () => requestJson<{
      regex_scripts?: RegexScript[];
      worldbook?: { entries?: unknown[] };
    }>('/api/compatibility'),
    import: (kind: CompatibilityAsset, filename: string, data: string) =>
      requestJson<{
        connection?: Connection;
        entries?: number;
        scripts?: RegexScript[];
      }>(`/api/compatibility/${kind}`, jsonBody('POST', { filename, data })),
  },
  connection: {
    get: () =>
      requestJson<{ connection?: Connection }>('/api/connection'),
    save: (connection: Connection) =>
      requestJson<{ ok: boolean }>('/api/connection', jsonBody('PUT', connection)),
  },
  chat: {
    stream: async (
      input: ChatInput,
      signal: AbortSignal,
    ): Promise<ReadableStream<Uint8Array>> => {
      const response = await fetch(
        '/api/chat',
        { ...jsonBody('POST', input), signal },
      );
      if (!response.ok || !response.body) {
        throw new Error(`HTTP ${response.status}`);
      }
      return response.body;
    },
  },
  memory: {
    list: (card: string) =>
      requestJson<{ memories?: Memory[]; total?: number }>(
        `/api/memories?card=${encodeURIComponent(card)}`,
      ),
  },
  state: {
    get: (card: string) =>
      requestJson<{ state?: Record<string, unknown> | null }>(
        `/api/state?card=${encodeURIComponent(card)}`,
      ),
    save: (card: string, state: object) =>
      requestJson<{ ok: boolean }>('/api/state', jsonBody('PUT', { card, state })),
  },
};
