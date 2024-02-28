use axum::{http::StatusCode, Json};
use reqwest::blocking::{multipart::Form, Client};
use serde::{Deserialize, Serialize};

pub type Error = Box<dyn std::error::Error>;
pub type Result<T> = std::result::Result<T, Error>;

#[macro_export]
macro_rules! str(
    ($s:expr) => ( String::from( $s ) );
);

#[macro_export]
macro_rules! api {
    ($s:expr) => {
        format!("/picup{}", $s).as_str()
    };
}

#[derive(Serialize, Deserialize)]
pub struct RestResponse<TData> {
    status: u16,
    msg: String,
    data: Option<TData>,
}

impl<TData> RestResponse<TData> {
    fn ok(urls: TData) -> Self {
        RestResponse {
            status: StatusCode::OK.as_u16(),
            msg: str!("ok"),
            data: Some(urls),
        }
    }

    fn no(msg: String) -> Self {
        RestResponse {
            status: StatusCode::BAD_REQUEST.as_u16(),
            msg: msg,
            data: None,
        }
    }

    pub fn res_ok(urls: TData) -> (StatusCode, Json<Self>) {
        (StatusCode::OK, Json(Self::ok(urls)))
    }

    pub fn res_no(msg: &str) -> (StatusCode, Json<Self>) {
        (StatusCode::BAD_REQUEST, Json(Self::no(str!(msg))))
    }

    pub fn status(&self) -> u16 {
        self.status
    }

    pub fn msg(&self) -> &String {
        &self.msg
    }

    pub fn data(self) -> Option<TData> {
        self.data
    }
}

/**
   Uploads images to the PicUp server using `/upload` API.

   `base_url`: Base url of the API. On calling, the url will be: `{baseurl}/picup/upload`.

   `token`: Access token for the API.

   `file_paths`: Paths of image files.

   If OK, the function will return a vector of urls for images uploaded, or error messages otherwise.

   ```rust
    let url = "http://127.0.0.1:19190";
    let token = "baka";
    let file_paths = vec!["/path/to/img1", "/path/to/img2"];

    let result = picup(url, token, &file_paths);
   ```
*/
pub fn picup<TPath>(base_url: &str, token: &str, file_paths: &[TPath]) -> Result<Vec<String>>
where
    TPath: AsRef<std::path::Path>,
{
    let client = Client::new();

    let mut form = Form::new();

    for path in file_paths {
        form = form.file("file", path).expect("invalid file path");
    }

    let res = client
        .post(format!("{}{}", base_url, api!("/upload")))
        .query(&[("access_token", token)])
        .multipart(form)
        .send()
        .unwrap()
        .json::<RestResponse<Vec<String>>>()
        .expect("request error");

    if res.status() != StatusCode::OK {
        return Err(Error::from(res.msg().as_str()));
    }

    return Ok(res.data().take().unwrap());
}
