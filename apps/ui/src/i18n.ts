// Interface strings in Russian and English. The rest of the window is still
// Russian only, so the language stays Russian; new screens put their strings
// here from the start, so that switching later is a matter of this file.

export type Lang = "ru" | "en";

export const lang: Lang = "ru";

const strings = {
  "nav.tweaks": { ru: "Твики", en: "Tweaks" },
  "tweaks.title": { ru: "Твики Windows", en: "Windows tweaks" },
  "tweaks.back": { ru: "← Библиотека", en: "← Library" },
  "tweaks.intro": {
    ru: "Каждое изменение LimeKit сначала записывает в журнал, поэтому его можно отменить. Права администратора не нужны: меняются только настройки вашей учётной записи.",
    en: "LimeKit writes every change to a journal first, so it can be undone. No admin rights needed: only your account's settings change.",
  },
  "tweaks.loading": { ru: "Читаю настройки…", en: "Reading settings…" },
  "tweaks.empty": { ru: "В каталоге пока нет твиков.", en: "The catalog has no tweaks yet." },
  "tweaks.undoAll": { ru: "Вернуть всё", en: "Undo everything" },
  "tweaks.exportUndo": { ru: "Откат в .reg", en: "Undo as .reg" },
  "tweaks.drift": {
    ru: "Windows или другая программа поменяли то, что включил LimeKit: {list}.",
    en: "Windows or another program changed what LimeKit had set: {list}.",
  },

  "provenance.documented": { ru: "документировано Microsoft", en: "documented by Microsoft" },
  "provenance.observed": { ru: "так пишет сама Windows", en: "what Windows itself writes" },
  "provenance.unsupported": { ru: "неподдерживаемый приём", en: "unsupported trick" },

  "availability.verified": { ru: "проверено на этой сборке", en: "checked on this build" },
  "availability.unverified": { ru: "не проверено на этой сборке", en: "not checked on this build" },
  "availability.built_in": { ru: "есть в Параметрах Windows", en: "built into Windows Settings" },
  "availability.too_old": { ru: "нужна сборка {build} или новее", en: "needs build {build} or later" },
  "availability.too_new": { ru: "только для сборок до {build}", en: "only for builds before {build}" },
  "availability.wrong_edition": { ru: "только для {editions}", en: "only for {editions}" },
  "availability.openSettings": { ru: "Открыть Параметры", en: "Open Settings" },

  "effect.explorer_restart": {
    ru: "заметно после перезапуска Проводника",
    en: "shows after File Explorer restarts",
  },
  "effect.sign_out": { ru: "заметно после выхода из системы", en: "shows after signing out" },

  "state.mixed": { ru: "сейчас стоит другое значение", en: "set to something else now" },
  "state.applied": {
    ru: "включено через LimeKit, выключение вернёт как было",
    en: "turned on by LimeKit; off puts back what was there",
  },

  "confirm.onTitle": { ru: "Включить «{title}»", en: "Turn on “{title}”" },
  "confirm.offTitle": { ru: "Выключить «{title}»", en: "Turn off “{title}”" },
  "confirm.changes": { ru: "Что изменится:", en: "What changes:" },
  "confirm.revert": {
    ru: "LimeKit вернёт значения, которые стояли до него.",
    en: "LimeKit puts back the values that were there before it.",
  },
  "confirm.unverified": {
    ru: "Этот твик ещё не проверяли на вашей сборке Windows. Если что-то пойдёт не так, его можно отменить.",
    en: "Nobody has checked this tweak on your Windows build yet. If something goes wrong, it can be undone.",
  },
  "confirm.unsupported": {
    ru: "Это неподдерживаемый приём: Windows может перестать его понимать после очередного обновления.",
    en: "This is an unsupported trick: Windows may stop honoring it with any update.",
  },
  "confirm.on": { ru: "Включить", en: "Turn on" },
  "confirm.off": { ru: "Выключить", en: "Turn off" },
  "confirm.cancel": { ru: "Отмена", en: "Cancel" },

  "value.absent": { ru: "нет значения", en: "no value" },
  "value.on": { ru: "включено", en: "on" },
  "value.off": { ru: "выключено", en: "off" },

  "outcome.on": { ru: "«{title}» включено", en: "“{title}” is on" },
  "outcome.off": { ru: "«{title}» выключено", en: "“{title}” is off" },
  "outcome.already": { ru: "Уже так, ничего не меняли", en: "Already like that, nothing changed" },
  "outcome.changedSince": {
    ru: "Эти значения поменяли уже после LimeKit, их не трогали: {list}",
    en: "These values were changed after LimeKit set them and were left alone: {list}",
  },
  "outcome.accessDenied": {
    ru: "Windows не дала изменить {target}: этот параметр она защищает от программ",
    en: "Windows refused to change {target}: it protects this setting from programs",
  },
  "outcome.notTaken": {
    ru: "Windows приняла изменение, но оставила своё значение: {target}. Ничего не изменилось.",
    en: "Windows took the change but kept its own value: {target}. Nothing changed.",
  },
  "outcome.notAvailable": { ru: "Этот твик не для вашей Windows", en: "This tweak is not for your Windows" },

  "restart.pending": {
    ru: "Часть изменений появится после перезапуска Проводника.",
    en: "Some changes show once File Explorer restarts.",
  },
  "restart.button": { ru: "Перезапустить Проводник", en: "Restart File Explorer" },
  "restart.question": {
    ru: "Панель задач и значки рабочего стола пропадут на несколько секунд, открытые окна Проводника закроются. Программы и документы это не затронет.",
    en: "The taskbar and desktop icons disappear for a few seconds and open File Explorer windows close. Programs and documents are not affected.",
  },
  "restart.ok": { ru: "Перезапустить", en: "Restart" },
  "restart.running": { ru: "Перезапускаю Проводник…", en: "Restarting File Explorer…" },
  "restart.done": { ru: "Проводник перезапущен", en: "File Explorer restarted" },

  "undoAll.title": { ru: "Вернуть всё", en: "Undo everything" },
  "undoAll.question": {
    ru: "Отменить все изменения, которые сделал LimeKit? Значения вернутся такими, какими были до него.",
    en: "Undo every change LimeKit made? The values go back to what they were before it.",
  },
  "undoAll.done": { ru: "Отменено изменений: {count}", en: "Changes undone: {count}" },
  "undoAll.none": { ru: "Отменять нечего", en: "Nothing to undo" },

  "export.title": { ru: "Сохранить откат", en: "Save the undo" },
  "export.filter": { ru: "Файл реестра", en: "Registry file" },
  "export.done": {
    ru: "Откат сохранён. Двойной щелчок по файлу вернёт значения и без LimeKit: {path}",
    en: "Undo saved. Double-clicking the file puts the values back, LimeKit or not: {path}",
  },
} as const;

export type Key = keyof typeof strings;

/** The string for `key`, with `{name}` placeholders filled from `vars`. */
export function t(key: Key, vars: Record<string, string | number> = {}): string {
  return strings[key][lang].replace(/\{(\w+)\}/g, (whole, name: string) =>
    name in vars ? String(vars[name]) : whole,
  );
}
