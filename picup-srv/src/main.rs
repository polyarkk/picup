use std::path::PathBuf;
use std::time::Duration;
use std::{collections::HashMap, sync::Arc};
use std::{env, process};

use axum::extract::DefaultBodyLimit;
use axum::{
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    serve, Router,
};

use picup_lib::{GetImgParam, ResponseCode, RestResponse, UploadImgParam, API_BASE_URL};
use tokio::io::{self, AsyncReadExt};
use tokio::{
    fs::{create_dir, create_dir_all, remove_dir_all, rename, try_exists, File},
    io::AsyncWriteExt,
    net::TcpListener,
    signal::ctrl_c,
};

use tokio_util::io::ReaderStream;
use toml::Table;
use tower_http::cors::CorsLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::{info, Level};
use urlencoding::encode;

macro_rules! uri_concat {
    ($base: expr, $( $s: expr ),*) => {
        {
            let mut uri = $base.to_string();
            $(
                uri.push('/');
                uri.push_str($s);
            )*
            uri
        }
    };
}

macro_rules! api_todo {
    () => {
        response_no(ResponseCode::NOT_IMPLEMENTED, "not implemented")
    };
    ( $s: expr ) => {
        response_no(
            ResponseCode::NOT_IMPLEMENTED,
            &format!("not implemented: {}", $s),
        )
    };
}

type JRestResponse<TData> = (StatusCode, Json<RestResponse<TData>>);

trait JsonResponse {
    fn response(status: StatusCode, s: Self) -> (StatusCode, Json<Self>)
    where
        Self: Sized;
}

impl<TData> JsonResponse for RestResponse<TData> {
    fn response(status: StatusCode, s: Self) -> JRestResponse<TData>
    where
        Self: Sized,
    {
        (status, Json(s))
    }
}

fn _response_ok_no_data() -> JRestResponse<()> {
    RestResponse::response(
        StatusCode::OK,
        RestResponse::new_no_data(ResponseCode::OK, "ok"),
    )
}

fn response_ok<TData>(data: TData) -> JRestResponse<TData> {
    RestResponse::response(
        StatusCode::OK,
        RestResponse::new(ResponseCode::OK, "ok", data),
    )
}

fn response_no<TData>(code: ResponseCode, msg: &str) -> JRestResponse<TData> {
    RestResponse::response(
        StatusCode::BAD_REQUEST,
        RestResponse::new_no_data(code, msg),
    )
}

struct SrvState {
    categories: HashMap<String, CategoryConfig>,
    access_token: String,
    pic_url_prefix: String,
    pic_directory: String,
}

struct CategoryConfig {
    allow_non_image_content: bool,
}

async fn upload_img(
    State(state): State<Arc<SrvState>>,
    param: Query<UploadImgParam>,
    mut multipart: Multipart,
) -> JRestResponse<Vec<String>> {
    truncate_temp(&state).await;

    let param = param.0;

    let r#override = param.r#override();

    if param.access_token() != &state.access_token {
        return response_no(ResponseCode::INVALID_TOKEN, "invalid token");
    }

    let mut file_names = Vec::new();

    let category = param.category();

    let category_config = state.categories.get(category);

    if category_config.is_none() {
        return response_no(ResponseCode::INVALID_CATEGORY, "invalid category");
    }

    let category_config = category_config.unwrap();

    // todo compress image when uploading
    let compress = param.compress();

    if compress != 0 {
        return api_todo!("compress");
    }

    let mut handled = 0;

    while let Some(field) = multipart.next_field().await.unwrap() {
        let file_name = field.file_name();

        if file_name.is_none() {
            return response_no(
                ResponseCode::BAD_FILE_NAME,
                &format!("invalid file name, file no: {}", handled + 1),
            );
        }

        let file_name = file_name.unwrap().to_owned();

        if !category_config.allow_non_image_content
            && !field.content_type().unwrap().contains("image")
        {
            return response_no(
                ResponseCode::NOT_A_IMAGE,
                &format!("not a image: {}", file_name),
            );
        }

        let file_path = uri_concat!(&state.pic_directory, category, &file_name);

        let exists = try_exists(&file_path).await;

        if exists.is_err() {
            return response_no(ResponseCode::INTERNAL_ERROR, "internal file system error");
        }

        let exists = exists.unwrap();

        if !r#override && exists {
            return response_no(
                ResponseCode::FILE_EXISTED,
                &format!("file existed: {}", file_name),
            );
        }

        let bytes = field.bytes().await;

        if bytes.is_err() {
            return response_no(ResponseCode::BAD_FILE, &format!("bad file: {}", file_name));
        }

        let file_temp_path = uri_concat!(&state.pic_directory, "temp", &file_name);

        let mut file = File::create(file_temp_path).await.unwrap();

        let written = file.write_all(&bytes.unwrap()).await;

        if written.is_err() {
            return response_no(ResponseCode::INTERNAL_ERROR, "internal file system error");
        }

        file_names.push(file_name);
        handled += 1;
    }

    let mut image_urls = Vec::new();

    // promising all files should be successfully uploaded
    for file_name in file_names {
        rename(
            uri_concat!(&state.pic_directory, "temp", &file_name),
            uri_concat!(&state.pic_directory, "asset", category, &file_name),
        )
        .await
        .unwrap();

        image_urls.push(uri_concat!(
            &state.pic_url_prefix,
            "asset",
            category,
            &encode(&file_name)
        ));
    }

    response_ok(image_urls)
}

