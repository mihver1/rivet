# Shelly — SSH Connection Manager for macOS

## Overview

Shelly — нативное macOS-приложение для хранения, управления и использования SSH-соединений. Референс: xpipe. Ключевые отличия от xpipe: нативный macOS UI (SwiftUI), Rust-ядро вместо Java, Cisco IOS-style CLI, встроенный MCP-сервер (фаза 2), собственный шифрованный vault.

## Scope

### MVP (Phase 1)
- Rust-демон (shellyd) с Unix socket IPC
- Шифрованный vault (директория с отдельными файлами)
- SSH-движок (russh) для программных операций
- CLI с Cisco IOS-style prefix matching
- Базовый SwiftUI GUI
- SCP через russh
- Импорт из ~/.ssh/config

### Phase 2 (отложено)
- SSH-тоннели (local, remote, dynamic)
- Групповые операции и оркестрация (workflows)
- MCP-сервер
- Script engine с шаблонами и переменными

---

## Architecture

### Компоненты

```
┌─────────────────────────────────────────────────────┐
│                     КЛИЕНТЫ                          │
│  ┌─────────────┐ ┌─────────────┐ ┌───────────────┐  │
│  │ SwiftUI App │ │ CLI (shelly)│ │ MCP (фаза 2)  │  │
│  └──────┬──────┘ └──────┬──────┘ └───────┬───────┘  │
│         └───────────────┼────────────────┘           │
│              Unix Socket + JSON-RPC 2.0              │
├─────────────────────────┼───────────────────────────┤
│              ┌──────────┴──────────┐                 │
│              │   ДЕМОН (shellyd)   │                 │
│              │       Rust          │                 │
│              └──────────┬──────────┘                 │
│    ┌────────────┬───────┴───────┬─────────────┐      │
│    ▼            ▼               ▼             ▼      │
│ Connection   SSH Engine     Vault Mgr    Task Runner │
│ Manager      (russh)        (AES-256)    (фаза 2)   │
└─────────────────────────────────────────────────────┘
```

### Стек технологий

| Компонент | Технология |
|-----------|-----------|
| Ядро/демон | Rust, tokio |
| SSH | russh |
| IPC | JSON-RPC 2.0 over Unix socket (jsonrpsee) |
| CLI framework | clap + custom prefix matcher |
| Vault encryption | AES-256-GCM + Argon2id |
| GUI | Swift, SwiftUI (macOS native) |
| Сериализация | serde + serde_json |
| Логирование | tracing |

### Rust↔Swift взаимодействие

Чистое разделение через Unix socket, никакого FFI. SwiftUI-приложение — клиент демона, как и CLI. Протокол — JSON-RPC 2.0.

---

## Project Structure

```
shelly/
├── Cargo.toml                    # Workspace root
├── crates/
│   ├── shelly-core/              # Общие типы, модели, протокол
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── connection.rs     # Connection, Group, Tag
│   │       ├── protocol.rs       # JSON-RPC request/response types
│   │       └── error.rs
│   │
│   ├── shelly-vault/             # Шифрованное хранилище
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── crypto.rs         # AES-256-GCM, Argon2id, KEK/DEK
│   │       ├── store.rs          # CRUD для vault-директории
│   │       └── models.rs
│   │
│   ├── shelly-ssh/               # SSH-движок
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── session.rs        # Управление сессиями, pool
│   │       ├── auth.rs           # Все методы аутентификации
│   │       ├── exec.rs           # Выполнение команд
│   │       ├── transfer.rs       # SCP / SFTP
│   │       └── tunnel.rs         # Тоннели (фаза 2, заглушка)
│   │
│   ├── shelly-daemon/            # Демон
│   │   └── src/
│   │       ├── main.rs
│   │       ├── server.rs         # Unix socket + JSON-RPC
│   │       ├── handlers.rs       # RPC method handlers
│   │       └── state.rs          # Runtime state
│   │
│   └── shelly-cli/               # CLI-клиент
│       └── src/
│           ├── main.rs
│           ├── client.rs         # JSON-RPC client
│           ├── prefix.rs         # Cisco IOS-style prefix matcher
│           ├── commands/
│           │   ├── mod.rs
│           │   ├── daemon.rs
│           │   ├── vault.rs
│           │   ├── conn.rs
│           │   ├── ssh.rs
│           │   ├── scp.rs
│           │   └── exec.rs
│           └── interactive.rs    # Запуск системного ssh
│
├── ShellyApp/                    # SwiftUI macOS app
│   ├── ShellyApp.xcodeproj
│   ├── Sources/
│   │   ├── ShellyApp.swift
│   │   ├── Models/
│   │   │   ├── Connection.swift
│   │   │   └── VaultStatus.swift
│   │   ├── Services/
│   │   │   └── DaemonClient.swift
│   │   └── Views/
│   │       ├── ConnectionList.swift
│   │       ├── ConnectionDetail.swift
│   │       ├── AddConnection.swift
│   │       └── VaultUnlock.swift
│   └── Resources/
│
├── docs/
└── tests/
```

