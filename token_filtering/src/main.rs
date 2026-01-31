use solana_program::pubkey::Pubkey;
use std::io::{self, Write};

const RUGCHECK_BASE: &str = "https://api.rugcheck.xyz";
const GMGN_BASE: &str = "https://gmgn.ai";

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36")
        .build()
        .expect("failed to create HTTP client")
}

fn gmgn_query_params() -> &'static str {
    "device_id=550e8400-e29b-41d4-a716-446655440000&fp_did=a1b2c3d4e5f67890abcdef12&client_id=gmgn_web_20260131-10580-95ff663&from_app=gmgn&app_ver=20260131-10580-95ff663&tz_name=Asia%2FCalcutta&tz_offset=19800&app_lang=en-US&os=web&worker=0"
}

async fn get_rugcheck_report(mint: &str) -> Result<serde_json::Value, reqwest::Error> {
    let url = format!("{}/v1/tokens/{}/report", RUGCHECK_BASE, mint);
    client().get(&url).send().await?.json::<serde_json::Value>().await
}

async fn get_gmgn_holder_stat(mint: &str) -> Result<serde_json::Value, reqwest::Error> {
    let url = format!(
        "{}/vas/api/v1/token_holder_stat/sol/{}?{}",
        GMGN_BASE,
        mint,
        gmgn_query_params()
    );
    client().get(&url).send().await?.json::<serde_json::Value>().await
}

async fn get_gmgn_token_holders(mint: &str) -> Result<serde_json::Value, reqwest::Error> {
    let url = format!(
        "{}/vas/api/v1/token_holders/sol/{}?{}&limit=100&cost=20&orderby=amount_percentage&direction=desc",
        GMGN_BASE,
        mint,
        gmgn_query_params()
    );
    client().get(&url).send().await?.json::<serde_json::Value>().await
}

fn gmgn_ok(response: &serde_json::Value) -> bool {
    let code = response.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
    let message = response.get("message").and_then(|m| m.as_str()).unwrap_or("");
    code == 0 && message == "success"
}