async fn get_img(
    State(state): State<Arc<SrvState>>,
    Path((category, file_name)): Path<(String, String)>,
    Query(param): Query<GetImgParam>,
) -> (StatusCode, Body) {
    if !state.categories.contains_key(&category) {
        return (StatusCode::NOT_FOUND, Body::empty());
    }

    let file = File::open(uri_concat!(
        &state.pic_directory,
        "asset",
        &category,
        &file_name
    ))
    .await;

    if file.is_err() {
        return (StatusCode::NOT_FOUND, Body::empty());
    }

    let stream = ReaderStream::new(file.unwrap());

    let compress = param.compress();

    if compress != 0 {
        return (StatusCode::NOT_IMPLEMENTED, Body::empty());
    }

    (StatusCode::OK, Body::from_stream(stream))
}

async fn get_img_urls(
    State(_state): State<Arc<SrvState>>,
    Path(_category): Path<String>,
    Query((_page, _limit, _precache)): Query<(String, String, Option<bool>)>,
) -> JRestResponse<Vec<String>> {
    api_todo!()
}

#[tokio::main]
async fn main() -> io::Result<()> {
    let dir = exe_path().join("picup-srv.toml");
    let dir_str = dir.to_str().unwrap().to_string();

    let mut file = File::open(dir).await.expect(&format!(
        "failed to find config file! it should be in [{:?}].",
        dir_str
    ));

    let mut cfg = String::new();
    file.read_to_string(&mut cfg).await?;

    let mut cfg = cfg.parse::<Table>().unwrap().remove("server").unwrap();
    let cfg = cfg.as_table_mut().unwrap();

    let timeout = cfg
        .remove("timeout")
        .unwrap_or(toml::Value::Integer(30))
        .as_integer()
        .unwrap()
        .try_into()
        .unwrap();

    let token = cfg.remove("token").expect("no token provided");
    let token = token.as_str().unwrap();

    let directory = cfg
        .remove("directory")
        .unwrap_or(toml::Value::String(dir_str));
    let directory = directory.as_str().unwrap();

    let port = cfg
        .remove("port")
        .unwrap_or(toml::Value::Integer(19190))
        .as_integer()
        .unwrap();

    let url = cfg
        .remove("url")
        .unwrap_or(toml::Value::String(format!("http://127.0.0.1:{}", port)));
    let url = url.as_str().unwrap();

    let mut categories = cfg.remove("categories").expect("no category provided");
    let categories = categories.as_table_mut().unwrap();

    let mut category_configs = HashMap::new();

    for (name, config) in categories {
        let config = config.as_table_mut().unwrap();

        category_configs.insert(
            name.to_owned(),
            CategoryConfig {
                allow_non_image_content: config
                    .remove("allow_all_files")
                    .unwrap_or(toml::Value::Boolean(false))
                    .as_bool()
                    .unwrap(),
            },
        );
    }

    let state = Arc::new(SrvState {
        categories: category_configs,
        access_token: token.to_string(),
        pic_url_prefix: format!("{}{}", url, API_BASE_URL),
        pic_directory: directory.to_string(),
    });

    create_dir_all(&state.pic_directory).await.unwrap();
    create_dir_all(uri_concat!(&state.pic_directory, "temp"))
        .await
        .unwrap();

    for category in state.categories.keys() {
        create_dir_all(uri_concat!(&state.pic_directory, "asset", category))
            .await
            .unwrap();
    }

    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();

    let app = Router::new()
        .nest(
            API_BASE_URL,
            Router::new()
                .route("/upload", post(upload_img))
                .route("/asset/:category/:file_name", get(get_img))
                .route("/category/:category", get(get_img_urls)),
        )
        .with_state(state)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .layer(TimeoutLayer::new(Duration::from_secs(timeout)))
        .layer(DefaultBodyLimit::max(1024 * 1024 * 1024 * 32)) // 32mb
        .layer(CorsLayer::very_permissive());

    info!(
        "PicUp server is now listening to port {}. Ctrl+C to stop the server.",
        port
    );

    let listener = TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .unwrap();

    serve(listener, app.into_make_service())
        .with_graceful_shutdown(sigterm())
        .await
        .unwrap();

    Ok(())
}

async fn sigterm() {
    let ctrl_c = async { ctrl_c().await.unwrap() };

    tokio::select! {
        _ = ctrl_c => {
            info!("PicUp server is now shutting down!");
            process::exit(0);
        }
    }
}

async fn truncate_temp(state: &Arc<SrvState>) {
    let temp_dir = uri_concat!(&state.pic_directory, "temp");
    remove_dir_all(&temp_dir).await.unwrap();
    create_dir(&temp_dir).await.unwrap();
}

fn exe_path() -> PathBuf {
    let mut path = env::current_exe().unwrap();

    path.pop();

    path
}
