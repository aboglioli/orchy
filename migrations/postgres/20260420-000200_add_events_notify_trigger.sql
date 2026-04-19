CREATE OR REPLACE FUNCTION notify_event_inserted()
RETURNS TRIGGER AS $$
BEGIN
    PERFORM pg_notify('orchy_events', NEW.organization);
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE TRIGGER events_after_insert
AFTER INSERT ON events
FOR EACH ROW EXECUTE FUNCTION notify_event_inserted();