// -----------------------------------------------------------------------------
// Struct for screening decision (all data points used by the algo later)
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct Risk {
    pub name: Option<String>,
    pub level: Option<String>,
    pub description: Option<String>,
    pub score: Option<i64>,
    pub value: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TopHolder {
    pub address: String,
    pub pct: f64,
    pub insider: bool,
    pub maker_token_tags: Vec<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct TokenScreenData {
    pub mint: String,

    // RugCheck
    pub score: Option<i64>,
    pub score_normalised: Option<i64>,
    pub top_holder_pct: Option<f64>,
    pub total_holders: u64,
    pub creator: Option<String>,
    pub creator_balance: Option<u64>,
    pub risks: Vec<Risk>,
    pub lp_locked_pct: Option<f64>,
    pub total_lp_providers: u64,
    pub graph_insiders_detected: u64,
    pub rugged: Option<bool>,
    pub price: Option<f64>,
    pub market_cap: Option<f64>,
    pub mc_per_holder: Option<f64>,

    // GMGN holder_stat
    pub fresh_wallet_count: Option<u64>,
    pub insider_count: Option<u64>,
    pub bluechip_owner_count: Option<u64>,
    pub bundler_count: Option<u64>,
    pub dex_bot_count: Option<u64>,
    pub sniper_count: Option<u64>,
    pub dev_count: Option<u64>,

    // Derived (computed from above)
    pub insiders_pct: Option<f64>,
    pub bluechip_pct: Option<f64>,
    pub bundler_pct: Option<f64>,
    pub fresh_ratio: Option<f64>,
    pub bundled_ratio: Option<f64>,

    // Derived from token_holders (more reliable than holder_stat when count > total_holders)
    pub bundler_supply_pct: Option<f64>,   // % of supply held by bundlers (sum of pct)
    pub bundler_holder_ratio: Option<f64>, // fraction of top holders that are bundlers

    // GMGN token_holders (top holders with labels)
    pub top_holders: Vec<TopHolder>,
}

// -----------------------------------------------------------------------------
// Evaluation: hard fails + warnings → Pass/Fail
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenResult {
    Pass,
    Fail,
}

/// Risk names that mean instant No (hard fail)
const CRITICAL_RISK_NAMES: &[&str] = &[
    "Top holder concentration",
    "Creator has rugged",
    "Creator sold",
    "Honeypot",
];

/// Tags that indicate a "diverse" top holder (not "too clean")
const DESIRABLE_HOLDER_TAGS: &[&str] = &["bundler", "bluechip", "whale", "axiom"];

fn evaluate(data: &TokenScreenData) -> (ScreenResult, Vec<String>) {
    let mut reasons = Vec::new();

    // ---- Hard fails ----
    if let Some(score) = data.score_normalised {
        if score > 20 {
            reasons.push(format!("HARD: score_normalised {} > 20", score));
        }
    }

    if let Some(pct) = data.top_holder_pct {
        if pct > 10.0 {
            reasons.push(format!("HARD: top_holder_pct {:.1}% > 10%", pct));
        }
    }

    if data.total_holders < 500 {
        reasons.push(format!(
            "HARD: total_holders {} < 500 (too few for new coin)",
            data.total_holders
        ));
    }
    if data.total_holders > 3000 {
        reasons.push(format!(
            "HARD: total_holders {} > 3000 (outside target range)",
            data.total_holders
        ));
    }

    if let Some(pct) = data.insiders_pct {
        if pct > 5.0 {
            reasons.push(format!("HARD: insiders_pct {:.1}% > 5%", pct));
        }
    }

    // Prefer bundler_supply_pct (from token_holders) - more reliable when holder_stat counts exceed total_holders
    let bundler_pct = data.bundler_supply_pct.or(data.bundler_pct);
    if let Some(pct) = bundler_pct {
        if pct > 30.0 {
            reasons.push(format!("HARD: bundler_pct {:.1}% > 30%", pct));
        }
    }

    if let Some(pct) = data.bluechip_pct {
        if pct < 0.5 {
            reasons.push(format!("HARD: bluechip_pct {:.2}% < 0.5%", pct));
        }
    }

    if let Some(r) = data.fresh_ratio {
        if r > 0.4 {
            reasons.push(format!("HARD: fresh_ratio {:.2} > 0.4", r));
        }
    }

    // Prefer bundler_holder_ratio (from token_holders) - more reliable when holder_stat counts exceed total_holders
    let bundled_ratio = data.bundler_holder_ratio.or(data.bundled_ratio);
    if let Some(r) = bundled_ratio {
        if r > 0.4 {
            reasons.push(format!("HARD: bundled_ratio {:.2} > 0.4", r));
        }
    }

    for risk in &data.risks {
        if let Some(ref name) = risk.name {
            if CRITICAL_RISK_NAMES.iter().any(|c| name.contains(c)) {
                reasons.push(format!("HARD: critical risk '{}'", name));
            }
        }
    }

    if data.rugged == Some(true) {
        reasons.push("HARD: token marked as rugged".to_string());
    }

    // "Too clean" top holders: top 10 all have only top_holder, none have bundler/bluechip/whale/axiom
    let top10 = data.top_holders.iter().take(10);
    let has_desirable = top10.clone().any(|h| {
        h.maker_token_tags
            .iter()
            .chain(h.tags.iter())
            .any(|t| DESIRABLE_HOLDER_TAGS.iter().any(|d| t.contains(d)))
    });
    if !data.top_holders.is_empty() && !has_desirable {
        reasons.push("HARD: top holders 'too clean' (no bundler/bluechip/whale/axiom)".to_string());
    }

    if !reasons.is_empty() {
        return (ScreenResult::Fail, reasons);
    }

    // ---- Warnings ----
    let mut warn_count = 0;

    if let Some(score) = data.score_normalised {
        if (10..=20).contains(&score) {
            reasons.push(format!("WARN: score_normalised {} in 10-20 (elevated)", score));
            warn_count += 1;
        }
    }

    if data.total_lp_providers < 5 {
        reasons.push(format!(
            "WARN: total_lp_providers {} < 5",
            data.total_lp_providers
        ));
        warn_count += 1;
    }

    for risk in &data.risks {
        if let Some(ref name) = risk.name {
            if !CRITICAL_RISK_NAMES.iter().any(|c| name.contains(c)) {
                reasons.push(format!("WARN: risk '{}'", name));
                warn_count += 1;
            }
        }
    }

    if let Some(c) = data.fresh_wallet_count {
        if c < 100 {
            reasons.push(format!("WARN: fresh_wallet_count {} < 100", c));
            warn_count += 1;
        }
    }

    if let Some(c) = data.bundler_count {
        if c < 100 {
            reasons.push(format!("WARN: bundler_count {} < 100", c));
            warn_count += 1;
        }
    }

    const WARNING_THRESHOLD: usize = 2;
    if warn_count >= WARNING_THRESHOLD {
        reasons.insert(
            0,
            format!(
                "FAIL: {} warnings (threshold {})",
                warn_count, WARNING_THRESHOLD
            ),
        );
        return (ScreenResult::Fail, reasons);
    }

    (ScreenResult::Pass, reasons)
}

fn parse_risk(v: &serde_json::Value) -> Risk {
    Risk {
        name: v.get("name").and_then(|n| n.as_str()).map(String::from),
        level: v.get("level").and_then(|l| l.as_str()).map(String::from),
        description: v.get("description").and_then(|d| d.as_str()).map(String::from),
        score: v.get("score").and_then(|s| s.as_i64()),
        value: v.get("value").and_then(|v| v.as_str()).map(String::from),
    }
}

fn parse_rugcheck(mint: &str, json: &serde_json::Value) -> TokenScreenData {
    let total_holders = json.get("totalHolders").and_then(|v| v.as_u64()).unwrap_or(0);
    let top_holder_pct = json
        .get("topHolders")
        .and_then(|a| a.as_array())
        .and_then(|a| a.first())
        .and_then(|h| h.get("pct").and_then(|p| p.as_f64()));

    let risks = json
        .get("risks")
        .and_then(|a| a.as_array())
        .map(|a| a.iter().map(parse_risk).collect())
        .unwrap_or_default();

    let lp_locked_pct = json
        .get("markets")
        .and_then(|m| m.as_array())
        .and_then(|a| a.first())
        .and_then(|m| m.get("lp"))
        .and_then(|lp| lp.get("lpLockedPct"))
        .and_then(|v| v.as_f64());

    let price = json.get("price").and_then(|v| v.as_f64());
    let supply = json
        .get("token")
        .and_then(|t| t.get("supply"))
        .and_then(|s| s.as_u64());
    let decimals = json
        .get("token")
        .and_then(|t| t.get("decimals"))
        .and_then(|d| d.as_u64())
        .unwrap_or(6);
    let market_cap = price.and_then(|p| supply.map(|s| p * (s as f64 / 10_f64.powi(decimals as i32))));
    let mc_per_holder = market_cap.map(|mc| mc / total_holders.max(1) as f64);

    TokenScreenData {
        mint: mint.to_string(),
        score: json.get("score").and_then(|v| v.as_i64()),
        score_normalised: json.get("score_normalised").and_then(|v| v.as_i64()),
        top_holder_pct,
        total_holders,
        creator: json.get("creator").and_then(|c| c.as_str()).map(String::from),
        creator_balance: json.get("creatorBalance").and_then(|v| v.as_u64()),
        risks,
        lp_locked_pct,
        total_lp_providers: json
            .get("totalLPProviders")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        graph_insiders_detected: json
            .get("graphInsidersDetected")
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        rugged: json.get("rugged").and_then(|v| v.as_bool()),
        price,
        market_cap,
        mc_per_holder,
        ..Default::default()
    }
}

fn merge_gmgn_holder_stat(data: &mut TokenScreenData, json: &serde_json::Value) {
    let d = match json.get("data") {
        Some(v) => v,
        None => return,
    };
    data.fresh_wallet_count = d.get("fresh_wallet_count").and_then(|v| v.as_u64());
    data.insider_count = d.get("insider_count").and_then(|v| v.as_u64());
    data.bluechip_owner_count = d.get("bluechip_owner_count").and_then(|v| v.as_u64());
    data.bundler_count = d.get("bundler_count").and_then(|v| v.as_u64());
    data.dex_bot_count = d.get("dex_bot_count").and_then(|v| v.as_u64());
    data.sniper_count = d.get("sniper_count").and_then(|v| v.as_u64());
    data.dev_count = d.get("dev_count").and_then(|v| v.as_u64());

    // Compute derived (cap at 100% / 1.0 when holder_stat counts can exceed total_holders)
    let total = data.total_holders as f64;
    if total > 0.0 {
        data.insiders_pct = data.insider_count.map(|c| (c as f64 / total) * 100.0);
        data.bluechip_pct = data.bluechip_owner_count.map(|c| (c as f64 / total) * 100.0);
        data.bundler_pct = data.bundler_count.map(|c| {
            let pct = (c as f64 / total) * 100.0;
            if pct > 100.0 { 100.0 } else { pct }
        });
        data.fresh_ratio = data.fresh_wallet_count.map(|c| c as f64 / total);
        data.bundled_ratio = data.bundler_count.map(|c| {
            let r = c as f64 / total;
            if r > 1.0 { 1.0 } else { r }
        });
    }
}

fn merge_gmgn_token_holders(data: &mut TokenScreenData, json: &serde_json::Value) {
    let list = json
        .get("data")
        .and_then(|d| d.get("list"))
        .and_then(|l| l.as_array());
    let Some(list) = list else { return };

    data.top_holders = list
        .iter()
        .map(|h| {
            let maker_token_tags = h
                .get("maker_token_tags")
                .and_then(|t| t.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str()).map(String::from).collect())
                .unwrap_or_default();
            let tags = h
                .get("tags")
                .and_then(|t| t.as_array())
                .map(|a| a.iter().filter_map(|v| v.as_str()).map(String::from).collect())
                .unwrap_or_default();
            TopHolder {
                address: h.get("address").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                pct: h.get("amount_percentage").and_then(|v| v.as_f64()).unwrap_or(0.0) * 100.0,
                insider: h.get("insider").and_then(|v| v.as_bool()).unwrap_or(false),
                maker_token_tags,
                tags,
            }
        })
        .collect();

    // Compute bundler metrics from token_holders (more reliable than holder_stat when counts exceed total_holders)
    let has_bundler = |h: &TopHolder| {
        h.maker_token_tags
            .iter()
            .chain(h.tags.iter())
            .any(|t| t.to_lowercase().contains("bundler"))
    };
    let bundler_supply_pct: f64 = data
        .top_holders
        .iter()
        .filter(|h| has_bundler(h))
        .map(|h| h.pct)
        .sum();
    let bundler_count_in_list = data.top_holders.iter().filter(|h| has_bundler(h)).count();
    let n = data.top_holders.len();
    data.bundler_supply_pct = if n > 0 {
        Some(bundler_supply_pct)
    } else {
        None
    };
    data.bundler_holder_ratio = if n > 0 {
        Some(bundler_count_in_list as f64 / n as f64)
    } else {
        None
    };
}

#[tokio::main]
async fn main() {
    let mut user_input = String::new();
    print!("please enter token address: ");
    io::stdout().flush().unwrap();
    io::stdin()
        .read_line(&mut user_input)
        .expect("failed to read user input");

    let mint = user_input.trim();
    let _token_address: Pubkey = mint.parse().expect("invalid pubkey");

    println!("starting rug pull check for {}", mint);

    let rugcheck = match get_rugcheck_report(mint).await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("failed to fetch RugCheck report: {}", e);
            return;
        }
    };

    let mut data = parse_rugcheck(mint, &rugcheck);

    if let Ok(gmgn_stat) = get_gmgn_holder_stat(mint).await {
        if gmgn_ok(&gmgn_stat) {
            merge_gmgn_holder_stat(&mut data, &gmgn_stat);
        }
    }

    if let Ok(gmgn_holders) = get_gmgn_token_holders(mint).await {
        if gmgn_ok(&gmgn_holders) {
            merge_gmgn_token_holders(&mut data, &gmgn_holders);
        }
    }

    println!("\n========== TOKEN SCREEN DATA ==========");
    println!("{:#?}", data);

    let (result, reasons) = evaluate(&data);
    println!("\n========== RESULT ==========");
    match result {
        ScreenResult::Pass => {
            println!("Worth providing liquidity: Yes");
            for r in &reasons {
                println!("  (warning) {}", r);
            }
        }
        ScreenResult::Fail => {
            println!("Worth providing liquidity: No");
            for r in &reasons {
                println!("  - {}", r);
            }
        }
    }
}
