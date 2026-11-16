import type { RegexScript } from '../api/client';

export function applyRegex(text: string, scripts: RegexScript[]): string {
  return scripts.filter(s => !s.disabled && !s.prompt_only && !s.promptOnly &&
    (!s.placement?.length || s.placement.includes(2))).reduce((value, script) => {
    const source = (script.find_regex || script.findRegex || '').trim();
    if (!source) return value;
    try {
      let pattern = source, flags = '';
      if (source.startsWith('/')) {
        const end = source.lastIndexOf('/');
        if (end > 0) { pattern = source.slice(1, end); flags = source.slice(end + 1); }
      }
      return value.replace(new RegExp(pattern, flags),
        (script.replace_string ?? script.replaceString ?? '').split('{{match}}').join('$&'));
    } catch { return value; }
  }, text);
}
