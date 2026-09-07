-- The two credit packs, as the server needs to know them.
--
-- App Store Connect decides what a pack *costs*. This decides what it is
-- *worth*. Nothing reconciles the two automatically, so a pack that is
-- missing here is a purchase that takes a customer's money and grants
-- nothing — the redeem endpoint looks the product up before it verifies
-- anything, and an unknown product is rejected.
--
-- `bundle_id` is not decoration. Apple signs every developer's receipts
-- with the same root, so a valid signature proves "some app", not "this
-- app"; this column is what closes that gap.
--
-- Run against the production database:
--
--   sqlite3 /var/lib/openapps/openapps.db < apps/ios/store/products.sql
--
-- Safe to re-run: the INSERTs are idempotent on (platform, product_id).

-- Fails loudly if the app was never registered, rather than inserting
-- rows that reference nothing. Register it first if this trips.
SELECT RAISE(ABORT, 'no app with id openpdfedit — register it first')
WHERE NOT EXISTS (SELECT 1 FROM apps WHERE id = 'openpdfedit');

INSERT OR IGNORE INTO app_iap_products
  (platform, product_id, app_id, bundle_id, credits, usd_price, created_at)
VALUES
  ('apple', 'credits_1000', 'openpdfedit', 'com.openpdfedit.app', 1000,  499, unixepoch()),
  ('apple', 'credits_5000', 'openpdfedit', 'com.openpdfedit.app', 5000, 1999, unixepoch());

-- Google Play, for when the Android app exists. The package name must
-- match the one in the Play Console; it is checked the same way Apple's
-- bundle id is.
--
-- INSERT OR IGNORE INTO app_iap_products
--   (platform, product_id, app_id, bundle_id, credits, usd_price, created_at)
-- VALUES
--   ('google', 'credits_1000', 'openpdfedit', 'com.openpdfedit.app', 1000,  499, unixepoch()),
--   ('google', 'credits_5000', 'openpdfedit', 'com.openpdfedit.app', 5000, 1999, unixepoch());

SELECT platform, product_id, bundle_id, credits, usd_price
FROM app_iap_products WHERE app_id = 'openpdfedit' ORDER BY platform, credits;
