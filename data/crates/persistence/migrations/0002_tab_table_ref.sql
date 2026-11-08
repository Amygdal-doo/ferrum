-- Table-browser tabs remember which table they show (JSON TableRef).
ALTER TABLE editor_tabs ADD COLUMN table_ref TEXT;
