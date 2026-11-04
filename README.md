# Диск-Мастер

Десктопное приложение для Windows 11: очистка диска, анализ занятого места,
управление программами и автозагрузкой, мониторинг системы и виджет на рабочем столе.

Переписано с Electron на **Tauri 2**: фронтенд остался на React + Tailwind, бэкенд —
нативный Rust, интерфейс рендерится системным **WebView2**. Итог: ~10 МБ вместо ~120 МБ,
старт ~0,25 с, нет локального HTTP-сервера.

## Требования

- Windows 11 (WebView2 входит в систему)
- Для сборки: Node 20+, pnpm, Rust (rustup + MSVC Build Tools)

## Разработка

```bash
pnpm install
pnpm dev          # tauri dev: Vite + Rust, горячая перезагрузка UI
```

Полезные команды:

| Команда | Что делает |
|---|---|
| `pnpm dev` | запуск приложения в dev-режиме |
| `pnpm build:web` | только интерфейс в `dist/client` |
| `pnpm build:portable` | release-сборка + папка `output/DiskMaster-portable-<версия>/` |
| `pnpm build:installer` | NSIS-установщик + артефакты автообновления |
| `pnpm check:types` | проверка TypeScript |
| `cargo test` (в `src-tauri`) | тесты guard-слоя и парсеров |
| `pnpm release` | публикация релиза на GitHub (см. ниже) |

## Структура

```
src/client/        React + Tailwind UI, вкладки и компоненты
src/client/api.ts  типизированный мост к Rust-командам (вместо oRPC)
src/shared/        общий контракт данных UI ↔ Rust
src-tauri/         Rust-бэкенд, конфиг Tauri, скрипты PowerShell
  src/commands.rs  #[tauri::command] — вся поверхность IPC
  src/guard.rs     защита от удаления системных путей + allow-list
  src/metrics.rs   сэмплер CPU/RAM/диска/сети (один PowerShell-процесс)
  src/scan.rs      категории мусора, большие файлы, очистка
  src/folders.rs   дерево папок
  src/duplicates.rs поиск дубликатов по SHA-256
  src/programs.rs  реестр установленных программ
  src/startup.rs   автозагрузка
  src/schedule.rs  Планировщик заданий Windows
  src/widget.rs    WPF-виджет (отдельный процесс PowerShell)
scripts/           сборка, портативная версия, релиз, версия
src-tauri/scripts/ metrics.ps1, monitor.ps1, startup.ps1
```

## Данные

Установленная версия хранит данные в `%LOCALAPPDATA%\DiskMaster`
(история места, настройки виджета, журнал расписания, состояние окна).

Портативная версия — в папке `disk-master-data` рядом с `DiskMaster.exe`
(наличие файла `portable.marker` включает этот режим). Чтобы перенести приложение,
скопируйте всю папку.

## Сборка для пользователей

- `output/DiskMaster-portable-<версия>/` — распаковать и запустить, без установки.
- NSIS-установщик — выбор папки, установка для текущего пользователя без UAC.

## Автообновление

Обновления публикуются в GitHub Releases репозитория `xxxronelxxx/diskmaster`.
Приложение проверяет подписанный `latest.json` при запуске и предлагает обновление;
установка начинается только по кнопке. Артефакты подписаны ключом minisign
(`~/.tauri/diskmaster.key`, в репозиторий не попадает), поэтому подменить обновление
без приватного ключа нельзя. Портативная версия обновляется вручную.

Первый релиз:

```bash
pnpm tauri signer generate -w ~/.tauri/diskmaster.key   # один раз, ключ нужно сохранить
pnpm release
```

## Безопасность

- нет HTTP-сервера и открытого порта — только IPC;
- CSP и запрет навигации на внешние адреса;
- удаление только по allow-list путей из результатов скана/анализа;
- защищённые системные каталоги, корни дисков и симлинки блокируются;
- настройки виджета валидируются по диапазонам.
