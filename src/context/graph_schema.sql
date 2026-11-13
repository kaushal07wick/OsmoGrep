CREATE TABLE revisions(id TEXT PRIMARY KEY, created_at TEXT NOT NULL);
CREATE TABLE analyses(id TEXT PRIMARY KEY, path TEXT NOT NULL, blob TEXT NOT NULL,
 parser_version TEXT NOT NULL, created_at TEXT NOT NULL, unsupported TEXT NOT NULL);
CREATE TABLE revision_files(revision TEXT NOT NULL REFERENCES revisions(id) ON DELETE CASCADE,
 path TEXT NOT NULL, analysis TEXT NOT NULL REFERENCES analyses(id), PRIMARY KEY(revision,path));
CREATE INDEX revision_analysis ON revision_files(analysis,revision);
CREATE TABLE nodes(analysis TEXT NOT NULL REFERENCES analyses(id) ON DELETE CASCADE,
 id TEXT NOT NULL, label TEXT NOT NULL, kind TEXT NOT NULL, payload TEXT NOT NULL,
 PRIMARY KEY(analysis,id));
CREATE INDEX node_identity ON nodes(id,analysis);
CREATE INDEX node_label ON nodes(label,analysis);
CREATE TABLE edges(analysis TEXT NOT NULL REFERENCES analyses(id) ON DELETE CASCADE,
 id TEXT NOT NULL, source TEXT NOT NULL, target TEXT NOT NULL, payload TEXT NOT NULL,
 PRIMARY KEY(analysis,id), FOREIGN KEY(analysis,source) REFERENCES nodes(analysis,id),
 FOREIGN KEY(analysis,target) REFERENCES nodes(analysis,id));
CREATE INDEX edge_source ON edges(source,analysis);
CREATE INDEX edge_target ON edges(target,analysis);
CREATE TABLE reports(id TEXT PRIMARY KEY, revision TEXT NOT NULL,
 fingerprint TEXT NOT NULL, created_at TEXT NOT NULL, payload TEXT NOT NULL);
CREATE INDEX report_fingerprint ON reports(fingerprint);
CREATE TABLE artifacts(id TEXT PRIMARY KEY, content TEXT NOT NULL);
CREATE TABLE memories(id TEXT PRIMARY KEY, revision TEXT NOT NULL REFERENCES revisions(id),
 environment TEXT NOT NULL, fingerprint TEXT NOT NULL, status TEXT NOT NULL, payload TEXT NOT NULL);
CREATE TABLE memory_resources(memory TEXT NOT NULL REFERENCES memories(id) ON DELETE CASCADE,
 resource TEXT NOT NULL, PRIMARY KEY(memory,resource));
CREATE INDEX memory_resource ON memory_resources(resource,memory);
CREATE TABLE observations(id TEXT PRIMARY KEY, revision TEXT NOT NULL REFERENCES revisions(id),
 service TEXT NOT NULL, resource TEXT NOT NULL, observed_at TEXT NOT NULL, payload TEXT NOT NULL);
CREATE INDEX observation_resource ON observations(revision,resource,observed_at);
CREATE TABLE experiment_jobs(id INTEGER PRIMARY KEY, report TEXT NOT NULL REFERENCES reports(id),
 status TEXT NOT NULL, expires_at INTEGER NOT NULL, evidence TEXT REFERENCES artifacts(id));
CREATE TABLE model_jobs(id INTEGER PRIMARY KEY,report TEXT NOT NULL REFERENCES reports(id),
 status TEXT NOT NULL,tokens INTEGER,evidence TEXT REFERENCES artifacts(id));
PRAGMA user_version=1;
