-- v10: diagramas Mermaid por proyecto (RF-MOD-19, D21) y sus citas. Igual que
-- la v18 de SQLite.

ALTER TABLE project ADD COLUMN next_diagram BIGINT NOT NULL DEFAULT 1;

CREATE TABLE diagram (
    id          TEXT COLLATE "C" PRIMARY KEY,
    project_id  TEXT COLLATE "C" NOT NULL REFERENCES project(id) ON DELETE CASCADE,
    number      BIGINT NOT NULL,
    title       TEXT COLLATE "C" NOT NULL,
    description TEXT COLLATE "C" NOT NULL,
    source      TEXT COLLATE "C" NOT NULL,
    created_at  TEXT COLLATE "C" NOT NULL,
    updated_at  TEXT COLLATE "C" NOT NULL,
    UNIQUE (project_id, number)
);

CREATE TABLE diagram_mention (
    source_id   TEXT COLLATE "C" NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    comment_id  TEXT COLLATE "C" REFERENCES comment(id) ON DELETE CASCADE,
    target_id   TEXT COLLATE "C" NOT NULL REFERENCES diagram(id) ON DELETE CASCADE
);
CREATE INDEX diagram_mention_target ON diagram_mention(target_id);
CREATE INDEX diagram_mention_source ON diagram_mention(source_id, comment_id);
