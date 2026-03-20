-- db/seed_events.sql — generated
INSERT INTO audit_event_types (id, name, description)
    VALUES (0, 'plugin.load', 'Plugin loaded into host')
    ON CONFLICT (name) DO NOTHING;
INSERT INTO audit_event_types (id, name, description)
    VALUES (1, 'plugin.unload', 'Plugin removed from host')
    ON CONFLICT (name) DO NOTHING;
INSERT INTO audit_event_types (id, name, description)
    VALUES (2, 'config.change', 'Configuration key changed')
    ON CONFLICT (name) DO NOTHING;
