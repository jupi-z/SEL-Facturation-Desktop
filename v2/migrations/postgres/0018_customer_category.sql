ALTER TABLE customers
    ADD COLUMN customer_category TEXT;

ALTER TABLE customers
    ADD CONSTRAINT customers_category_not_blank
    CHECK (customer_category IS NULL OR length(trim(customer_category)) > 0);
