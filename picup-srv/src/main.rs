use std::{collections::HashMap, sync::Arc};

use axum::{
    body::Body,
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    serve, Router,
};

use clap::{arg, command};

use picup_lib::{api, RestResponse};
use tokio::{
    fs::{create_dir, create_dir_all, remove_dir_all, rename, File},
    io::AsyncWriteExt,
    net::TcpListener,
};

use tokio_util::io::ReaderStream;
use urlencoding::encode;

struct SrvState {
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
    param: Query<HashMap<String, String>>,
    mut multipart: Multipart,
) -> (StatusCode, Json<RestResponse<Vec<String>>>) {
    truncate_temp(&state).await;

    let given_token = param.get("access_token");

    if given_token.is_none() || given_token.unwrap() != &state.access_token {
        return RestResponse::res_no("invalid token");
    }

    let mut image_urls = Vec::new();
    let mut file_names = Vec::new();

    while let Some(field) = multipart.next_field().await.unwrap() {
        if !field.content_type().unwrap().contains("image") {
            return RestResponse::res_no("not a image for one of images");
        }

        let file_name = field.file_name();

        if file_name.is_none() {
            return RestResponse::res_no("no file name for one of images");
        }

        let file_name = file_name.unwrap().to_owned();

        image_urls.push(format!("{}{}", state.pic_url_prefix, encode(&file_name)));

        let bytes = field.bytes().await;

        if bytes.is_err() {
            return RestResponse::res_no("bad image for one of images");
        }

        let file_path = format!("{}{}", state.pic_temp_directory, file_name);

        let mut file = File::create(file_path).await.unwrap();

        let written = file.write_all(&bytes.unwrap()).await;

        if written.is_err() {
            return RestResponse::res_no("file write failed");
        }

        file_names.push(file_name);
    }

    for file_name in file_names {
        rename(
            format!("{}{}", state.pic_temp_directory, file_name),
            format!("{}{}", state.pic_directory, file_name),
        )
        .await
        .unwrap();
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
            arg!(-t --token <token>     "Token for access to uploading images to the picbed.")
                .required(true),
            arg!(-d --dir <dir>         "Directory where stores images.")
                .required(true),
            arg!(-p --port <port>       "Port that the server listens to. If not given, 19190 will be used."),
            arg!(-u --url <url>         "Url that will be used on responsing to users the location of images. If not given, it will use api url in-built. It's usually be used for nginx with proxy_pass.")
        ])
        .get_matches();

    let port = matches.get_one::<u16>("port").unwrap_or(&19190);

    let access_token = matches.get_one::<String>("token").unwrap().to_owned();

    let dir = format!("{}/", matches
        .get_one::<String>("dir")
        .expect("image directory is not specified")
        .to_owned()
    );

    let pic_url_prefix = format!("{}{}", match matches.get_one::<String>("url") {
        Some(pic_url_prefix) => pic_url_prefix.to_owned(),
        None => format!("http://127.0.0.1:{}", port),
    }, api!("/pic/"));

    let state = Arc::new(SrvState {
        access_token,
        pic_url_prefix,
        pic_directory: dir.to_owned(),
        pic_temp_directory: format!("{}/temp/", dir),
    });

    create_dir_all(&state.pic_directory).await.unwrap();
    create_dir_all(&state.pic_temp_directory).await.unwrap();

    let app = Router::new()
        .route(api!("/upload"), post(upload_img))
        .route(api!("/pic/:file_name"), get(get_img))
        .with_state(state);

    let listener = TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .unwrap();

    println!(
        "PicUp is now listening to port {}.\nCtrl+C to stop the server.",
        port
    );

    serve(listener, app).await.unwrap();
}

async fn truncate_temp(state: &Arc<SrvState>) {
    let temp_dir = &state.pic_temp_directory;
    remove_dir_all(temp_dir).await.unwrap();
    create_dir(temp_dir).await.unwrap();
}
