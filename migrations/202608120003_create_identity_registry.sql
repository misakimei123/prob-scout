-- HIST-002 在 DATA-006 snapshot alias 之外增加带时间范围的历史身份；允许真实歧义入库，由解析器 fail closed。
CREATE TABLE canonical_teams (
    canonical_team_id TEXT PRIMARY KEY CHECK (canonical_team_id LIKE 'lol-team:%'),
    canonical_name TEXT NOT NULL CHECK (length(trim(canonical_name)) > 0),
    created_at_utc TEXT NOT NULL
);

CREATE TABLE team_identity_periods (
    id INTEGER PRIMARY KEY,
    canonical_team_id TEXT NOT NULL REFERENCES canonical_teams(canonical_team_id) ON DELETE RESTRICT,
    source_name TEXT NOT NULL,
    source_team_id TEXT,
    observed_name TEXT NOT NULL,
    normalized_name TEXT NOT NULL,
    valid_from_utc TEXT NOT NULL,
    valid_until_utc TEXT,
    evidence_ref TEXT NOT NULL,
    CHECK (source_team_id IS NULL OR length(trim(source_team_id)) > 0),
    CHECK (length(trim(observed_name)) > 0),
    CHECK (length(trim(normalized_name)) > 0),
    CHECK (length(trim(evidence_ref)) > 0),
    CHECK (valid_until_utc IS NULL OR valid_until_utc > valid_from_utc),
    UNIQUE (canonical_team_id, source_name, observed_name, valid_from_utc)
);

CREATE INDEX team_identity_periods_source_id_lookup
ON team_identity_periods(source_name, source_team_id, valid_from_utc, valid_until_utc)
WHERE source_team_id IS NOT NULL;

CREATE INDEX team_identity_periods_name_lookup
ON team_identity_periods(source_name, normalized_name, valid_from_utc, valid_until_utc);

CREATE TABLE canonical_competitions (
    canonical_competition_id TEXT PRIMARY KEY CHECK (canonical_competition_id LIKE 'lol-competition:%'),
    canonical_name TEXT NOT NULL CHECK (length(trim(canonical_name)) > 0),
    created_at_utc TEXT NOT NULL
);

CREATE TABLE competition_identity_periods (
    id INTEGER PRIMARY KEY,
    canonical_competition_id TEXT NOT NULL REFERENCES canonical_competitions(canonical_competition_id) ON DELETE RESTRICT,
    source_name TEXT NOT NULL,
    source_competition_id TEXT,
    observed_name TEXT NOT NULL,
    normalized_name TEXT NOT NULL,
    valid_from_utc TEXT NOT NULL,
    valid_until_utc TEXT,
    evidence_ref TEXT NOT NULL,
    CHECK (source_competition_id IS NULL OR length(trim(source_competition_id)) > 0),
    CHECK (length(trim(observed_name)) > 0),
    CHECK (length(trim(normalized_name)) > 0),
    CHECK (length(trim(evidence_ref)) > 0),
    CHECK (valid_until_utc IS NULL OR valid_until_utc > valid_from_utc),
    UNIQUE (canonical_competition_id, source_name, observed_name, valid_from_utc)
);

CREATE INDEX competition_identity_periods_source_id_lookup
ON competition_identity_periods(source_name, source_competition_id, valid_from_utc, valid_until_utc)
WHERE source_competition_id IS NOT NULL;

CREATE INDEX competition_identity_periods_name_lookup
ON competition_identity_periods(source_name, normalized_name, valid_from_utc, valid_until_utc);