### Зависимости между crate'ами

```
shelly-daemon → shelly-core, shelly-vault, shelly-ssh
shelly-ssh    → shelly-core
shelly-cli    → shelly-core
```

---

## Vault

### Формат: директория с отдельными зашифрованными файлами

```
~/.shelly/vault/
├── vault.toml              # Не зашифрован: версия, параметры Argon2id
├── master.key              # DEK, зашифрованный KEK'ом
├── connections/
│   ├── <uuid>.enc          # Каждое соединение — отдельный файл
│   └── ...
├── groups/
│   ├── <uuid>.enc
│   └── ...
├── keys/
│   ├── <uuid>.enc          # SSH-ключи (если хранятся в vault)
│   └── ...
└── settings.enc            # Настройки
```

### Двухуровневая схема ключей (KEK/DEK)

1. **KEK (Key Encryption Key)** = `Argon2id(master_password, salt)` — параметры: m=64MB, t=3, p=4
2. **DEK (Data Encryption Key)** — случайный 256-bit ключ, зашифрован KEK'ом и хранится в `master.key`
3. Все `.enc` файлы зашифрованы DEK'ом через **AES-256-GCM**

Формат `.enc` файла:
```
[nonce: 12 bytes][ciphertext: variable][auth_tag: 16 bytes]
```

Plaintext внутри каждого `.enc` — JSON.

### Операции

- **Unlock**: password → Argon2id → KEK → decrypt master.key → DEK в памяти
- **Lock**: zeroize DEK из памяти
- **Смена пароля**: new_password → new_KEK → re-encrypt DEK → write master.key. Файлы данных не трогаются.
- **Сохранение соединения**: JSON → AES-256-GCM(DEK, new_nonce) → `connections/<uuid>.enc`. Остальные файлы не трогаются.
- **Auto-lock**: vault блокируется после настраиваемого таймаута неактивности

### Принципы безопасности

- Plaintext никогда не записывается на диск
- DEK zeroize при lock
- Новый nonce при каждом сохранении файла
- Argon2id с высокими параметрами для защиты от brute-force

---

## Data Models

### Connection

```rust
struct Connection {
    id:          Uuid,
    name:        String,            // уникальное имя, "production-web-1"
    host:        String,            // IP или hostname
    port:        u16,               // default: 22
    username:    String,
    auth:        AuthMethod,
    tags:        Vec<String>,
    group_ids:   Vec<Uuid>,
    jump_host:   Option<Uuid>,      // ProxyJump через другой Connection
    options:     SshOptions,        // keepalive, compression, etc.
    notes:       Option<String>,
    created_at:  DateTime<Utc>,
    updated_at:  DateTime<Utc>,
}
```

### AuthMethod

```rust
enum AuthMethod {
    Password(String),
    PrivateKey {
        key_data: Vec<u8>,          // ключ хранится В vault
        passphrase: Option<String>,
    },
    KeyFile {
        path: PathBuf,              // ссылка на файл в ~/.ssh/
        passphrase: Option<String>,
    },
    Agent,                          // делегировать ssh-agent
    Certificate {
        cert_path: PathBuf,
        key_path: PathBuf,
    },
    Interactive,                    // keyboard-interactive (2FA)
}
```

### Group

```rust
struct Group {
    id:          Uuid,
    name:        String,
    description: Option<String>,
    color:       Option<String>,    // для GUI
}
```

### SshOptions

```rust
struct SshOptions {
    keepalive_interval: Option<u32>,    // секунды
    keepalive_count_max: Option<u32>,
    compression: bool,
    connect_timeout: Option<u32>,       // секунды
    extra_args: Vec<String>,            // для системного ssh
}
```

---

## Daemon (shellyd)

### Жизненный цикл

1. **Запуск**: `shelly daemon start` или автоматически при первом обращении CLI/GUI
2. **Инициализация**: создать Unix socket (`~/.shelly/shelly.sock`), загрузить `vault.toml`
3. **Ожидание unlock**: vault заблокирован, принимает только `vault.unlock`, `vault.status`, `daemon.status`
4. **Работа**: полный набор RPC-методов
5. **Остановка**: `shelly daemon stop` или SIGTERM — graceful shutdown, закрытие сессий

Файлы демона:
- `~/.shelly/shelly.sock` — Unix socket
- `~/.shelly/shellyd.pid` — PID file
- `~/.shelly/config.toml` — глобальный конфиг (не шифрован: auto-lock timeout, log level, etc.)
- `~/.shelly/logs/` — лог-файлы

