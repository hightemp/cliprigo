import type { Messages } from "./index";

export const ru: Messages = {
  app: {
    ytdlpUpdated: "yt-dlp обновлён до {version}",
  },
  tabs: {
    download: "Загрузка",
    history: "История",
    settings: "Настройки",
  },
  common: {
    cancel: "Отмена",
    close: "Закрыть",
    retry: "Повторить",
    loading: "Загрузка…",
    change: "Изменить",
  },
  components: {
    ytdlp: "yt-dlp",
    ffmpeg: "ffmpeg",
    deno: "Deno",
  },
  setup: {
    title: "Подготовка",
    description:
      "Для работы нужны yt-dlp, ffmpeg и Deno. Они скачиваются один раз из официальных релизов.",
    waiting: "Ожидание",
    downloading: "Скачивание {name}…",
    verifying: "Проверка {name}…",
    installing: "Установка {name}…",
    installed: "Установлено",
    failed: "Не удалось установить {name}",
  },
  download: {
    placeholder: "Вставьте ссылку на YouTube",
    paste: "Вставить",
    probe: "Получить",
    probing: "Получаем информацию о видео…",
    notYoutube:
      "Это не похоже на ссылку YouTube. Может сработать, но без гарантий.",
    video: "Видео",
    audio: "Только звук",
    quality: "Качество",
    codec: "Кодек",
    best: "Лучшее доступное",
    audioFormat: "Формат",
    folder: "Папка",
    start: "Скачать",
    startCount: {
      one: "Скачать {count} видео",
      other: "Скачать {count} видео",
    },
    added: "Добавлено в очередь",
  },
  playlist: {
    count: {
      one: "{count} видео",
      other: "{count} видео",
    },
    selectAll: "Выбрать все",
    selected: "Выбрано {selected} из {total}",
    unavailable: "Недоступно",
    alreadyDownloaded: "Уже скачано",
    inQueue: "Уже в очереди",
  },
  duplicate: {
    title: "Уже скачано",
    completed: "«{title}» уже есть в истории.",
    active: "«{title}» уже в очереди.",
    again: "Скачать снова",
    open: "Открыть файл",
  },
  queue: {
    title: "Очередь",
    empty: "Сейчас ничего не скачивается",
    count: {
      one: "{count} загрузка",
      few: "{count} загрузки",
      many: "{count} загрузок",
      other: "{count} загрузки",
    },
    waiting: "Ожидает",
    part: "часть {part}",
    left: "осталось {eta}",
    merging: "Склейка видео и звука…",
    converting: "Конвертация звука…",
    processing: "Обработка…",
  },
  status: {
    queued: "В очереди",
    downloading: "Скачивается",
    processing: "Обработка",
    completed: "Готово",
    failed: "Ошибка",
    canceled: "Отменено",
    interrupted: "Прервано",
  },
  history: {
    search: "Поиск по названию или каналу",
    all: "Все",
    empty: "Пока ничего не скачано",
    noResults: "Ничего не найдено",
    fileMissing: "Файл не найден",
    open: "Открыть",
    showInFolder: "Показать в папке",
    copyLink: "Скопировать ссылку",
    openYoutube: "Открыть на YouTube",
    downloadAgain: "Скачать снова",
    retry: "Повторить",
    remove: "Удалить",
    copied: "Ссылка скопирована",
    confirmRemove: "Удалить «{title}» из истории?",
    deleteFile: "Также удалить файл с диска",
    loadMore: "Показать ещё",
    added: "Добавлено в очередь",
  },
  codecs: {
    name: {
      auto: "Авто",
      h264: "H.264 — совместимый",
      av1: "AV1 — компактный",
      vp9: "VP9",
    },
    hint: {
      auto: "Лучшее разрешение. До 1080p — H.264, который открывается в любом плеере; выше (2K, 4K) у YouTube есть только VP9 и AV1 — для них подойдёт VLC.",
      h264: "Файл открывается в любом плеере: «Кино и ТВ», Windows Media Player, QuickTime, телефоны, телевизоры. YouTube отдаёт H.264 максимум в 1080p.",
      av1: "Самые маленькие файлы, до 4K и выше. Windows: нужно бесплатное «AV1 Video Extension» из Microsoft Store; macOS QuickTime — только на Mac с M3 и новее. Без ограничений: VLC, mpv, браузеры.",
      vp9: "До 4K и выше. Windows: нужны «VP9 Video Extensions» (обычно уже установлены); QuickTime на macOS не откроет. Без ограничений: VLC, IINA, mpv, браузеры.",
    },
  },
  settings: {
    proxy: "Прокси",
    proxyHint:
      "Для yt-dlp, превью и скачивания компонентов. Пусто — системные настройки. Примеры: http://127.0.0.1:8888, socks5://127.0.0.1:1080",
    checkProxy: "Проверить",
    proxyOk: "YouTube доступен через прокси · {ms} мс",
    directOk: "YouTube доступен без прокси · {ms} мс",
    proxyFailure: {
      proxy_unreachable:
        "Не удаётся подключиться к прокси. Проверьте адрес, порт и что прокси запущен.",
      proxy_auth:
        "Прокси требует логин и пароль: http://логин:пароль@хост:порт",
      proxy_refused:
        "Прокси отказался соединять с YouTube. Возможно, он не поддерживает HTTPS (CONNECT) или блокирует YouTube.",
      unreachable:
        "YouTube недоступен без прокси. Укажите прокси или включите VPN.",
      timeout:
        "Нет ответа 15 секунд. Прокси или YouTube слишком медленные или заблокированы.",
      http_status: "Неожиданный ответ: {detail}",
      unknown: "Проверка не удалась: {detail}",
    },
    copyDiagnostics: "Скопировать диагностику",
    downloadDir: "Папка загрузок",
    choose: "Выбрать…",
    defaultFormat: "Формат по умолчанию",
    defaultQuality: "Качество по умолчанию",
    defaultCodec: "Кодек по умолчанию",
    concurrency: "Одновременных загрузок",
    filenameTemplate: "Шаблон имени файла",
    filenameHint:
      "Шаблон yt-dlp, например %(title)s [%(id)s].%(ext)s. Перед .%(ext)s добавляется качество видео: 720p, best h264.",
    language: "Язык",
    languageSystem: "Системный",
    autoUpdate: "Проверять обновления yt-dlp при запуске",
    components: "Компоненты",
    checkUpdates: "Проверить обновления",
    checking: "Проверяем…",
    update: "Обновить до {version}",
    upToDate: "Актуальная версия",
    reinstall: "Переустановить",
    notInstalled: "Не установлен",
    openLogs: "Открыть папку логов",
    saved: "Сохранено",
  },
  formats: {
    video: "Видео (MP4)",
    audio_mp3: "Звук (MP3)",
    audio_m4a: "Звук (M4A)",
  },
  close: {
    title: "Идут загрузки",
    text: {
      one: "Если закрыть сейчас, {count} загрузка будет остановлена, а недокачанные файлы удалены.",
      few: "Если закрыть сейчас, {count} загрузки будут остановлены, а недокачанные файлы удалены.",
      many: "Если закрыть сейчас, {count} загрузок будут остановлены, а недокачанные файлы удалены.",
      other:
        "Если закрыть сейчас, {count} загрузки будут остановлены, а недокачанные файлы удалены.",
    },
    quit: "Остановить и выйти",
    stay: "Продолжить загрузку",
  },
  errors: {
    invalid_proxy:
      "Неверный адрес прокси. Нужен URL вида http://127.0.0.1:8888 или socks5://host:1080.",
    copyDetails: "Скопировать подробности",
    copied: "Подробности скопированы. Вставьте их в сообщение разработчику.",
    file_missing: "Файл перемещён или удалён.",
    unknown: "Что-то пошло не так",
    network: "Нет связи с сервером. Проверьте подключение к интернету.",
    rate_limited: "GitHub ограничил запросы. Попробуйте через час.",
    checksum:
      "Скачанный файл повреждён (не совпала контрольная сумма). Попробуйте позже.",
    unsupported_platform: "Эта система или процессор не поддерживаются.",
    components_missing: "Нужные компоненты ещё не установлены.",
    ytdlp_unavailable: "Видео недоступно: удалено, скрыто или заблокировано.",
    ytdlp_bot_check:
      "YouTube попросил подтвердить, что вы не бот. Так бывает в некоторых сетях (VPN, хостинг): попробуйте позже или из другой сети.",
    ytdlp_age_restricted:
      "У видео возрастное ограничение, нужен вход в аккаунт.",
    ytdlp_unsupported: "Эта ссылка не поддерживается.",
    ytdlp_network: "Ошибка сети во время загрузки. Проверьте подключение.",
    ytdlp_format_unavailable: "Выбранное качество недоступно для этого видео.",
    ytdlp_timeout:
      "YouTube не ответил. Возможно, он заблокирован или замедлен в вашей сети: проверьте VPN или укажите прокси в настройках и попробуйте снова. Подробности: {message}",
    ytdlp_ffmpeg:
      "Ошибка обработки в ffmpeg. Попробуйте переустановить компоненты.",
  },
};
