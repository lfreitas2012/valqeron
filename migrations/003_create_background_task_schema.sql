CREATE TABLE IF NOT EXISTS background_task
(
    id               BLOB PRIMARY KEY,
    kind             TEXT    NOT NULL CHECK (length(kind) > 0),
    status           TEXT    NOT NULL DEFAULT 'PENDING' CHECK (status IN ('PENDING', 'RUNNING', 'SUCCEEDED', 'FAILED')),
    payload          TEXT,
    scheduled_at     TEXT    NOT NULL,
    started_at       TEXT,
    finished_at      TEXT,
    attempts         INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    max_attempts     INTEGER NOT NULL DEFAULT 1 CHECK (max_attempts >= 1),
    retry_delay_secs INTEGER NOT NULL DEFAULT 0 CHECK (retry_delay_secs >= 0),
    last_error       TEXT,
    created_at       TEXT    NOT NULL,
    updated_at       TEXT    NOT NULL,
    version          INTEGER NOT NULL DEFAULT 1
) STRICT, WITHOUT ROWID;

CREATE INDEX IF NOT EXISTS idx_background_task_due ON background_task (status, scheduled_at);

CREATE INDEX IF NOT EXISTS idx_background_task_kind ON background_task (kind, scheduled_at);
