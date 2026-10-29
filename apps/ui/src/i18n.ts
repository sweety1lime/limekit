// Interface strings in Russian and English. The rest of the window is still
// Russian only, so the language stays Russian; new screens put their strings
// here from the start, so that switching later is a matter of this file.

export type Lang = "ru" | "en";

export const lang: Lang = "ru";

const strings = {
  "nav.windows": { ru: "Windows", en: "Windows" },
  "windows.title": { ru: "Настройка Windows", en: "Customize Windows" },
  "windows.tabTweaks": { ru: "Твики", en: "Tweaks" },
  "windows.tabThemes": { ru: "Темы", en: "Themes" },
  "strip.title": { ru: "Настройка Windows", en: "Customize Windows" },
  "strip.sub": {
    ru: "Твики системы, темы курсоров и звуков. Любое изменение можно отменить.",
    en: "System tweaks, cursor and sound themes. Every change can be undone.",
  },
  "strip.tweaks": { ru: "Твики Windows", en: "Windows tweaks" },
  "strip.themes": { ru: "Темы", en: "Themes" },
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

  "category.explorer": { ru: "Проводник", en: "File Explorer" },
  "category.taskbar": { ru: "Панель задач", en: "Taskbar" },
  "category.start": { ru: "Меню «Пуск»", en: "Start" },
  "category.suggestions": { ru: "Советы и реклама Windows", en: "Windows tips and ads" },
  "category.privacy": { ru: "Конфиденциальность", en: "Privacy" },
  "category.notifications": { ru: "Уведомления", en: "Notifications" },
  "category.appearance": { ru: "Оформление", en: "Appearance" },

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

  "effect.new_windows": {
    ru: "заметно в новых окнах Проводника",
    en: "shows in new File Explorer windows",
  },
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

  // No quotes around {title}: titles carry their own, as in «Пуск».
  "confirm.onTitle": { ru: "Включить: {title}", en: "Turn on: {title}" },
  "confirm.offTitle": { ru: "Выключить: {title}", en: "Turn off: {title}" },
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

  "outcome.on": { ru: "Включено: {title}", en: "On: {title}" },
  "outcome.off": { ru: "Выключено: {title}", en: "Off: {title}" },
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
    ru: "Панель задач и значки рабочего стола пропадут на несколько секунд, окна Проводника закроются и откроются снова. Программы и документы это не затронет.",
    en: "The taskbar and desktop icons disappear for a few seconds, and File Explorer windows close and open again. Programs and documents are not affected.",
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

  "themes.intro": {
    ru: "Тема меняет курсоры или звуки Windows. Свои темы можно добавить файлом .limetheme: он только попадает в список, пока вы не нажмёте «Применить». LimeKit записывает каждое изменение в журнал, поэтому его можно отменить. Права администратора не нужны.",
    en: "A theme changes the cursors or the sounds of Windows. Add your own from a .limetheme file: it only joins the list until you press Apply. LimeKit writes every change to a journal, so it can be undone. No admin rights needed.",
  },
  "themes.loading": { ru: "Читаю темы…", en: "Reading themes…" },
  "themes.empty": { ru: "Windows не назвала ни одной темы.", en: "Windows did not list any themes." },
  "themes.partCursors": { ru: "Курсоры", en: "Cursors" },
  "themes.partSounds": { ru: "Звуки", en: "Sounds" },
  "themes.groupPackage": { ru: "Мои темы", en: "My themes" },
  "themes.groupLimeKit": { ru: "Встроенные", en: "Built in" },
  "themes.groupWindows": { ru: "Курсоры из Windows", en: "Cursors from Windows" },
  "themes.fromWindows": { ru: "из Windows", en: "from Windows" },
  "themes.fromLimeKit": { ru: "встроенная тема", en: "built-in theme" },
  "themes.fromPackage": { ru: "из пакета", en: "from a package" },
  "themes.add": { ru: "Добавить тему", en: "Add a theme" },
  "themes.fileFilter": { ru: "Темы LimeKit", en: "LimeKit themes" },
  "themes.importing": { ru: "Добавляю тему…", en: "Adding the theme…" },
  "themes.imported": { ru: "Тема «{title}» добавлена", en: "Theme “{title}” added" },
  "themes.remove": { ru: "Удалить", en: "Delete" },
  "themes.removeTitle": { ru: "Удалить тему", en: "Delete the theme" },
  "themes.removeQuestion": {
    ru: "Удалить тему «{title}» из библиотеки? Сам файл пакета останется там, где лежит.",
    en: "Delete the theme “{title}” from the library? The package file stays where it is.",
  },
  "themes.removeApplied": {
    ru: "Тема применена: LimeKit сначала вернёт то, что стояло до неё.",
    en: "The theme is applied: LimeKit puts back what was there before it first.",
  },
  "themes.removed": { ru: "Тема «{title}» удалена", en: "Theme “{title}” deleted" },
  "themes.skipped": {
    ru: "Не применятся события, которых нет в этой Windows: {list}",
    en: "These events are not in this Windows and are skipped: {list}",
  },
  "themes.cursors": { ru: "курсоры", en: "cursors" },
  "themes.sounds": { ru: "звуки", en: "sounds" },
  "themes.silent": { ru: "без звука", en: "silent" },
  "themes.apply": { ru: "Применить", en: "Apply" },
  "themes.revert": { ru: "Вернуть", en: "Put back" },
  "themes.applied": {
    ru: "применена через LimeKit, «Вернуть» вернёт как было",
    en: "applied by LimeKit; Put back restores what was there",
  },
  "themes.builtin": { ru: "встроенный", en: "built-in" },
  "themes.nothing": {
    ru: "Ничего не изменится: всё уже так, как в теме.",
    en: "Nothing changes: everything is already as the theme has it.",
  },
  "themes.replaces": {
    ru: "Сейчас применена тема «{title}», LimeKit сначала снимет её.",
    en: "The theme “{title}” is applied now; LimeKit takes it off first.",
  },
  "themes.unverified": {
    ru: "Темы ещё не проверяли на вашей сборке Windows. Если что-то пойдёт не так, тему можно отменить.",
    en: "Nobody has checked themes on your Windows build yet. If something goes wrong, the theme can be undone.",
  },
  "themes.onTitle": { ru: "Применить тему: {title}", en: "Apply theme: {title}" },
  "themes.offTitle": { ru: "Вернуть: {title}", en: "Put back: {title}" },
  "themes.outcomeOn": { ru: "Применена тема: {title}", en: "Theme applied: {title}" },
  "themes.outcomeOff": { ru: "Тема снята: {title}", en: "Theme taken off: {title}" },
  "themes.notAvailable": { ru: "Темы не для вашей Windows", en: "Themes are not for your Windows" },
  "themes.drift": {
    ru: "Windows или другая программа поменяли то, что поставила тема: {list}.",
    en: "Windows or another program changed what a theme had set: {list}.",
  },

  "cursor.Arrow": { ru: "Основной режим", en: "Normal select" },
  "cursor.Help": { ru: "Выбор справки", en: "Help select" },
  "cursor.AppStarting": { ru: "Фоновый режим", en: "Working in background" },
  "cursor.Wait": { ru: "Занят", en: "Busy" },
  "cursor.Crosshair": { ru: "Точное выделение", en: "Precision select" },
  "cursor.IBeam": { ru: "Выделение текста", en: "Text select" },
  "cursor.NWPen": { ru: "Рукописный ввод", en: "Handwriting" },
  "cursor.No": { ru: "Недоступно", en: "Unavailable" },
  "cursor.SizeNS": { ru: "Изменение размера по вертикали", en: "Vertical resize" },
  "cursor.SizeWE": { ru: "Изменение размера по горизонтали", en: "Horizontal resize" },
  "cursor.SizeNWSE": { ru: "Диагональное изменение размера 1", en: "Diagonal resize 1" },
  "cursor.SizeNESW": { ru: "Диагональное изменение размера 2", en: "Diagonal resize 2" },
  "cursor.SizeAll": { ru: "Перемещение", en: "Move" },
  "cursor.UpArrow": { ru: "Альтернативное выделение", en: "Alternate select" },
  "cursor.Hand": { ru: "Выбор ссылки", en: "Link select" },
  "cursor.Pin": { ru: "Выбор места", en: "Location select" },
  "cursor.Person": { ru: "Выбор человека", en: "Person select" },
} as const;

export type Key = keyof typeof strings;

/** The string for `key`, with `{name}` placeholders filled from `vars`. */
export function t(key: Key, vars: Record<string, string | number> = {}): string {
  return strings[key][lang].replace(/\{(\w+)\}/g, (whole, name: string) =>
    name in vars ? String(vars[name]) : whole,
  );
}
