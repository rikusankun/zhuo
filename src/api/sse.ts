export type ChatEvent = {
  token?: string;
  processed?: string;
  raw_content?: string;
  error?: string;
  prompt_chars?: number;
  estimated_tokens?: number;
  usage?: { total_tokens?: number };
};

/** Decode complete SSE events, including split UTF-8 and a final frame without a newline. */
export async function consumeChatStream(
  stream: ReadableStream<Uint8Array>,
  onEvent: (event: ChatEvent) => void,
): Promise<void> {
  const reader = stream.getReader();
  const decoder = new TextDecoder();
  let buffer = '';
  let data: string[] = [];
  let finished = false;
  const dispatch = () => {
    if (!data.length) return;
    const raw = data.join('\n').trim();
    data = [];
    if (raw === '[DONE]') { finished = true; return; }
    if (!raw) return;
    const event: unknown = JSON.parse(raw);
    if (!event || typeof event !== 'object' || Array.isArray(event)) {
      throw new Error('无效的流式响应');
    }
    onEvent(event as ChatEvent);
  };
  const line = (value: string) => {
    if (value === '') dispatch();
    else if (value.startsWith('data:')) data.push(value.slice(5).replace(/^ /, ''));
  };
  try {
    while (!finished) {
      const { done, value } = await reader.read();
      buffer += done ? decoder.decode() : decoder.decode(value, { stream: true });
      let boundary: number;
      while ((boundary = buffer.indexOf('\n')) !== -1 && !finished) {
        line(buffer.slice(0, boundary).replace(/\r$/, ''));
        buffer = buffer.slice(boundary + 1);
      }
      if (done) {
        if (buffer) line(buffer.replace(/\r$/, ''));
        dispatch();
        if (!finished) throw new Error('连接提前结束，请刷新对话确认保存状态');
        break;
      }
    }
  } finally {
    await reader.cancel().catch(() => {});
    reader.releaseLock();
  }
}
