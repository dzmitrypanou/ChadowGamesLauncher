# Chadow Games Launcher

Десктоп-лаунчер Minecraft Java Edition для проекта [CHADOW](https://chadow.ru). 

Собран на Tauri 2 (Rust + Vite), UI в фирменном стиле chadow.ru.

**Версия:** 0.6.8 Obsidian

## Возможности

- Загрузка конфигурации с bootstrap API (`https://chadow.ru/api/minecraft/bootstrap`)
- Автоматическая установка Java (Adoptium) и клиента Minecraft с Fabric
- Обновление клиентского пака по SHA256 с сервера
- Список игровых серверов с пингом (онлайн, задержка)
- Пробуждение спящего сервера перед подключением
- Offline-вход по нику
- Настройки: RAM, путь установки, режим окна, dev-режим
- Клиентский Fabric-мод с ограничениями UI (без главного меню после выхода из мира)

## Структура проекта

```
├── src/                  # Frontend (Vite, vanilla JS)
├── src-tauri/            # Backend (Rust, Tauri)
├── client-mod/           # Fabric-мод для клиента Minecraft
├── config/               # Версия лаунчера (version.json)
├── scripts/              # Сборка архивов и синхронизация
├── tools/build-client-zip/  # Утилита на Rust для полной загрузки клиента
└── reference-client/     # Локальная копия клиента (не в git, см. ниже)
```

## Требования

| Компонент | Версия |
|-----------|--------|
| Node.js | 18+ |
| Rust | 1.70+ |
| Windows | 10/11 |
| JDK | 21+ (для сборки мода) |

## Разработка

```bash
npm install
npm run tauri dev
```

Откроется окно лаунчера с hot-reload фронтенда на `http://localhost:5173`.

## Сборка лаунчера

```bash
npm run tauri build
```

Готовые установщики появятся в `src-tauri/target/release/bundle/`:

- `Chadow Games Launcher_x.x.x_x64-setup.exe` — NSIS-инсталлер
- `Chadow Games Launcher_x.x.x_x64_en-US.msi` — MSI

## Fabric-мод

Мод `chadow-games-client` (Minecraft 1.21.11, Fabric Loader 0.18.2) ограничивает доступ к главному меню, мультиплееру и Realms после выхода из мира.

```bash
cd client-mod
.\gradlew.bat build
```

JAR: `client-mod/build/libs/chadow-games-client-*.jar`

## Скрипты сборки клиента

Все скрипты запускаются из корня репозитория.

### Синхронизация reference-client

Копирует установленный клиент из `%APPDATA%\ChadowGamesLauncher` в `reference-client/` для локальной сборки архивов. Папка не хранится в git (~500 МБ).

```powershell
.\scripts\sync-reference-client.ps1
```

Опции: `-Version 1.21.11`, `-SkipModBuild`, `-SourceRoot "путь"`.

### Полный ZIP клиента (для загрузки на сервер)

```powershell
# Из локальной установки или reference-client
.\scripts\build-full-client-archive.ps1

# Vanilla (без мода и Fabric)
.\scripts\build-full-client-archive.ps1 -Vanilla

# Скачать с Mojang и собрать ZIP (нужен Rust)
.\scripts\build-full-client-archive.ps1 -Download
```

Результат: `dist/minecraft-1.21.11-client.zip`

### ZIP только с модом (client pack overlay)

```powershell
.\scripts\build-client-pack.ps1 -Version 1.0.17
```

Результат: `dist/chadow-client-pack-1.0.17.zip`

### Иконки инсталлера

```powershell
.\scripts\generate-installer-assets.ps1
```

Генерирует `installer-header.bmp` и `installer-sidebar.bmp` из логотипа.

## Данные лаунчера

| Что | Путь |
|-----|------|
| Профиль, кэш bootstrap | `%APPDATA%\ChadowGamesLauncher\` |
| Minecraft (jar, libs, assets) | `%APPDATA%\ChadowGamesLauncher\` |
| Java (Adoptium) | `%APPDATA%\ChadowGamesLauncher\runtime\` |

Профиль (`profile.json`) содержит ник, URL API, выбранные серверы, пути установки и настройки отображения.

## Настройка на сервере

В админке chadow.ru: **Minecraft сервер** → `/admin/minecraft`

Bootstrap API отдаёт:

- версию Minecraft и Java
- URL и SHA256 клиентского пака
- список серверов (адрес, порт, иконка, MOTD)
- параметры пробуждения сервера

После сборки ZIP загрузите архив в админку и укажите хеш в bootstrap.

## Стек

- **Frontend:** Vite 6, vanilla JS, CSS
- **Backend:** Rust, Tauri 2, reqwest
- **Mod:** Fabric Loom, Java 21, Mixins
- **Installer:** NSIS (Windows)
