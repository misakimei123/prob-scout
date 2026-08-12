-- 保存原始数据的来源与完整性信息，后续 processed dataset 可据此追溯输入。
CREATE TABLE source_records (
    id INTEGER PRIMARY KEY,
    source_name TEXT NOT NULL,
    external_id TEXT NOT NULL,
    captured_at TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    raw_path TEXT NOT NULL,
    UNIQUE (source_name, external_id, captured_at)
);