Опционально: launchd plist для автозапуска.

### Runtime State

```rust
struct DaemonState {
    vault: Option<UnlockedVault>,           // None = locked
    sessions: HashMap<SessionId, SshSession>, // активные SSH-сессии
    clients: Vec<ClientConnection>,         // подключённые клиенты (GUI, CLI)
}
```

- **Connection pooling**: переиспользование SSH-сессий к одному хосту
- **Keepalive**: каждые 30 секунд по умолчанию
- **Auto-reconnect**: при обрыве соединения

---

## IPC Protocol — JSON-RPC 2.0

### Транспорт

Unix socket: `~/.shelly/shelly.sock`. Каждое JSON-RPC сообщение — newline-delimited JSON.

### MVP методы

| Метод | Params | Result | Описание |
|-------|--------|--------|----------|
| `vault.unlock` | `{password}` | `{ok}` | Разблокировать vault |
| `vault.lock` | — | `{ok}` | Заблокировать vault |
| `vault.status` | — | `{locked: bool}` | Статус vault |
| `vault.init` | `{password}` | `{ok}` | Инициализация нового vault |
| `vault.change_password` | `{old, new}` | `{ok}` | Смена мастер-пароля |
| `conn.list` | `{tag?, group_id?}` | `[Connection]` | Список соединений |
| `conn.get` | `{id \| name}` | `Connection` | Получить по ID или имени |
| `conn.create` | `{Connection}` | `{id}` | Создать соединение |
| `conn.update` | `{id, fields}` | `{ok}` | Обновить соединение |
| `conn.delete` | `{id \| name}` | `{ok}` | Удалить |
| `conn.import_ssh_config` | `{path?}` | `{imported: u32}` | Импорт из SSH config |
| `ssh.exec` | `{conn_id, command}` | `{exit_code, stdout, stderr}` | Выполнить команду |
| `ssh.connect_info` | `{conn_id}` | `{host, port, user, key_path?, ...}` | Параметры для системного ssh |
| `scp.upload` | `{conn_id, local, remote}` | `{ok, bytes}` | Загрузить файл |
| `scp.download` | `{conn_id, remote, local}` | `{ok, bytes}` | Скачать файл |
| `daemon.status` | — | `{uptime, sessions, vault_locked}` | Статус демона |

### Пример обмена

```json
→ {"jsonrpc":"2.0", "method":"ssh.exec", "id":1,
   "params":{"connection_id":"abc123", "command":"uptime"}}

← {"jsonrpc":"2.0", "id":1,
   "result":{"exit_code":0, "stdout":"up 42 days", "stderr":""}}
```

### Ошибки

Стандартные JSON-RPC error codes + кастомные:
- `-32001` — vault locked
- `-32002` — connection not found
- `-32003` — SSH auth failed
- `-32004` — SSH connection failed
- `-32005` — SCP transfer failed

---

## SSH Engine

### Гибридный режим

**Программные операции (через russh, внутри демона):**
- `ssh.exec` — выполнение команд, сбор stdout/stderr
- `scp.upload` / `scp.download` — передача файлов
- Тоннели (фаза 2)

**Интерактивные сессии (через системный ssh, из CLI):**
- `shelly ssh <name>` → CLI получает параметры от демона → `exec("ssh", args)`
- Полный TTY, поддержка tmux/screen, escape sequences

### Аутентификация

Поддерживаемые методы:
1. **Password** — пароль из vault
2. **PrivateKey** — ключ хранится в vault, записывается во временный файл (0600, удаляется после использования) или передаётся через russh напрямую
3. **KeyFile** — ссылка на файл в файловой системе (`~/.ssh/id_ed25519`)
4. **Agent** — делегирование ssh-agent
5. **Certificate** — SSH-сертификаты
6. **Interactive** — keyboard-interactive (2FA/TOTP)

### Connection pooling

- Демон держит HashMap активных SSH-сессий
- При повторном `ssh.exec` к тому же хосту — переиспользует сессию
- Keepalive каждые 30 секунд (настраиваемо)
- Сессия закрывается при таймауте неактивности или ошибке

---

## CLI — shelly

### Cisco IOS-style prefix matching

Команды парсятся по кратчайшему однозначному префиксу:

```
shelly vault unlock    →  shelly v u    →  v u
shelly conn list       →  shelly co l   →  co l
shelly ssh <name>      →  shelly ss <n> →  ss <n>
shelly exec <n> "cmd"  →  shelly e <n>  →  e <n>
```

Алгоритм:
1. Разбить ввод на токены
2. Для каждого токена — найти команды, начинающиеся с него
3. Ровно одно совпадение → раскрыть
4. Несколько → `Ambiguous command`, показать варианты
5. Ноль → `Unknown command`, предложить ближайшие (Levenshtein distance)

