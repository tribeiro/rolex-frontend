use askama::Template;
use axum::{
    extract::Query,
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
    routing::get,
    Router,
};
use rolex::{
    exposure_log::exposure_log,
    narrative_log::narrative_log,
    sal_script_info::{
        available_scripts::{self, AvailableScript},
        sal_script_info::SalScriptInfo,
    },
};
use serde::Deserialize;
use std::collections::HashMap;
use tower_http::services::ServeDir;

use chrono::{Duration, NaiveDateTime, NaiveTime, Utc};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let assets_path = std::env::current_dir().unwrap();

    let api_router = Router::new().route("/getLogMessages", get(get_log_messages));

    let app = Router::new()
        .nest("/rolex2/api", api_router)
        .route("/", get(|| async { Redirect::permanent("/rolex2") }))
        .route("/rolex2", get(handler))
        .route("/rolex2/log_explorer_app", get(log_explorer_app))
        .route("/rolex2/log_explorer", get(log_explorer))
        .route("/rolex2/night_plan", get(night_plan))
        .route("/rolex2/eon_report", get(eon_report))
        .route("/rolex2/api/getScriptInfo", get(get_script_info))
        .nest_service(
            "/rolex2/assets",
            ServeDir::new(format!("{}/assets", assets_path.to_str().unwrap())),
        );

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    println!("Starting ROLEx server.");

    axum::serve(listener, app).await.unwrap();

    Ok(())
}

#[derive(Debug, Deserialize)]
struct LogExplorerQuery {
    log_date: Option<String>,
}

async fn handler() -> impl IntoResponse {
    println!("Sending welcome page.");
    let template = HelloTemplate {};
    HtmlTemplate(template)
}

async fn log_explorer(Query(query): Query<LogExplorerQuery>) -> impl IntoResponse {
    println!("Log Date: {:?}", query.log_date);
    let log_form = {
        if let Some(selected_date) = &query.log_date {
            let log_form = get_log_form(&selected_date, &None).await;
            log_form
        } else {
            LogForm {
                logmessages: vec![],
                last_entry_date: "Invalid Date".to_owned(),
                last_entry_time: "".to_owned(),
            }
        }
    };
    let template = LogExplorerTemplateStatic {
        selected_date: query.log_date.unwrap_or("Invalid Date".to_owned()),
        logmessages: log_form.logmessages,
    };
    HtmlTemplate(template)
}

async fn log_explorer_app() -> impl IntoResponse {
    let template = LogExplorerTemplate {};
    HtmlTemplate(template)
}

async fn night_plan() -> impl IntoResponse {
    let template = NighPlanTemplate {};
    HtmlTemplate(template)
}

async fn eon_report() -> impl IntoResponse {
    let template = EndOfNightReportTemplate {};
    HtmlTemplate(template)
}

async fn get_log_messages(Query(params): Query<HashMap<String, String>>) -> impl IntoResponse {
    println!("{params:?}");

    if let Some(selected_date) = params.get("log_date") {
        let template = get_log_form(&selected_date, &None).await;
        HtmlTemplate(template)
    } else {
        let now = Utc::now();
        if let Some(day_transition) = NaiveTime::from_hms_opt(12, 0, 0) {
            println!("No log_date parameter in query. Assuming live logging.");
            let time = now.time();
            let date_formatted = {
                if time > day_transition {
                    // Today
                    format!("{}", now.date_naive().format("%Y-%m-%d"))
                } else {
                    // Yesterday
                    let yesterday = now.date_naive() - Duration::days(1);
                    format!("{}", yesterday.format("%Y-%m-%d"))
                }
            };
            let log_time = {
                if let Some(log_time) = params.get("log_time") {
                    Some(log_time.as_str())
                } else {
                    None
                }
            };
            let template = get_log_form(&date_formatted, &log_time).await;
            HtmlTemplate(template)
        } else {
            get_empty_log_form()
        }
    }
}

async fn get_script_info(Query(params): Query<HashMap<String, String>>) -> impl IntoResponse {
    println!("{params:?}");
    if let (Some(sal_index), Some(timestamp)) = (params.get("sal_index"), params.get("timestamp")) {
        let template = {
            if let Ok(sal_index) = sal_index.trim().parse() {
                let available_script = AvailableScript {
                    sal_index,
                    timestamp: timestamp.to_string(),
                    ..Default::default()
                };
                SalScriptInfo::retrieve("summit_efd", &available_script)
                    .await
                    .unwrap_or(SalScriptInfo::default())
            } else {
                SalScriptInfo::default()
            }
        };
        HtmlTemplate(template)
    } else {
        let template = SalScriptInfo::default();
        HtmlTemplate(template)
    }
}

