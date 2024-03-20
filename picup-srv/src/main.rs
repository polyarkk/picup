use std::sync::Arc;
use std::time::Duration;

use axum::{
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    serve, Router,
};

use clap::{arg, command, ArgAction};

use picup_lib::{api, ResponseCode, RestResponse, UploadImgParam};
use tokio::{
    fs::{create_dir, create_dir_all, remove_dir_all, rename, try_exists, File},
    io::AsyncWriteExt,
    net::TcpListener,
    signal::ctrl_c,
};

use tokio_util::io::ReaderStream;
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
        response_no(ResponseCode::NOT_IMPLEMENTED, &format!("not implemented: {}", $s))
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

#[derive(Debug)]
struct SrvState {
    allow_non_image_content: bool,
    categories: Vec<String>,
    access_token: String,
    pic_url_prefix: String,
    pic_directory: String,
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

    if !state.categories.contains(category) {
        return response_no(ResponseCode::INVALID_CATEGORY, "invalid category");
    }

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

        if !state.allow_non_image_content && !field.content_type().unwrap().contains("image") {
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
            uri_concat!(&state.pic_directory, category, &file_name),
        )
        .await
        .unwrap();

        image_urls.push(uri_concat!(
            &state.pic_url_prefix,
            "/asset",
            category,
            &encode(&file_name)
        ));
    }

    response_ok(image_urls)
}

async fn get_img(
    State(state): State<Arc<SrvState>>,
    Path((category, file_name)): Path<(String, String)>,
    Query(compress): Query<Option<u8>>,
) -> (StatusCode, Body) {
    if !state.categories.contains(&category) {
        return (StatusCode::NOT_FOUND, Body::empty());
    }

    let file = File::open(uri_concat!(&state.pic_directory, &category, &file_name)).await;

    if file.is_err() {
        return (StatusCode::NOT_FOUND, Body::empty());
    }


    let stream = ReaderStream::new(file.unwrap());

    if compress.is_some() {
        let compress = compress.unwrap();

        if compress != 0 {
            return (StatusCode::NOT_IMPLEMENTED, Body::empty());
        }
    }

    (StatusCode::OK, Body::from_stream(stream))
}

async fn get_img_urls(
    State(_state): State<Arc<SrvState>>,
    Path(_category): Path<String>,
    Query(
        (_page, _limit, _precache)
    ): Query<(String, String, Option<bool>)>,
) -> JRestResponse<Vec<String>> {
    api_todo!()
}

#[tokio::main]
async fn main() {
    let matches = command!()
        .args(&[
            arg!(-o --timeout <sec>         "Seconds before timeout for each requests. Default: 30"),
            arg!(-c --category [name]       "Names for categories.")
                .action(ArgAction::Append)
                .required(true),
            arg!(-n --"allow-all-files"     "Files those are not images can also be uploaded.")
                .action(ArgAction::SetTrue),
            arg!(-t --token <token>         "Token for access to uploading images to the server.")
                .required(true),
            arg!(-d --dir <dir>             "Directory where stores images.")
                .required(true),
            arg!(-p --port <port>           "Port that the server listens to. If not given, 19190 will be used."),
            arg!(-u --url <url>             "Url that will be used on responding to users the location of images. If not given, it will use api url in-built. It's usually be used for nginx with proxy_pass.")
        ])
        .get_matches();

    let categories = matches
        .get_many::<String>("category")
        .unwrap_or_default()
        .map(|str_ref| str_ref.to_owned())
        .collect::<Vec<String>>();

    let allow_non_image_content = matches.get_flag("allow-all-files");

    let port = matches.get_one::<u16>("port").unwrap_or(&19190);

    let access_token = matches.get_one::<String>("token").unwrap().to_owned();

    let dir = matches
            .get_one::<String>("dir")
            .expect("image directory is not specified")
            .to_owned().to_string();

    let pic_url_prefix = format!(
        "{}{}",
        match matches.get_one::<String>("url") {
            Some(pic_url_prefix) => pic_url_prefix.to_owned(),
            None => format!("http://127.0.0.1:{}", port),
        },
        api!("")
    );

    let state = Arc::new(SrvState {
        allow_non_image_content,
        categories,
        access_token,
        pic_url_prefix,
        pic_directory: dir.to_owned(),
    });

    create_dir_all(&state.pic_directory).await.unwrap();
    create_dir_all(uri_concat!(&state.pic_directory, "temp")).await.unwrap();

    for category in &state.categories {
        create_dir_all(uri_concat!(&state.pic_directory, category))
            .await
            .unwrap();
    }

    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();

    let app = Router::new()
        .route(api!("/upload"), post(upload_img))
        .route(api!("/asset/:category/:file_name"), get(get_img))
        .route(api!("/category/:category"), get(get_img_urls))
        .with_state(state)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .layer(TimeoutLayer::new(Duration::from_secs(
            *matches.get_one::<u64>("timeout").unwrap_or(&30),
        )));

    info!(
        "PicUp server is now listening to port {}. Ctrl+C to stop the server.",
        port
    );

    let listener = TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .unwrap();

    serve(listener, app.into_make_service())
        .with_graceful_shutdown(async {
            ctrl_c().await.unwrap();
            info!("PicUp server is not shutting down!");
        })
        .await
        .unwrap();
}

async fn truncate_temp(state: &Arc<SrvState>) {
    let temp_dir = uri_concat!(&state.pic_directory, "temp");
    remove_dir_all(&temp_dir).await.unwrap();
    create_dir(&temp_dir).await.unwrap();
}
