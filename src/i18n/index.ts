import enUS from './en-US';
import zhCN from './zh-CN';
import type { LanguageMode } from '../types';

const messages = {
  'en-US': enUS,
  'zh-CN': zhCN
};

export type Locale = keyof typeof messages;
type MessageTree = typeof enUS;

export function resolveLocale(language: LanguageMode): Locale {
  if (language === 'zh-CN' || language === 'en-US') {
    return language;
  }

  return navigator.language.toLowerCase().startsWith('zh') ? 'zh-CN' : 'en-US';
}

export function createTranslator(language: LanguageMode) {
  const locale = resolveLocale(language);
  const dictionary = messages[locale] as MessageTree;

  return (key: string, params: Record<string, string | number> = {}) => {
    const value = key.split('.').reduce<unknown>((current, part) => {
      if (current && typeof current === 'object' && part in current) {
        return (current as Record<string, unknown>)[part];
      }
      return undefined;
    }, dictionary);

    if (typeof value !== 'string') {
      return key;
    }

    return Object.entries(params).reduce((text, [name, replacement]) => {
      return text.split(`{${name}}`).join(String(replacement));
    }, value);
  };
}
