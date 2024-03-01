use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    serve, Router,
};

use clap::{arg, command};

use picup_lib::{api, ResponseCode, RestResponse, UploadImgParam};
use tokio::{
    fs::{create_dir, create_dir_all, remove_dir_all, rename, try_exists, File},
    io::AsyncWriteExt,
    net::TcpListener,
    signal::ctrl_c,
};

use tokio_util::io::ReaderStream;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::{info, Level};
use urlencoding::encode;

trait Response<TData> {
    fn res_ok(data: TData) -> (StatusCode, Json<Self>)
    where
        Self: Sized;

    fn res_no(code: ResponseCode, msg: &str) -> (StatusCode, Json<Self>)
    where
        Self: Sized;
}

impl<TData> Response<TData> for RestResponse<TData> {
    fn res_ok(data: TData) -> (StatusCode, Json<Self>) {
        (StatusCode::OK, Json(Self::ok(data)))
    }

    fn res_no(code: ResponseCode, msg: &str) -> (StatusCode, Json<Self>) {
        (
            StatusCode::BAD_REQUEST,
            Json(Self::no(code, format!("{}", msg))),
        )
    }
}

struct SrvState {
    allow_non_image_content: bool,
    access_token: String,
    pic_url_prefix: String,
    pic_directory: String,
    pic_temp_directory: String,
}

///
/// Uploads one or more images to the picbed.
///
/// Request Form Data:
/// - imgs: Images
///
/// Response JSON:
/// - status: 200 if success, 400 if fail.
/// - msg: "ok" if success, fail reason if fail.
/// - urls: urls to get the image uploaded to the picbed. nothing if fail.
///
/// Note: If one of images failed to upload, other images will not be uploaded, too.
///
async fn upload_img(
    State(state): State<Arc<SrvState>>,
    param: Query<UploadImgParam>,
    mut multipart: Multipart,
) -> (StatusCode, Json<RestResponse<Vec<String>>>) {
    truncate_temp(&state).await;

    let param = param.0;

    let r#override = param.r#override();

    if param.access_token() != state.access_token {
        return RestResponse::res_no(ResponseCode::INVALID_TOKEN, "invalid token");
    }

    let mut file_names = Vec::new();

    let mut handled = 0;

    while let Some(field) = multipart.next_field().await.unwrap() {
        let file_name = field.file_name();

        if file_name.is_none() {
            return RestResponse::res_no(
                ResponseCode::BAD_FILE_NAME,
                &format!("invalid file name, file no: {}", handled + 1),
            );
        }

        let file_name = file_name.unwrap().to_owned();

        if !state.allow_non_image_content && !field.content_type().unwrap().contains("image") {
            return RestResponse::res_no(
                ResponseCode::NOT_A_IMAGE,
                &format!("not a image: {}", file_name),
            );
        }

        let file_path = format!("{}{}", state.pic_temp_directory, file_name);

        let exists = try_exists(&file_path).await;

        if exists.is_err() {
            return RestResponse::res_no(
                ResponseCode::INTERNAL_ERROR,
                "internal file system error",
            );
        }

        let exists = exists.unwrap();

        if !r#override && exists {
            return RestResponse::res_no(
                ResponseCode::FILE_EXISTED,
                &format!("file existed: {}", file_name),
            );
        }

        let bytes = field.bytes().await;

        if bytes.is_err() {
            return RestResponse::res_no(
                ResponseCode::BAD_FILE,
                &format!("bad file: {}", file_name),
            );
        }

        let mut file = File::create(file_path).await.unwrap();

        let written = file.write_all(&bytes.unwrap()).await;

        if written.is_err() {
            return RestResponse::res_no(
                ResponseCode::INTERNAL_ERROR,
                "internal file system error",
            );
        }

        file_names.push(file_name);
        handled += 1;
    }

    let mut image_urls = Vec::new();

    for file_name in file_names {
        rename(
            format!("{}{}", state.pic_temp_directory, file_name),
            format!("{}{}", state.pic_directory, file_name),
        )
        .await
        .unwrap();

        image_urls.push(format!("{}{}", state.pic_url_prefix, encode(&file_name)));
    }

    return RestResponse::res_ok(image_urls);
}

///
/// Gets a image to the picbed.
///
/// Request Path Variable:
/// - file_name: File name of the image.
///
/// Response Body:
/// - an image, or nothing if no image was found.
///
/// Response Status Codes:
/// - 200: found
/// - 404: not found
///
async fn get_img(
    State(state): State<Arc<SrvState>>,
    Path(file_name): Path<String>,
) -> (StatusCode, Body) {
    let file = File::open(format!("{}{}", state.pic_directory, file_name)).await;

    if file.is_err() {
        return (StatusCode::NOT_FOUND, Body::empty());
    }

    let stream = ReaderStream::new(file.unwrap());

    return (StatusCode::OK, Body::from_stream(stream));
}

#[tokio::main]
async fn main() {
    let matches = command!()
        .args(&[
            arg!(-n --allow-all-files   "Files those are not images can also be uploaded."),
            arg!(-t --token <token>     "Token for access to uploading images to the picbed.")
                .required(true),
            arg!(-d --dir <dir>         "Directory where stores images.")
                .required(true),
            arg!(-p --port <port>       "Port that the server listens to. If not given, 19190 will be used."),
            arg!(-u --url <url>         "Url that will be used on responsing to users the location of images. If not given, it will use api url in-built. It's usually be used for nginx with proxy_pass.")
        ])
        .get_matches();

    let allow_non_image_content = matches.get_flag("allow-all-files");

    let port = matches.get_one::<u16>("port").unwrap_or(&19190);

    let access_token = matches.get_one::<String>("token").unwrap().to_owned();

    let dir = format!(
        "{}/",
        matches
            .get_one::<String>("dir")
            .expect("image directory is not specified")
            .to_owned()
    );

    let pic_url_prefix = format!(
        "{}{}",
        match matches.get_one::<String>("url") {
            Some(pic_url_prefix) => pic_url_prefix.to_owned(),
            None => format!("http://127.0.0.1:{}", port),
        },
        api!("/pic/")
    );

    let state = Arc::new(SrvState {
        allow_non_image_content,
        access_token,
        pic_url_prefix,
        pic_directory: dir.to_owned(),
        pic_temp_directory: format!("{}/temp/", dir),
    });

    create_dir_all(&state.pic_directory).await.unwrap();
    create_dir_all(&state.pic_temp_directory).await.unwrap();

    tracing_subscriber::fmt()
        .with_target(false)
        .compact()
        .init();

    let app = Router::new()
        .route(api!("/upload"), post(upload_img))
        .route(api!("/pic/:file_name"), get(get_img))
        .with_state(state)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        );

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
    let temp_dir = &state.pic_temp_directory;
    remove_dir_all(temp_dir).await.unwrap();
    create_dir(temp_dir).await.unwrap();
}
