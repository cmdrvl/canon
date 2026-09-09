-- D3/G3 public-deal selection seed for bd-2ocv.
--
-- Grain: one row per public CMBS deal accession selected from the retained H7
-- loan-key cohort. The H7 seed below is copied from
-- scripts/geo_measurements/fixtures/d1_residuals/canon_geo_h7_population.v0.json
-- after de-duplicating by loan_key + subject_id. The selection rule is frozen:
-- largest count of H7 subjects with reach_status = full, tie broken by the
-- lowest accession string. The second deal is the next row by the same order.
--
-- Required live preparation before execution:
-- * describe PROPERTY_MART.LOAN_ISSUANCE.
-- * The H7 artifact remains retained evidence; this query does not claim live
--   precision or E4 movement.
--
-- Fresh cmdrvl-data MCP probe on 2026-09-09 selected:
-- * rank 1: 0001539497-19-000868 / 1774962 / BANK 2019-BNK18
--   with 56 deal loans, 3 H7 subjects, 2 full reach.
-- * rank 2: 0001539497-19-002255 / 1794303 /
--   CITIGROUP COMMERCIAL MORTGAGE TRUST 2019-C7
--   with 55 deal loans, 2 H7 subjects, 2 full reach.

WITH
h7_subjects AS (
  SELECT column1::TEXT AS loan_key, column2::TEXT AS reach_status
  FROM VALUES
    ('0048d206538d873999f3c2d49f259d31', 'full'),
    ('073ad3a0862827c75501ac66570eb783', 'full'),
    ('07571f6c5757ed966e978b889709caec', 'full'),
    ('076f078b2e509ea5ce0d0787352ac934', 'partial'),
    ('08b389ead42790039ae8b7458ffb1768', 'none'),
    ('0b2476809a8914f5c097a7bdfb340991', 'full'),
    ('0b296a95d154bb0b07a9ddf6733250b1', 'partial'),
    ('0bf44dbcbba46cc3b2c4aafb6ebfeca1', 'none'),
    ('11306d29410b4f52d24de5c1c25bfb43', 'full'),
    ('13fe14e9b37e366e77cd78aa6b5dc4ef', 'none'),
    ('1785a5edc1bc53a620ea2e4669c256bc', 'full'),
    ('2a2d2adf109a880c7f5efd5750972745', 'full'),
    ('2b7bc3d79f35a8e9f91103795d5dc2a6', 'full'),
    ('2bd9a92fb4c295cf8917a9fd43df3333', 'full'),
    ('2e3fac03a961f918cfa6545c33dd95ef', 'full'),
    ('327d8ca486c3607d296f7815f3a7e0ff', 'full'),
    ('3330deb3e65852b88b56094dafc1498d', 'full'),
    ('42bb478bf589352aa72dd46076f6c6ff', 'full'),
    ('433c6803fe0f42d7384f5dddace182c5', 'full'),
    ('49a1e6bbb9d36a905988b675721a3627', 'full'),
    ('49d6e4dcac8841eca517c1190949b551', 'full'),
    ('4b2d7fc321ef05c03b7e8ecaad0152a6', 'partial'),
    ('5081a27f5b2e4b03f660bbba032d82ed', 'full'),
    ('5387c521ca6a6f0dc15c6badec8c0134', 'full'),
    ('59d308b789056c1be5bbee76a1132dc1', 'full'),
    ('5f20d7e125c3f9f761c5119885eef3f3', 'none'),
    ('6134edf46aa6085ebbe33371eec7cb0c', 'partial'),
    ('61d6cd030f3f61a27ebc488503f127c0', 'partial'),
    ('645469abb0305bb496b4fc1b29c5b853', 'full'),
    ('6668e47c143195a41650c2c8eeee9ef1', 'full'),
    ('6adf5e5094f33076147d7fb3e876974e', 'partial'),
    ('6bfe47de21ff7d7e24bf6464871dea9f', 'partial'),
    ('6ed280773520fa75508c912ef4cc1ddb', 'none'),
    ('7402ec7245fbb952845811f2985dc6ad', 'full'),
    ('76f1aad84ddf56ddf5bc81c94063ca70', 'none'),
    ('784461d833637c5cc7c8354947811a53', 'none'),
    ('7b18a4419586e1411d46dafa4375865c', 'partial'),
    ('7f1c77c14409757473122b8016aeae79', 'none'),
    ('870c78d3e872ceb44352a058bf9a819d', 'full'),
    ('8d71ae8d52b901af4a5b657af0b1021c', 'full'),
    ('913f619113b7ceec6118a4844f7e459c', 'full'),
    ('9970d9136018599001e61f2ba8be3519', 'none'),
    ('9a06659b027295baea4de8364dfb8684', 'full'),
    ('9b01e88088f06c65fbd3123bb1ebf9e3', 'none'),
    ('a7ae4cbb005862888e700f7019245ae0', 'none'),
    ('a89343223f12351e8cc666345560b9a8', 'none'),
    ('abc2df548bbd7e2a655aab2c43845b4d', 'partial'),
    ('b1409df04e4b208fe22b043b7c5716ce', 'full'),
    ('b34ace2027f17984c0cbe01683c00ace', 'full'),
    ('b825ec3c31fb3a69ce98a278faa57651', 'partial'),
    ('b85b08189259f887be6bf5597b6e388b', 'none'),
    ('b9045ea09a2e27f6344e39ef5167d0a3', 'partial'),
    ('b986131b16a50d5d7464e1a135290076', 'full'),
    ('ba808176a06bd0612eb8781788606653', 'none'),
    ('beeb77b980757418309bb8f58a6b4fc8', 'partial'),
    ('c6da00e8d423120b97033e5751fc4ffa', 'none'),
    ('c7aae0bb8dcacba8ad27bdfd51cea625', 'full'),
    ('c88a233fbc140d00649dd966add4abb7', 'full'),
    ('c8aaebd5518b664ffa60eb73c3f5dd47', 'full'),
    ('d0e218bc4105a6467cf4225e50437bf1', 'none'),
    ('d34cdcf84fc0badf2d2c364af2ec6ebd', 'full'),
    ('d709ee238f8a21b5452734d5fd8c2de5', 'none'),
    ('dfc0f3e18f4d1be82f94b66b6bc22e84', 'full'),
    ('e134fd1ecaebbd36aa47a3c73acaaf59', 'none'),
    ('e7b0bdb8eeb55cb44e166d9da0314030', 'partial'),
    ('e8e5a5f2fb5a3e69d6dd5a72a8334505', 'full'),
    ('e91ba17e4c02aa0a8c23b4fbf42120db', 'partial'),
    ('eba02144113d62311c81b88192b65625', 'full'),
    ('f214970742c4aa0e386fa8b344d1dc4d', 'full'),
    ('f46f9ea45311d9e17972ae06637393de', 'full'),
    ('fe7fe96d598dc540c7b2265bcd142dbc', 'partial')
),
deal_rollup AS (
  SELECT
    li.first_seen_filing_id AS accession,
    li.cik AS deal_id,
    MIN(li.company_name) AS company_name,
    COUNT(DISTINCT li.loan_key) AS deal_loan_count,
    COUNT(DISTINCT h.loan_key) AS h7_subject_loans,
    COUNT(DISTINCT IFF(h.reach_status = 'full', h.loan_key, NULL)) AS h7_full_loans,
    COUNT(DISTINCT IFF(h.reach_status = 'partial', h.loan_key, NULL)) AS h7_partial_loans,
    COUNT(DISTINCT IFF(h.reach_status = 'none', h.loan_key, NULL)) AS h7_none_loans
  FROM EDGAR_DB.PROPERTY_MART.LOAN_ISSUANCE li
  LEFT JOIN h7_subjects h
    ON h.loan_key = li.loan_key
  WHERE li.first_seen_filing_id IS NOT NULL
    AND li.cik IS NOT NULL
    AND li.loan_key IS NOT NULL
  GROUP BY li.first_seen_filing_id, li.cik
  HAVING COUNT(DISTINCT h.loan_key) > 0
),
ranked AS (
  SELECT
    ROW_NUMBER() OVER (
      ORDER BY h7_full_loans DESC, accession ASC, deal_id ASC
    ) AS selection_rank,
    *
  FROM deal_rollup
)
SELECT
  'canon_geo_d3_deal_selection.v0' AS row_contract,
  selection_rank,
  accession,
  deal_id,
  company_name,
  deal_loan_count,
  h7_subject_loans,
  h7_full_loans,
  h7_partial_loans,
  h7_none_loans,
  deal_loan_count - h7_subject_loans AS not_in_h7_cohort_loans
FROM ranked
WHERE selection_rank <= 2
ORDER BY selection_rank;