async fn get_log_form(selected_date: &str, start_time: &Option<&str>) -> LogForm {
    let parse_from_str = NaiveDateTime::parse_from_str;
    let (start_time, last_entry_time) = {
        if let Some(last_entry_time) = start_time {
            if let Ok(start_time) = parse_from_str(last_entry_time, "%Y-%m-%dT%H:%M:%S%.f") {
                (Ok(start_time), last_entry_time.to_string())
            } else {
                (
                    parse_from_str(&format!("{selected_date}T12:00:00"), "%Y-%m-%dT%H:%M:%S"),
                    last_entry_time.to_string(),
                )
            }
        } else {
            (
                parse_from_str(&format!("{selected_date}T12:00:00"), "%Y-%m-%dT%H:%M:%S"),
                format!("{selected_date}T12:00:00"),
            )
        }
    };

    match start_time {
        Ok(min_date_added) => {
            let max_date_added = min_date_added + Duration::days(1);
            let params = Some(HashMap::from([
                ("min_date_added".to_string(), min_date_added.to_string()),
                ("max_date_added".to_string(), max_date_added.to_string()),
                ("limit".to_string(), "10000".to_string()),
            ]));

            let base_url = "https://summit-lsp.lsst.codes/narrativelog/messages";
            let narrative_logs = {
                match narrative_log::NarrativeLog::retrieve(base_url, &params).await {
                    Ok(narrative_log) => narrative_log,
                    Err(error) => {
                        println!("{error}");
                        vec![]
                    }
                }
            };

            println!("Got {} narrative logs.", narrative_logs.len());
            let base_url = "https://summit-lsp.lsst.codes/exposurelog/messages";
            let exposure_logs = exposure_log::ExposureLog::retrieve(base_url, &params)
                .await
                .unwrap_or(vec![]);

            println!("Got {} exposure logs.", exposure_logs.len());

            let available_scripts: Vec<available_scripts::AvailableScript> =
                available_scripts::AvailableScript::retrieve(
                    "summit_efd",
                    &min_date_added,
                    &max_date_added,
                )
                .await
                .unwrap_or(vec![])
                .into_iter()
                .filter_map(|available_script| {
                    if available_script.is_final() {
                        Some(available_script)
                    } else {
                        None
                    }
                })
                .collect();

            let logmessages: Vec<(String, String)> = {
                let mut logmessages: Vec<(String, String)> = narrative_logs
                    .into_iter()
                    .map(|entry| {
                        (
                            entry.get_date_added().to_string(),
                            entry
                                .render()
                                .unwrap_or("Failed to render message.".to_string()),
                        )
                    })
                    .chain(exposure_logs.into_iter().map(|entry| {
                        (
                            entry
                                .get_date_added()
                                .clone()
                                .unwrap_or(min_date_added.to_string()),
                            entry
                                .render()
                                .unwrap_or("Failed to render message.".to_string()),
                        )
                    }))
                    .chain(available_scripts.into_iter().map(|entry| {
                        (
                            entry.get_date_added().to_string(),
                            entry
                                .render()
                                .unwrap_or("Failed to render block message.".to_string()),
                        )
                    }))
                    .collect();

                logmessages.sort();
                logmessages.reverse();
                logmessages
            };
            let last_entry_time = {
                if logmessages.is_empty() {
                    last_entry_time
                } else {
                    let last_entry_time = logmessages
                        .get(0)
                        .unwrap_or(&("".to_string(), "".to_string()))
                        .0
                        .replace("Z", "");
                    if let Ok(last_entry_time) =
                        parse_from_str(&last_entry_time, "%Y-%m-%dT%H:%M:%S%.f")
                    {
                        let new_last_entry_time = last_entry_time + Duration::milliseconds(100);
                        new_last_entry_time
                            .format("%Y-%m-%dT%H:%M:%S%.f")
                            .to_string()
                    } else {
                        last_entry_time
                    }
                }
            };
            let template = LogForm {
                logmessages: logmessages.into_iter().map(|(_, entry)| entry).collect(),
                last_entry_time,
                last_entry_date: selected_date.to_owned(),
            };
            return template;
        }
        Err(error) => {
            println!("Error parsing selected date: {selected_date}. Err: {error}");
            return LogForm {
                logmessages: vec![],
                last_entry_time: "".to_owned(),
                last_entry_date: selected_date.to_owned(),
            };
        }
    }
}

fn get_empty_log_form() -> HtmlTemplate<LogForm> {
    let template = LogForm {
        logmessages: vec![],
        last_entry_time: "".to_owned(),
        last_entry_date: "".to_owned(),
    };
    HtmlTemplate(template)
}

#[derive(Template)]
#[template(path = "hello.html")]
struct HelloTemplate;

#[derive(Template, Debug)]
#[template(path = "log_explorer.html")]
struct LogExplorerTemplate;

#[derive(Template, Debug)]
#[template(path = "log_explorer_static.html")]
struct LogExplorerTemplateStatic {
    selected_date: String,
    logmessages: Vec<String>,
}

#[derive(Template)]
#[template(path = "night_plan.html")]
struct NighPlanTemplate;

#[derive(Template)]
#[template(path = "eon_report.html")]
struct EndOfNightReportTemplate;

#[derive(Template, Debug)]
#[template(path = "log_list.html")]
struct LogForm {
    logmessages: Vec<String>,
    last_entry_time: String,
    last_entry_date: String,
}

struct HtmlTemplate<T>(T);

impl<T> IntoResponse for HtmlTemplate<T>
where
    T: Template,
{
    fn into_response(self) -> axum::response::Response {
        match self.0.render() {
            Ok(html) => Html(html).into_response(),
            Err(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to render template. Error: {err}"),
            )
                .into_response(),
        }
    }
}
