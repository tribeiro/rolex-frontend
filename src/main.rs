use askama::Template;
use axum::{
    extract::Query,
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use rolex::{block_log::block_log, exposure_log::exposure_log, narrative_log::narrative_log};
use serde::Deserialize;
use std::collections::HashMap;
use tower_http::services::ServeDir;

use chrono::{Duration, NaiveDateTime};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let assets_path = std::env::current_dir().unwrap();

    let api_router = Router::new().route("/getLogMessages", get(get_log_messages));

    let app = Router::new()
        .nest("/api", api_router)
        .route("/", get(handler))
        .route("/log_explorer_app", get(log_explorer_app))
        .route("/log_explorer", get(log_explorer))
        .route("/night_plan", get(night_plan))
        .route("/eon_report", get(eon_report))
        .nest_service(
            "/assets",
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
    let template = HelloTemplate {};
    HtmlTemplate(template)
}

async fn log_explorer(Query(query): Query<LogExplorerQuery>) -> impl IntoResponse {
    println!("Log Date: {:?}", query.log_date);
    let log_form = {
        if let Some(selected_date) = &query.log_date {
            let log_form = get_log_form(&selected_date).await;
            log_form
        } else {
            LogForm {
                logmessages: vec![],
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
        let template = get_log_form(&selected_date).await;
        HtmlTemplate(template)
    } else {
        get_empty_log_form()
    }
}

async fn get_log_form(selected_date: &str) -> LogForm {
    let updated_content = format!("Content updated for date: {selected_date}");

    println!("{updated_content}");

    let parse_from_str = NaiveDateTime::parse_from_str;

    match parse_from_str(&format!("{selected_date}T12:00:00"), "%Y-%m-%dT%H:%M:%S") {
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

            let block_logs =
                block_log::BlockLog::retrieve("summit_efd", &min_date_added, &max_date_added)
                    .await
                    .unwrap_or(vec![]);

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
                    .chain(block_logs.into_iter().map(|entry| {
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
            let template = LogForm {
                logmessages: logmessages.into_iter().map(|(_, entry)| entry).collect(),
            };
            return template;
        }
        Err(error) => {
            println!("Error parsing selected date: {selected_date}. Err: {error}");
            return LogForm {
                logmessages: vec![],
            };
        }
    }
}

fn get_empty_log_form() -> HtmlTemplate<LogForm> {
    let template = LogForm {
        logmessages: vec![],
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
