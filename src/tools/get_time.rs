
use chrono::{DateTime, Utc};
use chrono_tz::Tz;
use log::info;
use std::collections::HashMap;

use crate::{context::ObserverContext, lmclient::LMTool};

pub struct GetTime {}

impl GetTime {
    pub fn new() -> Self {
        Self {}
    }
}

impl GetTime {
    /// 国コードを元に現在の時刻を取得する
    pub fn get_time_by_country(&self, country_code: &str) -> Result<String, String> {
        // 国コード -> タイムゾーンの対応表
        let country_to_tz: HashMap<&str, Tz> = [
            ("AE", chrono_tz::Asia::Dubai),
            ("AR", chrono_tz::America::Argentina::Buenos_Aires),
            ("AT", chrono_tz::Europe::Vienna),
            ("AU", chrono_tz::Australia::Sydney),
            ("BE", chrono_tz::Europe::Brussels),
            ("BG", chrono_tz::Europe::Sofia),
            ("BO", chrono_tz::America::La_Paz),
            ("BR", chrono_tz::America::Sao_Paulo),
            ("CA", chrono_tz::America::Toronto),
            ("CH", chrono_tz::Europe::Zurich),
            ("CL", chrono_tz::America::Santiago),
            ("CN", chrono_tz::Asia::Shanghai),
            ("CO", chrono_tz::America::Bogota),
            ("CZ", chrono_tz::Europe::Prague),
            ("DE", chrono_tz::Europe::Berlin),
            ("DK", chrono_tz::Europe::Copenhagen),
            ("EC", chrono_tz::America::Guayaquil),
            ("EG", chrono_tz::Africa::Cairo),
            ("ES", chrono_tz::Europe::Madrid),
            ("FI", chrono_tz::Europe::Helsinki),
            ("FR", chrono_tz::Europe::Paris),
            ("GB", chrono_tz::Europe::London),
            ("GR", chrono_tz::Europe::Athens),
            ("HK", chrono_tz::Asia::Hong_Kong),
            ("HU", chrono_tz::Europe::Budapest),
            ("ID", chrono_tz::Asia::Jakarta),
            ("IE", chrono_tz::Europe::Dublin),
            ("IL", chrono_tz::Asia::Jerusalem),
            ("IN", chrono_tz::Asia::Kolkata),
            ("IR", chrono_tz::Asia::Tehran),
            ("IT", chrono_tz::Europe::Rome),
            ("JP", chrono_tz::Asia::Tokyo),
            ("KR", chrono_tz::Asia::Seoul),
            ("MX", chrono_tz::America::Mexico_City),
            ("MY", chrono_tz::Asia::Kuala_Lumpur),
            ("NG", chrono_tz::Africa::Lagos),
            ("NL", chrono_tz::Europe::Amsterdam),
            ("NO", chrono_tz::Europe::Oslo),
            ("NZ", chrono_tz::Pacific::Auckland),
            ("PE", chrono_tz::America::Lima),
            ("PH", chrono_tz::Asia::Manila),
            ("PK", chrono_tz::Asia::Karachi),
            ("PL", chrono_tz::Europe::Warsaw),
            ("PT", chrono_tz::Europe::Lisbon),
            ("PY", chrono_tz::America::Asuncion),
            ("QA", chrono_tz::Asia::Qatar),
            ("RO", chrono_tz::Europe::Bucharest),
            ("RU", chrono_tz::Europe::Moscow),
            ("SA", chrono_tz::Asia::Riyadh),
            ("SE", chrono_tz::Europe::Stockholm),
            ("SG", chrono_tz::Asia::Singapore),
            ("TH", chrono_tz::Asia::Bangkok),
            ("TR", chrono_tz::Europe::Istanbul),
            ("TW", chrono_tz::Asia::Taipei),
            ("UA", chrono_tz::Europe::Kyiv),
            ("US", chrono_tz::America::New_York),
            ("UY", chrono_tz::America::Montevideo),
            ("VE", chrono_tz::America::Caracas),
            ("VN", chrono_tz::Asia::Ho_Chi_Minh),
            ("ZA", chrono_tz::Africa::Johannesburg),
        ]
        .iter()
        .cloned()
        .collect();

        // 対応するタイムゾーンを取得
        let tz = country_to_tz
            .get(country_code)
            .ok_or(format!("Unsupported country code: {}", country_code))?;

        // 現在の UTC 時間を取得し、指定のタイムゾーンに変換
        let utc_now: DateTime<Utc> = Utc::now();
        let local_time = utc_now.with_timezone(tz);

        Ok(format!("The current time in {} ({}) is: {}", country_code, tz, local_time))
    }
}

#[async_trait::async_trait]
impl LMTool for GetTime {
    fn name(&self) -> String {
        "get-location-time".to_string()
    }

    fn description(&self) -> String {
        "Get the current time of the location based on the country code".to_string()
    }

    fn json_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "country_code": {
                    "type": "string",
                    "description": "ISO 3166-1 alpha-2 country code (e.g., 'US', 'JP', 'FR')"
                },
                "$explain": {
                    "type": "string",
                    "description": "A brief explanation of what you are doing with this tool."
                }
            },
            "required": ["country_code"]
        })
    }

    async fn execute(&self, args: serde_json::Value, _ob_ctx: ObserverContext) -> Result<String, String> {
        info!("GetTime::run called with args: {:?}", args);
        let country_code = args.get("country_code")
            .and_then(|v| v.as_str())
            .ok_or("Missing or invalid 'country_code' parameter".to_string())?;

        self.get_time_by_country(country_code)
    }
}
