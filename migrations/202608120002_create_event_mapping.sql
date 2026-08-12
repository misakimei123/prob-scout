-- DATA-006 只保存显式来源证据；不在数据库层猜测队名或把不同语义的时间合并。
CREATE TABLE events (
    event_id TEXT PRIMARY KEY,
    game TEXT NOT NULL,
    competition TEXT NOT NULL,
    best_of INTEGER NOT NULL CHECK (best_of IN (1, 3, 5)),
    canonical_team_a_id TEXT NOT NULL,
    canonical_team_b_id TEXT NOT NULL,
    CHECK (canonical_team_a_id <> canonical_team_b_id)
);

CREATE TABLE event_aliases (
    id INTEGER PRIMARY KEY,
    event_id TEXT NOT NULL REFERENCES events(event_id) ON DELETE CASCADE,
    source_name TEXT NOT NULL,
    source_event_id TEXT NOT NULL,
    observed_team_a_name TEXT NOT NULL,
    observed_team_b_name TEXT NOT NULL,
    observed_time_utc TEXT NOT NULL,
    observed_time_kind TEXT NOT NULL CHECK (
        observed_time_kind IN ('scheduled_start', 'market_end', 'game_start')
    ),
    UNIQUE (source_name, source_event_id)
);

CREATE TABLE team_aliases (
    id INTEGER PRIMARY KEY,
    canonical_team_id TEXT NOT NULL,
    source_name TEXT NOT NULL,
    source_team_id TEXT,
    observed_name TEXT NOT NULL,
    normalized_name TEXT NOT NULL,
    UNIQUE (canonical_team_id, source_name, observed_name)
);

CREATE UNIQUE INDEX team_aliases_source_id_unique
ON team_aliases(source_name, source_team_id)
WHERE source_team_id IS NOT NULL;

CREATE TABLE market_mappings (
    event_id TEXT PRIMARY KEY REFERENCES events(event_id) ON DELETE CASCADE,
    polymarket_event_id TEXT NOT NULL UNIQUE,
    market_id TEXT NOT NULL UNIQUE,
    condition_id TEXT NOT NULL UNIQUE,
    gamma_end_date_utc TEXT NOT NULL,
    clob_game_start_time_utc TEXT NOT NULL,
    outcome_0_team_id TEXT NOT NULL,
    outcome_0_name TEXT NOT NULL,
    outcome_0_token_id TEXT NOT NULL UNIQUE,
    outcome_1_team_id TEXT NOT NULL,
    outcome_1_name TEXT NOT NULL,
    outcome_1_token_id TEXT NOT NULL UNIQUE,
    CHECK (outcome_0_team_id <> outcome_1_team_id),
    CHECK (outcome_0_token_id <> outcome_1_token_id)
);