`?` после токена показывает доступные подкоманды (как в IOS).

### Дерево команд

```
shelly
├── daemon
│   ├── start
│   ├── stop
│   └── status
├── vault
│   ├── unlock
│   ├── lock
│   ├── init
│   └── change-password
├── conn
│   ├── list [--tag TAG] [--group GROUP]
│   ├── add (интерактивный)
│   ├── edit <name>
│   ├── rm <name>
│   ├── show <name>
│   └── import [--path PATH]
├── ssh <name> [-- command...]
├── scp <src> <dst>
└── exec <name|--group GROUP> <command>
```

### Поведение CLI

- При первом вызове: если демон не запущен — запустить автоматически
- При обращении к vault-зависимым командам: если vault locked — запросить пароль
- Вывод: таблицы для `conn list`, JSON с `--json` флагом для скриптов
- Exit codes: 0 = ok, 1 = error, 2 = vault locked, 3 = connection failed

---

## SwiftUI GUI

### Экраны (MVP)

1. **Vault Unlock** — ввод мастер-пароля при запуске
2. **Connection List** — sidebar с группами/тегами + detail panel
3. **Connection Detail** — информация о соединении, быстрые действия
4. **Add/Edit Connection** — форма с полями Connection model
5. **Daemon Status** — статус демона, активные сессии (в menu bar)

### Паттерн UI

macOS NavigationSplitView:
- **Sidebar**: группы, теги, поиск
- **Detail**: информация о выбранном соединении

### Действия из GUI

- **Open in Terminal** — запускает Terminal.app с `shelly ssh <name>`
- **Quick Command** — текстовое поле, выполнение через `ssh.exec`, вывод результата
- **Upload File** — drag & drop или файл-диалог → `scp.upload`
- **Edit** — редактирование параметров соединения
- **Copy SSH Command** — копировать `ssh user@host -p port` в буфер

### Коммуникация с демоном

`DaemonClient.swift` — JSON-RPC клиент через Unix socket:
- Foundation `FileHandle` или `NWConnection` для Unix socket
- `JSONEncoder`/`JSONDecoder` для сериализации
- Async/await Swift concurrency

---

## Configuration

### ~/.shelly/config.toml (не шифрован)

```toml
[daemon]
auto_start = true
log_level = "info"

[vault]
auto_lock_timeout = 900     # секунды, 0 = отключено

[ssh]
default_keepalive = 30      # секунды
default_connect_timeout = 10
connection_pool_max = 50

[cli]
default_output = "table"    # "table" | "json"
```

---

## Phase 2 (future scope)

### SSH Tunnels
- Local forwarding: `-L local:remote_host:remote_port`
- Remote forwarding: `-R remote:local_host:local_port`
- Dynamic (SOCKS5): `-D port`
- Тоннели живут в демоне, переживают закрытие GUI/CLI

### Group Operations
- Параллельное выполнение команд на группе хостов
- Сбор и агрегация результатов
- Массовая раскладка файлов

### Orchestration (Workflows)
- YAML-описание последовательности шагов
- Переменные и шаблоны
- Условное выполнение
- Прогресс и отчёты

### MCP Server
- Shelly как MCP-сервер для Claude/LLM
- Tools: list_connections, exec_command, copy_file, manage_tunnel
- Позволяет AI-агентам управлять SSH-инфраструктурой

---

## Verification

### Как проверить MVP

1. **Vault**: `shelly vault init` → `shelly vault unlock` → `shelly vault lock` — проверить шифрование/дешифрование
2. **Connection CRUD**: `shelly conn add` → `shelly conn list` → `shelly conn show` → `shelly conn edit` → `shelly conn rm`
3. **SSH exec**: `shelly exec <name> "uptime"` — выполнение команды через russh
4. **Interactive SSH**: `shelly ssh <name>` — интерактивная сессия через системный ssh
5. **SCP**: `shelly scp ./file <name>:/tmp/` → `shelly scp <name>:/tmp/file ./`
6. **Prefix matching**: `shelly v u`, `shelly co l`, `shelly ss <name>`
7. **GUI**: запустить ShellyApp → разблокировать vault → просмотр соединений → Quick Command
8. **Import**: `shelly conn import` — импорт из ~/.ssh/config
9. **Daemon lifecycle**: `shelly daemon status` → `shelly daemon stop` → автозапуск при обращении

### Тесты

- **Unit tests**: crypto (vault encrypt/decrypt roundtrip), prefix matcher, connection model
- **Integration tests**: daemon start/stop, JSON-RPC protocol, SSH operations (требуют SSH-сервер в Docker)
