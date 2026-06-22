CREATE UNIQUE INDEX oidc_login_transactions_by_browser_binding
    ON oidc_login_transactions (browser_binding_hash);
