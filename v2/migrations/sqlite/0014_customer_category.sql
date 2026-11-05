ALTER TABLE customers ADD COLUMN customer_category TEXT;

CREATE TRIGGER customers_category_not_blank_insert
BEFORE INSERT ON customers
WHEN NEW.customer_category IS NOT NULL AND length(trim(NEW.customer_category)) = 0
BEGIN
    SELECT RAISE(ABORT, 'customer_category must not be blank');
END;

CREATE TRIGGER customers_category_not_blank_update
BEFORE UPDATE OF customer_category ON customers
WHEN NEW.customer_category IS NOT NULL AND length(trim(NEW.customer_category)) = 0
BEGIN
    SELECT RAISE(ABORT, 'customer_category must not be blank');
END;
