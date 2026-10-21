# Cliprigo

Простое кроссплатформенное приложение для скачивания видео с YouTube с историей загрузок.
Работает поверх [yt-dlp](https://github.com/yt-dlp/yt-dlp). Само скачивает и обновляет yt-dlp, ffmpeg и Deno, ставить ничего вручную не нужно.

Стек: Tauri 2 + Svelte 5 + SQLite. Подробности — в [PRD.ru.md](PRD.ru.md), план — в [TASKS.ru.md](TASKS.ru.md).

## Разработка

Требуется Node.js 22+, pnpm 10+, Rust stable и [системные зависимости Tauri](https://v2.tauri.app/start/prerequisites/).

```sh
pnpm install
pnpm tauri dev      # запуск в режиме разработки
pnpm tauri build    # сборка установщика
pnpm lint           # prettier + svelte-check + vitest
pnpm ui:preview     # UI-сценарии в headless Chrome со скриншотами (нужен Google Chrome)
```

Rust-тесты: `cargo test` в `src-tauri`. Сквозные тесты с настоящим YouTube запускаются отдельно:
`cargo test live_ -- --ignored --test-threads=1`.

Если в окружении задан `HTTP_PROXY`, добавьте `NO_PROXY=localhost,127.0.0.1`, иначе окно
`tauri dev` будет запрашивать dev-сервер через прокси.

## Запуск неподписанной сборки

Релизы пока не подписаны, поэтому при первом запуске система покажет предупреждение.

- **Windows:** «Windows защитила ваш компьютер» → «Подробнее» → «Выполнить в любом случае».
- **macOS:** «Системные настройки» → «Конфиденциальность и безопасность» → «Открыть всё равно».
  Или в терминале: `xattr -dr com.apple.quarantine /Applications/Cliprigo.app`.
- **Linux:** ничего делать не нужно.

## Дисклеймер

Cliprigo не содержит yt-dlp, ffmpeg и Deno, а скачивает их из официальных релизов на GitHub.
Скачивайте только тот контент, на который у вас есть права, и соблюдайте условия использования сервисов.

## Лицензия

MIT
