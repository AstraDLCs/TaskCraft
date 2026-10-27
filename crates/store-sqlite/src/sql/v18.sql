-- v18: diagramas Mermaid por proyecto (RF-MOD-19, D21) y sus citas desde
-- descripciones y comentarios: [[API-D3]] y ![[API-D3]].

ALTER TABLE project ADD COLUMN next_diagram INTEGER NOT NULL DEFAULT 1;

CREATE TABLE diagram (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES project(id) ON DELETE CASCADE,
    number      INTEGER NOT NULL,
    title       TEXT NOT NULL,
    description TEXT NOT NULL,
    source      TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    UNIQUE (project_id, number)
);

-- Se recalculan al guardar el texto que las contiene, como las menciones.
CREATE TABLE diagram_mention (
    source_id   TEXT NOT NULL REFERENCES task(id) ON DELETE CASCADE,
    comment_id  TEXT REFERENCES comment(id) ON DELETE CASCADE,
    target_id   TEXT NOT NULL REFERENCES diagram(id) ON DELETE CASCADE
);
CREATE INDEX diagram_mention_target ON diagram_mention(target_id);
CREATE INDEX diagram_mention_source ON diagram_mention(source_id, comment_id);
