#!/bin/bash
set -e

# 1. Validate arguments
if [ "$#" -ne 2 ]; then
    echo "Usage: $0 <path_to_sqlite_db> <entity>"
    echo "Available entities: all, issuer"
    exit 1
fi

DB_PATH=$1
ENTITY=$2

# 2. Validate entity option
if [[ "$ENTITY" != "all" && "$ENTITY" != "issuer" ]]; then
    echo "Error: Invalid entity '$ENTITY'."
    echo "Available entities: all, issuer"
    exit 1
fi

# 3. Check if sqlite3 is installed
if ! command -v sqlite3 &> /dev/null; then
    echo "Error: sqlite3 could not be found. Please install it first."
    exit 1
fi

# 4. Define the function to load issuers
load_issuers() {
    echo "Loading issuers into database: $DB_PATH..."

    sqlite3 "$DB_PATH" <<'EOF'
BEGIN TRANSACTION;

INSERT OR IGNORE INTO issuer (id, name, status, created_at, version, cnpj, lei, country_code)
VALUES
    -- Financials & Banks
    (randomblob(16), 'Itaú Unibanco Holding S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '60872504000123', NULL, 'BR'),
    (randomblob(16), 'Banco Bradesco S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '60746948000112', NULL, 'BR'),
    (randomblob(16), 'Banco do Brasil S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '00000000000191', '549300H400B6R2450C53', 'BR'),
    (randomblob(16), 'B3 S.A. - Brasil, Bolsa, Balcão', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '09346601000125', '254900X7B94Y87K24S45', 'BR'),
    (randomblob(16), 'Itaúsa S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '61532644000115', NULL, 'BR'),
    (randomblob(16), 'BB Seguridade Participações S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '17344597000194', NULL, 'BR'),

    -- Commodities & Energy (Oil, Gas, Mining)
    (randomblob(16), 'Petróleo Brasileiro S.A. - Petrobras', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '33000167000101', '549300N738Q5O2A04M11', 'BR'),
    (randomblob(16), 'Vale S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '33592510000154', '549300BQO2QG6F9A2A21', 'BR'),
    (randomblob(16), 'Cosan S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '50746577000115', NULL, 'BR'),
    (randomblob(16), 'Vibra Energia S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '34274233000102', NULL, 'BR'),
    (randomblob(16), 'Gerdau S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '33611500000119', NULL, 'BR'),
    (randomblob(16), 'Braskem S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '42150391000170', NULL, 'BR'),

    -- Utilities (Electricity & Water)
    (randomblob(16), 'Centrais Elétricas Brasileiras S.A. - Eletrobras', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '00001180000126', NULL, 'BR'),
    (randomblob(16), 'Equatorial Energia S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '03220438000173', NULL, 'BR'),
    (randomblob(16), 'Companhia de Saneamento Básico do Estado de São Paulo - Sabesp', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '43776517000180', NULL, 'BR'),
    (randomblob(16), 'CPFL Energia S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '02429144000193', NULL, 'BR'),
    (randomblob(16), 'Engie Brasil Energia S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '02474398000123', NULL, 'BR'),
    (randomblob(16), 'Companhia Paranaense de Energia - Copel', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '76483817000120', NULL, 'BR'),
    (randomblob(16), 'Eneva S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '04423567000121', NULL, 'BR'),
    (randomblob(16), 'Energisa S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '00864214000106', NULL, 'BR'),

    -- Food, Beverage & Pulp
    (randomblob(16), 'Ambev S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '07526557000100', NULL, 'BR'),
    (randomblob(16), 'JBS S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '02916265000160', NULL, 'BR'),
    (randomblob(16), 'Suzano S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '16404287000155', NULL, 'BR'),
    (randomblob(16), 'Klabin S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '89637490000145', NULL, 'BR'),

    -- Logistics & Infrastructure
    (randomblob(16), 'Rumo S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '02387241000160', NULL, 'BR'),
    (randomblob(16), 'CCR S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '02846056000197', NULL, 'BR'),
    (randomblob(16), 'Ultrapar Participações S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '33256439000139', NULL, 'BR'),
    (randomblob(16), 'Localiza Rent a Car S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '16670085000155', NULL, 'BR'),

    -- Industry, Tech & Telecom
    (randomblob(16), 'WEG S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '84429695000111', NULL, 'BR'),
    (randomblob(16), 'Embraer S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '07689002000189', NULL, 'BR'),
    (randomblob(16), 'Telefônica Brasil S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '02558157000162', NULL, 'BR'),
    (randomblob(16), 'TIM S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '02421421000111', NULL, 'BR'),
    (randomblob(16), 'Totvs S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '53113791000122', NULL, 'BR'),

    -- Retail & Consumer Goods
    (randomblob(16), 'Raia Drogasil S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '61585865000151', NULL, 'BR'),
    (randomblob(16), 'Lojas Renner S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '92754738000162', NULL, 'BR'),
    (randomblob(16), 'Natura &Co Holding S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '32785497000197', NULL, 'BR'),
    (randomblob(16), 'Magazine Luiza S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '47960950000121', NULL, 'BR'),
    (randomblob(16), 'Sendas Distribuidora S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '06057223000171', NULL, 'BR'),
    (randomblob(16), 'B2W Companhia Digital', 'RETIRED', '2026-07-30T12:00:00Z', 1, '00776574000156', NULL, 'BR'),

    -- Healthcare
    (randomblob(16), 'Rede D''Or São Luiz S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '06047087000139', NULL, 'BR'),
    (randomblob(16), 'Hypera S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '02932074000191', NULL, 'BR'),
    (randomblob(16), 'Hapvida Participações e Investimentos S.A.', 'ACTIVE', '2026-07-30T12:00:00Z', 1, '05197443000138', NULL, 'BR');

COMMIT;
EOF
    echo "Successfully loaded issuers."
}

# 5. Route execution based on entity
if [[ "$ENTITY" == "issuer" || "$ENTITY" == "all" ]]; then
    load_issuers
fi

echo "Done!"
