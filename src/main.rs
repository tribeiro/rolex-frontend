use askama::Template;
use axum::{
    extract::Query,
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use rolex::{exposure_log::exposure_log, narrative_log::narrative_log};
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
        .route("/log_explorer", get(log_explorer))
        .route("/night_plan", get(night_plan))
        .route("/eon_report", get(eon_report))
        .nest_service(
            "/assets",
            ServeDir::new(format!("{}/assets", assets_path.to_str().unwrap())),
        );

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    axum::serve(listener, app).await.unwrap();

    Ok(())
}

async fn handler() -> impl IntoResponse {
    let template = HelloTemplate {};
    HtmlTemplate(template)
}

async fn log_explorer() -> impl IntoResponse {
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
        let updated_content = format!("Content updated for date: {selected_date}");

        println!("{updated_content}");

        // let startDate: string = searchParams.get("log_date") + "T12:00:00";
        // let endDate: string = dayjs(startDate).add(1, "day").format("YYYY-MM-DDTHH:mm:ss");

        let parse_from_str = NaiveDateTime::parse_from_str;

        match parse_from_str(&format!("{selected_date}T12:00:00"), "%Y-%m-%dT%H:%M:%S") {
            Ok(min_date_added) => {
                let max_date_added = min_date_added + Duration::days(1);
                let base_url = "https://tucson-teststand.lsst.codes/narrativelog/messages";
                let params = Some(HashMap::from([
                    ("min_date_added".to_string(), min_date_added.to_string()),
                    ("max_date_added".to_string(), max_date_added.to_string()),
                    ("limit".to_string(), "10000".to_string()),
                ]));

                // let (narrative_logs, exposure_logs) = tokio::join!(
                //     narrative_log::NarrativeLog::retrieve(base_url, &params),
                //     narrative_log::NarrativeLog::retrieve(base_url, &params)
                // );
                if let Ok(narrative_logs) =
                    narrative_log::NarrativeLog::retrieve(base_url, &params).await
                {
                    println!("number of messages: {}.", narrative_logs.len());
                    let template = LogForm {
                        logmessages: narrative_logs
                            .into_iter()
                            .map(|narrative_log| {
                                narrative_log
                                    .render()
                                    .unwrap_or("Failed to render message.".to_string())
                            })
                            .collect(),
                    };
                    HtmlTemplate(template)
                } else {
                    get_empty_log_form()
                }
            }
            Err(error) => {
                println!("Error parsing selected date: {selected_date}. Err: {error}");
                get_empty_log_form()
            }
        }
    } else {
        get_empty_log_form()
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

#[derive(Template)]
#[template(path = "log_explorer.html")]
struct LogExplorerTemplate;

#[derive(Template)]
#[template(path = "night_plan.html")]
struct NighPlanTemplate;

#[derive(Template)]
#[template(path = "eon_report.html")]
struct EndOfNightReportTemplate;

#[derive(Template)]
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
