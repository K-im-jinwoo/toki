CREATE TABLE conversation_summaries (
    conversation_id TEXT PRIMARY KEY,
    title TEXT NOT NULL DEFAULT '',
    status TEXT NOT NULL DEFAULT '',
    not_fully_idle INTEGER NOT NULL DEFAULT 0,
    killed INTEGER NOT NULL DEFAULT 0,
    last_modified_time TEXT NOT NULL,
    workspace_uris TEXT NOT NULL DEFAULT ''
);

INSERT INTO conversation_summaries VALUES
    ('idle', 'Idle conversation', 'CASCADE_RUN_STATUS_IDLE', 0, 0, '2027-01-15T07:59:40Z', 'file:///Users/demo/Project/toki'),
    ('running', 'Running conversation', 'CASCADE_RUN_STATUS_RUNNING', 1, 0, '2027-01-15T07:59:40Z', ''),
    ('busy', 'Busy conversation', 'CASCADE_RUN_STATUS_BUSY', 0, 0, '2027-01-15T07:59:40Z', ''),
    ('waiting', 'Waiting conversation', 'CASCADE_RUN_STATUS_WAITING_FOR_APPROVAL', 0, 0, '2027-01-15T07:59:40Z', ''),
    ('killed', 'Killed conversation', 'CASCADE_RUN_STATUS_RUNNING', 1, 1, '2027-01-15T07:59:50Z', '');
