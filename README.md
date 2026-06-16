# Chadow MC Launcher

Десктоп-лаунчер Minecraft Java Edition в стиле [chadow.ru](https://chadow.ru).

## Возможности

- Конфигурация с `https://test.chadow.ru/api/minecraft/bootstrap` (prod: `https://chadow.ru/...`)
- Скачивание Java (Adoptium) и клиента Minecraft
- Пинг сервера (онлайн, задержка)
- Offline-вход по нику
- UI в стиле chadow.ru

## Требования

- Node.js 18+
- Rust 1.70+
- Windows 10/11

## Разработка

```bash
npm install
npm run tauri dev
```

## Сборка

```bash
npm run tauri build
```

`.exe` / `.msi` появятся в `src-tauri/target/release/bundle/`.

## Данные

Игра и профиль: `%APPDATA%\ChadowGamesLauncher\`

## Настройка сервера

В админке chadow.ru: **Minecraft сервер** (`/admin/minecraft`).
